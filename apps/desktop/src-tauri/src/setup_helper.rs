use crate::elevation;
use crate::provider_process;
use crate::provider_runtime;
use crate::setup_protocol::{
    read_frame, write_frame, Destination, DirectTarget, OperationRequest, SourceImage,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(any(not(windows), test))]
use std::fs;
use std::fs::OpenOptions;
#[cfg(not(windows))]
use std::io::Read;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use zeroize::Zeroizing;

type Emitter = Arc<Mutex<Box<dyn Write + Send>>>;
fn emit(output: &Emitter, value: Value) {
    if let Ok(mut writer) = output.lock() {
        let _ = write_frame(&mut *writer, &value);
    }
}
fn provider_event(output: &Emitter, mut value: Value) {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if kind == "result" {
        return;
    }
    if let Some(fields) = value.as_object_mut() {
        fields.insert("protocol".into(), json!(1));
        fields.remove("protocolVersion");
        fields.insert("type".into(), json!("event"));
        if !fields.contains_key("bytes") {
            if let Some(bytes) = fields.get("completedBytes").cloned() {
                fields.insert("bytes".into(), bytes);
            }
        }
        if kind == "error" {
            // Only entry() emits terminal helper messages, after run() has
            // drained and reaped its child. A nested error must not make the
            // desktop close IPC while disk/volume cleanup is still active.
            fields.insert("stage".into(), json!("finishing"));
            fields.insert("cancelAvailable".into(), json!(false));
            fields.insert("providerError".into(), json!(true));
        }
        emit(output, value);
    }
}
fn stage(output: &Emitter, name: &str, message: &str, cancel_available: bool) {
    emit(
        output,
        json!({"protocol":1,"type":"event","stage":name,"message":message,"cancelAvailable":cancel_available}),
    );
}
fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Operation cancelled".into())
    } else {
        Ok(())
    }
}

pub fn entry(id: &str, parent: u32) -> Result<(), String> {
    let connection = elevation::connect(id, parent)?;
    let output = Arc::new(Mutex::new(connection.writer));
    let mut reader = BufReader::new(connection.reader);
    let mut envelope = read_frame(&mut reader)?.ok_or("Missing operation request")?;
    let records_directory = envelope
        .as_object_mut()
        .ok_or("Invalid operation request")?
        .remove("recordsDirectory")
        .map(serde_json::from_value::<PathBuf>)
        .transpose()
        .map_err(|e| e.to_string())?;
    if records_directory
        .as_ref()
        .is_some_and(|path| !path.is_absolute() || !path.is_dir())
    {
        return Err("Invalid operation records directory".into());
    }
    let request: OperationRequest = serde_json::from_value(envelope).map_err(|e| e.to_string())?;
    if request.protocol != 1 {
        return Err("Unsupported operation protocol".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let cancellation = Arc::clone(&cancel);
    let (confirm_sender, confirmations) = mpsc::channel();
    std::thread::spawn(move || loop {
        match read_frame(&mut reader) {
            Ok(Some(message)) if message.get("type").and_then(Value::as_str) == Some("cancel") => {
                cancellation.store(true, Ordering::Relaxed);
            }
            Ok(Some(message))
                if message.get("type").and_then(Value::as_str) == Some("confirmation") =>
            {
                if confirm_sender.send(message).is_err() {
                    break;
                }
            }
            Ok(Some(_)) => {
                cancellation.store(true, Ordering::Relaxed);
                break;
            }
            _ => {
                cancellation.store(true, Ordering::Relaxed);
                break;
            }
        }
    });
    match execute(
        request,
        records_directory.as_deref(),
        &output,
        Arc::clone(&cancel),
        &confirmations,
    ) {
        Ok(result) => {
            emit(
                &output,
                json!({"protocol":1,"type":"result","result":result}),
            );
            Ok(())
        }
        Err(error) => {
            emit(
                &output,
                json!({"protocol":1,"type":"error","message":error,"cancelled":cancel.load(Ordering::Relaxed)}),
            );
            Err(error)
        }
    }
}

fn confirm(
    output: &Emitter,
    receiver: &mpsc::Receiver<Value>,
    cancel: &AtomicBool,
    summary: &str,
    plan: &Value,
) -> Result<(), String> {
    cancelled(cancel)?;
    let binding = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(plan).map_err(|e| e.to_string())?)
    );
    emit(
        output,
        json!({"protocol":1,"type":"confirmation_required","binding":binding,"summary":summary,"plan":plan}),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(900);
    loop {
        cancelled(cancel)?;
        match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(response)
                if response.get("binding").and_then(Value::as_str) == Some(binding.as_str()) =>
            {
                return if response.get("approved").and_then(Value::as_bool) == Some(true) {
                    Ok(())
                } else {
                    Err("Operation was not confirmed".into())
                };
            }
            Ok(_) => return Err("Operation confirmation did not match the plan".into()),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Desktop disconnected before confirmation".into())
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if std::time::Instant::now() >= deadline {
            return Err("Operation confirmation expired".into());
        }
    }
}

struct PreparedSource {
    path: PathBuf,
    #[cfg(windows)]
    guard: crate::iso_source::HeldIso,
}

fn prepare_source(
    source: &SourceImage,
    directory: &Path,
    output: &Emitter,
    cancel: &AtomicBool,
) -> Result<PreparedSource, String> {
    cancelled(cancel)?;
    if !source.path.is_absolute()
        || source.length == 0
        || source.length > 64 * 1024 * 1024 * 1024
        || source.signature.is_empty()
        || source.signature.len() > 16384
        || source.file_name.len() > 128
        || !source.file_name.starts_with("omarchy-")
        || !source.file_name.ends_with(".iso")
        || !source
            .file_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || [b'.', b'-'].contains(&byte))
    {
        return Err("Invalid verified-image request".into());
    }
    #[cfg(windows)]
    let guard = crate::iso_source::HeldIso::open(&source.path, source.length)?;
    #[cfg(windows)]
    let path = guard.path().to_path_buf();
    // Other hosts retain protected staging until they have an equivalent
    // mandatory lifetime guard; advisory POSIX locks do not prevent changes.
    #[cfg(not(windows))]
    let path = {
        let mut cursor = PathBuf::new();
        for component in source.path.components() {
            cursor.push(component);
            if matches!(
                component,
                std::path::Component::Prefix(_) | std::path::Component::RootDir
            ) {
                continue;
            }
            if fs::symlink_metadata(&cursor)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
            {
                return Err(
                    "The original image path contains a symbolic link or directory junction".into(),
                );
            }
        }
        if !fs::symlink_metadata(&source.path)
            .map_err(|e| e.to_string())?
            .is_file()
        {
            return Err("The selected image is not a regular file".into());
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let mut input = options.open(&source.path).map_err(|e| e.to_string())?;
        let metadata = input.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() != source.length {
            return Err("The selected image changed or is not a regular file".into());
        }
        let path = directory.join(&source.file_name);
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let mut buffer = vec![0_u8; 1024 * 1024];
        let mut total = 0_u64;
        loop {
            cancelled(cancel)?;
            let read = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > source.length {
                return Err("The image grew while preparing it".into());
            }
            file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
            if total % (64 * 1024 * 1024) < buffer.len() as u64 {
                emit(
                    output,
                    json!({"protocol":1,"type":"event","stage":"preparing","message":"Preparing the verified image…","bytes":total,"totalBytes":source.length,"cancelAvailable":true}),
                );
            }
        }
        file.sync_all().map_err(|e| e.to_string())?;
        if total != source.length {
            return Err("The image ended before its expected size".into());
        }
        drop(file);
        path
    };
    stage(
        output,
        "authenticating",
        "Verifying the official image…",
        true,
    );
    omarchy_release_client::authenticate_iso_offline(
        &path,
        source.length,
        &source.sha256,
        &source.signature,
        cancel,
        |progress| {
            emit(output, json!({"protocol":1,"type":"event","stage":"authenticating",
                "message":"Verifying the official image…",
                "bytes":progress.received_bytes,"totalBytes":progress.total_bytes,"cancelAvailable":true}));
        },
    )
    .map_err(|e| e.to_string())?;
    let mut signature = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(format!("{}.sig", source.file_name)))
        .map_err(|e| e.to_string())?;
    signature
        .write_all(&source.signature)
        .map_err(|e| e.to_string())?;
    signature.sync_all().map_err(|e| e.to_string())?;
    Ok(PreparedSource {
        path,
        #[cfg(windows)]
        guard,
    })
}

fn safe_label(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or("Unknown device")
        .chars()
        .filter(|c| {
            !c.is_control()
                && !('\u{202a}'..='\u{202e}').contains(c)
                && !('\u{2066}'..='\u{2069}').contains(c)
        })
        .take(160)
        .collect()
}

#[cfg(all(test, windows))]
mod source_tests {
    use super::*;

    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn authentication_failure_and_cancellation_release_the_original_without_staging() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.iso");
        fs::write(&path, b"ISO").unwrap();
        let stage_dir = dir.path().join("operation");
        fs::create_dir(&stage_dir).unwrap();
        let source = SourceImage {
            path: path.clone(),
            file_name: "omarchy-test.iso".into(),
            length: 3,
            sha256: "0".repeat(64),
            signature: b"invalid signature".to_vec(),
        };
        let output: Emitter = Arc::new(Mutex::new(Box::new(std::io::sink())));
        for cancel in [false, true] {
            assert!(
                prepare_source(&source, &stage_dir, &output, &AtomicBool::new(cancel)).is_err()
            );
            assert_eq!(fs::read(&path).unwrap(), b"ISO");
            assert_eq!(fs::read_dir(&stage_dir).unwrap().count(), 0);
            assert!(OpenOptions::new().write(true).open(&path).is_ok());
        }
    }

    #[test]
    #[ignore = "requires an existing official ISO and detached signature; never writes to that ISO"]
    fn existing_official_iso_authenticates_in_place_with_progress() {
        let path = PathBuf::from(std::env::var_os("OMARCHY_TEST_ISO").expect("ISO path required"));
        let signature_path =
            std::env::var_os("OMARCHY_TEST_SIGNATURE").expect("Signature required");
        let release: Value = serde_json::from_str(include_str!(
            "../../../../providers/image-builder-x86/release-lock.json"
        ))
        .unwrap();
        let source = SourceImage {
            path: path.clone(),
            file_name: format!("omarchy-{}.iso", release["version"].as_str().unwrap()),
            length: release["sizeBytes"].as_u64().unwrap(),
            sha256: release["sha256"].as_str().unwrap().into(),
            signature: fs::read(signature_path).unwrap(),
        };
        let dir = tempfile::tempdir().unwrap();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let output: Emitter = Arc::new(Mutex::new(Box::new(Capture(Arc::clone(&captured)))));
        let timer = std::time::Instant::now();
        let prepared =
            prepare_source(&source, dir.path(), &output, &AtomicBool::new(false)).unwrap();
        assert_eq!(prepared.path, path);
        let staged = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            staged,
            vec![std::ffi::OsString::from(format!(
                "{}.sig",
                source.file_name
            ))]
        );
        let bytes = captured.lock().unwrap();
        let events = String::from_utf8_lossy(&bytes)
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert!(events
            .iter()
            .any(|e| e["bytes"].as_u64() == Some(source.length)));
        assert!(events.iter().any(|e| e["bytes"]
            .as_u64()
            .is_some_and(|n| n > 0 && n < source.length)));
        assert!(events.iter().all(|e| e["stage"] == "authenticating"));
        println!("Authenticated {} bytes at original path in {:.3}s; {} progress events; only signature staged", source.length, timer.elapsed().as_secs_f64(), events.len());
    }
}

fn write_request(directory: &Path, name: &str, value: &Value) -> Result<PathBuf, String> {
    let path = directory.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > crate::setup_protocol::MAX_FRAME {
        return Err("Operation request is too large".into());
    }
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(path)
}

fn revalidate_usb(
    runtime: &Path,
    manifest: &provider_runtime::RuntimeManifest,
    workspace: &Path,
    source: &Path,
    identity: &Value,
    cancel: &Arc<AtomicBool>,
    output: &Emitter,
) -> Result<(), String> {
    cancelled(cancel)?;
    let mut command = provider_process::media_command(runtime, manifest)?;
    provider_process::privileged_environment(&mut command, workspace)?;
    let listed = provider_process::run(
        command,
        Some(json!({"protocol":1,"action":"list","sourcePath":source})),
        Arc::clone(cancel),
        None,
        false,
        |value| provider_event(output, value),
    )?;
    let fingerprint = identity
        .get("fingerprint")
        .and_then(Value::as_str)
        .ok_or("No USB identity was selected")?;
    let drives = listed
        .get("drives")
        .and_then(Value::as_array)
        .ok_or("Provider did not return drive discovery")?;
    let matches: Vec<_> = drives
        .iter()
        .filter(|drive| {
            drive
                .pointer("/identity/fingerprint")
                .and_then(Value::as_str)
                == Some(fingerprint)
        })
        .collect();
    if matches.len() != 1 {
        return Err("The selected USB drive is no longer uniquely identified".into());
    }
    let drive = matches[0];
    if drive.get("eligible").and_then(Value::as_bool) != Some(true)
        || drive.get("identity") != Some(identity)
    {
        return Err("The USB drive changed, contains the original or staged image, or is no longer eligible".into());
    }
    Ok(())
}

fn execute(
    request: OperationRequest,
    records_directory: Option<&Path>,
    output: &Emitter,
    cancel: Arc<AtomicBool>,
    confirmations: &mpsc::Receiver<Value>,
) -> Result<Value, String> {
    let manifest = provider_runtime::manifest()?;
    let runtime_source = provider_runtime::locate(&manifest)?;
    let id = uuid::Uuid::new_v4().to_string();
    let workspace = elevation::protected_directory(None, &format!("Omarchy-Setup-{id}"))?;
    let outcome: Result<Value, String> = (|| {
        let runtime = elevation::protected_directory(Some(&workspace), "providers")?;
        stage(
            output,
            "preparing",
            "Preparing the installation tools…",
            true,
        );
        provider_runtime::copy_protected(&runtime_source, &runtime, &manifest, &cancel)?;
        let mut protected_paths = vec![
            request.source.path.clone(),
            std::env::current_exe().map_err(|e| e.to_string())?,
            workspace.clone(),
        ];
        if let Some(directory) = records_directory {
            protected_paths.push(directory.to_path_buf());
            if let Destination::Usb { identity } | Destination::UsbPreserve { identity, .. } =
                &request.destination
            {
                revalidate_usb(
                    &runtime, &manifest, &workspace, directory, identity, &cancel, output,
                )?;
            }
        }
        if let Destination::InspectUsb { identity } = &request.destination {
            revalidate_usb(
                &runtime,
                &manifest,
                &workspace,
                &request.source.path,
                identity,
                &cancel,
                output,
            )?;
            let inspection = crate::usb_preserve::inspect(
                identity,
                request.source.length,
                &request.source.sha256,
            );
            return Ok(json!({"usbInspection":inspection}));
        }
        if let Destination::UsbPreserve { identity, plan } = &request.destination {
            revalidate_usb(
                &runtime,
                &manifest,
                &workspace,
                &request.source.path,
                identity,
                &cancel,
                output,
            )?;
            let fresh = crate::usb_preserve::recheck(identity, plan)?;
            crate::usb_preserve::validate_addition(
                &fresh,
                request.source.length,
                &request.source.sha256,
            )?;
            let action = "Keep existing files and add the Omarchy installer";
            let summary = format!("{action} on {}?\n\nDevice: {}\nImage: {}\nExisting files and partitions will be retained. The ISO needs {:.2} GiB plus a safety reserve.\n\nIf a previous UEFI bootloader exists, it will be backed up on this USB before replacement. Its previous boot option will be replaced. Automatic restoration is not currently available.\n\nThis installer boots on x64 UEFI with Secure Boot off.",safe_label(identity.get("description")),safe_label(identity.get("device")),request.source.file_name,request.source.length as f64/1_073_741_824.0);
            confirm(
                output,
                confirmations,
                &cancel,
                &summary,
                &crate::setup_protocol::usb_confirmation_plan(&request)?
                    .ok_or("Missing USB approval plan")?,
            )?;
            revalidate_usb(
                &runtime,
                &manifest,
                &workspace,
                &request.source.path,
                identity,
                &cancel,
                output,
            )?;
            let receipt = {
                let source = prepare_source(&request.source, &workspace, output, &cancel)?;
                revalidate_usb(
                    &runtime,
                    &manifest,
                    &workspace,
                    &request.source.path,
                    identity,
                    &cancel,
                    output,
                )?;
                if let Some(directory) = records_directory {
                    revalidate_usb(
                        &runtime, &manifest, &workspace, directory, identity, &cancel, output,
                    )?;
                }
                crate::usb_preserve::install(
                    identity,
                    plan,
                    &source.path,
                    request.source.length,
                    &request.source.sha256,
                    &runtime.join("usb-preserve/boot/BOOTX64.EFI"),
                    &cancel,
                    |mut value| {
                        value["protocol"] = json!(1);
                        value["type"] = json!("event");
                        emit(output, value);
                    },
                )?
            };
            let receipt_path = write_request(
                &workspace,
                "receipt.json",
                &json!({"operationId":id,"destination":request.destination,"receipt":receipt}),
            )?;
            return Ok(json!({"receipt":receipt,"receiptPath":receipt_path}));
        }
        if matches!(request.destination, Destination::PrepareFirmware) {
            let info_path = write_request(&workspace, "firmware-info.json", &json!({}))?;
            let mut command =
                provider_process::direct_command(&runtime, &manifest, "firmware-info", &info_path)?;
            provider_process::privileged_environment(&mut command, &workspace)?;
            let info =
                provider_process::run(command, None, Arc::clone(&cancel), None, false, |_| {})?;
            let hash = info["sha256"]
                .as_str()
                .filter(|hash| hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()))
                .ok_or("Invalid firmware preparation plan")?;
            confirm(output, confirmations, &cancel,
                "Restart this computer into UEFI firmware settings now? Save your work and have your Windows recovery key available. The helper will temporarily suspend active Windows OS BitLocker protection for this transition and arrange restoration when Windows returns. Existing suspension is preserved. In firmware, disable Secure Boot, leave TPM enabled, save, and return to Windows. Reopen Omarchy Setup and check with administrator access before installing. No partitions are changed by this step.", &info)?;
            let path = write_request(
                &workspace,
                "firmware-request.json",
                &json!({"operationId":id,"expectedSha256":hash}),
            )?;
            let mut command =
                provider_process::direct_command(&runtime, &manifest, "prepare-firmware", &path)?;
            provider_process::privileged_environment(&mut command, &workspace)?;
            stage(
                output,
                "firmware-preparation",
                "Preparing Windows protection and restarting into firmware settings…",
                false,
            );
            return provider_process::run(
                command,
                None,
                Arc::clone(&cancel),
                None,
                false,
                |mut value| {
                    if let Some(fields) = value.as_object_mut() {
                        fields.insert("cancelAvailable".into(), json!(false));
                    }
                    provider_event(output, value)
                },
            );
        }
        if matches!(request.destination, Destination::PrepareRuntime) {
            confirm(output, confirmations, &cancel,
                "Import the verified, packaged Linux construction runtime into Docker? Docker Desktop must already be installed and running with its Linux engine. This imports the application’s pinned image; it does not change Windows features, disks, firmware or BitLocker.", &json!({"action":"prepare-runtime","archiveSha256":manifest.files.iter().find(|file| file.path == "image-builder-x86/runtime.tar").map(|file| &file.sha256)}))?;
            let path = write_request(&workspace, "runtime-request.json", &json!({}))?;
            let mut command =
                provider_process::direct_command(&runtime, &manifest, "prepare-runtime", &path)?;
            provider_process::privileged_environment(&mut command, &workspace)?;
            stage(
                output,
                "runtime-preparation",
                "Importing the verified construction runtime…",
                false,
            );
            return provider_process::run(
                command,
                None,
                Arc::clone(&cancel),
                None,
                false,
                |value| provider_event(output, value),
            );
        }
        if matches!(request.destination, Destination::InspectDirect) {
            let probe_request = write_request(
                &workspace,
                "probe-request.json",
                &json!({"protectedPaths":protected_paths}),
            )?;
            let mut command =
                provider_process::direct_command(&runtime, &manifest, "probe", &probe_request)?;
            provider_process::privileged_environment(&mut command, &workspace)?;
            let probe =
                provider_process::run(command, None, Arc::clone(&cancel), None, false, |value| {
                    provider_event(output, value)
                })?;
            return Ok(json!({"inspection":probe}));
        }
        // Expensive construction starts only after elevated target/prerequisite
        // inspection; a selected extent never overrides the provider's own policy.
        if let Destination::DirectX86 {
            disk_number,
            disk_unique_id,
            target,
            allocation_bytes,
            boot_menu,
        } = &request.destination
        {
            boot_menu.validate()?;
            let probe_request = write_request(
                &workspace,
                "probe-request.json",
                &json!({"protectedPaths":protected_paths}),
            )?;
            let mut command =
                provider_process::direct_command(&runtime, &manifest, "probe", &probe_request)?;
            provider_process::privileged_environment(&mut command, &workspace)?;
            let probe =
                provider_process::run(command, None, Arc::clone(&cancel), None, false, |value| {
                    provider_event(output, value)
                })?;
            if let Some(missing) = probe
                .get("prerequisites")
                .and_then(Value::as_array)
                .and_then(|items| items.iter().find(|item| item["available"] != true))
            {
                return Err(missing["message"]
                    .as_str()
                    .unwrap_or("A construction prerequisite is missing")
                    .to_owned());
            }
            let disk = probe
                .get("disks")
                .and_then(Value::as_array)
                .and_then(|disks| {
                    disks.iter().find(|disk| {
                        disk["diskNumber"].as_u64() == Some(u64::from(*disk_number))
                            && disk["diskUniqueId"].as_str() == Some(disk_unique_id.as_str())
                    })
                })
                .ok_or("The selected disk identity changed")?;
            if disk["eligible"] != true {
                return Err(format!(
                    "The selected disk is not eligible: {}",
                    disk["blockers"]
                ));
            }
            let minimum = probe["minimumUnallocatedBytes"]
                .as_u64()
                .ok_or("The provider did not report its required space")?;
            if *allocation_bytes < minimum || allocation_bytes % 1_048_576 != 0 {
                return Err("The selected installation size is invalid".into());
            }
            if probe["bitLocker"]["known"] != true
                || !probe["bitLocker"]["blockers"]
                    .as_array()
                    .is_some_and(Vec::is_empty)
            {
                return Err("BitLocker status is unavailable or prevents this installation".into());
            }
            let valid = match target {
                DirectTarget::Free { start_offset_bytes } => {
                    disk["freeExtents"].as_array().is_some_and(|extents| {
                        extents.iter().any(|extent| {
                            extent["offsetBytes"].as_u64() == Some(*start_offset_bytes)
                                && extent["sizeBytes"]
                                    .as_u64()
                                    .is_some_and(|size| size >= *allocation_bytes)
                        })
                    })
                }
                DirectTarget::Shrink {
                    partition_number,
                    partition_guid,
                } => disk["shrinkCandidates"]
                    .as_array()
                    .is_some_and(|candidates| {
                        candidates.iter().any(|candidate| {
                            candidate["eligible"] == true
                                && candidate["partitionNumber"].as_u64()
                                    == Some(u64::from(*partition_number))
                                && candidate["partitionGuid"].as_str()
                                    == Some(partition_guid.as_str())
                                && candidate["maximumAllocationBytes"]
                                    .as_u64()
                                    .is_some_and(|maximum| maximum >= *allocation_bytes)
                        })
                    }),
                DirectTarget::Delete {
                    partition_number,
                    partition_guid,
                    start_offset_bytes,
                    partition_size_bytes,
                    confirmation,
                } => {
                    let expected = format!("Disk {disk_number} Partition {partition_number}");
                    confirmation == &expected
                        && disk["deleteCandidates"]
                            .as_array()
                            .is_some_and(|candidates| {
                                candidates.iter().any(|candidate| {
                                    candidate["eligible"] == true
                                        && candidate["partitionNumber"].as_u64()
                                            == Some(u64::from(*partition_number))
                                        && candidate["partitionGuid"].as_str()
                                            == Some(partition_guid.as_str())
                                        && candidate["offsetBytes"].as_u64()
                                            == Some(*start_offset_bytes)
                                        && candidate["sizeBytes"].as_u64()
                                            == Some(*partition_size_bytes)
                                        && candidate["maximumAllocationBytes"]
                                            .as_u64()
                                            .is_some_and(|maximum| maximum >= *allocation_bytes)
                                })
                            })
                }
            };
            if !valid {
                return Err(
                    "The selected installation space or shrink limit changed; refresh the choices"
                        .into(),
                );
            }
        }
        let source = prepare_source(&request.source, &workspace, output, &cancel)?;
        let callback = |value: Value| provider_event(output, value);
        let receipt = match &request.destination {
            Destination::InspectUsb { .. }
            | Destination::UsbPreserve { .. }
            | Destination::InspectDirect
            | Destination::PrepareFirmware
            | Destination::PrepareRuntime => return Err("Unexpected preparation state".into()),
            Destination::Usb { identity } => {
                // The original source disk remains excluded throughout writing.
                revalidate_usb(
                    &runtime,
                    &manifest,
                    &workspace,
                    &request.source.path,
                    identity,
                    &cancel,
                    output,
                )?;
                if source.path != request.source.path {
                    revalidate_usb(
                        &runtime,
                        &manifest,
                        &workspace,
                        &source.path,
                        identity,
                        &cancel,
                        output,
                    )?;
                }
                let sector = identity
                    .get("blockSize")
                    .and_then(Value::as_u64)
                    .filter(|n| (512..=4096).contains(n) && n.is_power_of_two())
                    .ok_or("USB physical sector size is unavailable")?;
                let padding = (sector - request.source.length % sector) % sector;
                let span = request
                    .source
                    .length
                    .checked_add(padding)
                    .ok_or("USB image span overflow")?;
                if identity
                    .get("size")
                    .and_then(Value::as_u64)
                    .is_none_or(|capacity| span > capacity)
                {
                    return Err("The USB cannot hold the image and its final-sector padding".into());
                }
                let summary = format!("Erase all data on {}?\n\nDevice: {}\nCapacity: {} bytes\nImage: {}\n\nThe image will be written and verified. Other disks will not be modified.", safe_label(identity.get("description")), safe_label(identity.get("device")), identity.get("size").and_then(Value::as_u64).unwrap_or(0), request.source.file_name);
                let plan = crate::setup_protocol::usb_confirmation_plan(&request)?
                    .ok_or("Missing USB approval plan")?;
                confirm(output, confirmations, &cancel, &summary, &plan)?;
                revalidate_usb(
                    &runtime,
                    &manifest,
                    &workspace,
                    &request.source.path,
                    identity,
                    &cancel,
                    output,
                )?;
                stage(output, "writing", "Creating the bootable USB…", true);
                let mut command = provider_process::media_command(&runtime, &manifest)?;
                provider_process::privileged_environment(&mut command, &workspace)?;
                if let Some(directory) = records_directory {
                    revalidate_usb(
                        &runtime, &manifest, &workspace, directory, identity, &cancel, output,
                    )?;
                }
                let media_request = json!({"protocol":1,"action":"write","sourcePath":source.path,"length":request.source.length,"sha256":request.source.sha256,"target":identity});
                #[cfg(windows)]
                let media_request = {
                    let mut value = media_request;
                    // prepare_source authenticated both signature and SHA-256
                    // while holding this guard. It stays alive until run exits.
                    value["sourceVerification"] = source.guard.reader_binding()?;
                    value
                };
                let receipt = provider_process::run(
                    command,
                    Some(media_request),
                    Arc::clone(&cancel),
                    None,
                    true,
                    callback,
                )?;
                crate::setup_protocol::validate_usb_receipt(&request, &receipt)?;
                receipt
            }
            Destination::DirectX86 {
                disk_number,
                disk_unique_id,
                target,
                allocation_bytes,
                boot_menu,
            } => {
                let mut staging_key = Zeroizing::new([0_u8; 32]);
                getrandom::fill(staging_key.as_mut())
                    .map_err(|_| "Could not generate the private staging key")?;
                let output_directory = elevation::protected_directory(Some(&workspace), "image")?;
                let preliminary = json!({"kind":"direct_x86","diskNumber":disk_number,"diskUniqueId":disk_unique_id,"target":target,"allocationBytes":allocation_bytes,"bootMenu":boot_menu,"sourceSha256":request.source.sha256,"encryption":"luks2"});
                let space_description = match target {
                    DirectTarget::Free { .. } => "Use existing unallocated space.".to_owned(),
                    DirectTarget::Shrink { partition_number, .. } => format!("Make space by shrinking partition {partition_number}."),
                    DirectTarget::Delete { partition_number, partition_size_bytes, .. } => format!("Replace partition {partition_number} ({:.2} GiB). All its data will be deleted after final confirmation.", *partition_size_bytes as f64 / 1_073_741_824.0),
                };
                let boot_review = boot_menu.summary();
                let preliminary_summary = format!("Prepare Omarchy for Disk {disk_number}?\n\nSpace for Omarchy: {:.2} GiB\n{space_description}\n\nThis prepares Omarchy locally. You’ll review the final changes before installation begins.", *allocation_bytes as f64 / 1_073_741_824.0);
                confirm(
                    output,
                    confirmations,
                    &cancel,
                    &preliminary_summary,
                    &preliminary,
                )?;
                let build = json!({"operationId":id,"sourceIsoPath":source.path,"sourceSignaturePath":workspace.join(format!("{}.sig", request.source.file_name)),"outputDirectory":output_directory,"bootMenu":boot_menu});
                let build_request = write_request(&workspace, "build-request.json", &build)?;
                let mut command =
                    provider_process::direct_command(&runtime, &manifest, "build", &build_request)?;
                provider_process::privileged_environment(&mut command, &workspace)?;
                stage(
                    output,
                    "building",
                    "Building Omarchy from the official image…",
                    true,
                );
                let built = provider_process::run_with_staging_key(
                    command,
                    staging_key.as_ref(),
                    Arc::clone(&cancel),
                    Some(output_directory.join("cancel.requested")),
                    true,
                    callback,
                )?;
                cancelled(&cancel)?;
                let mut plan_input = json!({"operationId":id,"diskNumber":disk_number,"diskUniqueId":disk_unique_id,"allocationBytes":allocation_bytes,"manifestPath":built.get("manifestPath").ok_or("Builder did not return its manifest")?,"manifestSha256":built.get("manifestSha256").ok_or("Builder did not return its manifest digest")?,"encryption":"luks2"});
                plan_input["protectedPaths"] = json!(protected_paths);
                plan_input["bootMenu"] = json!(boot_menu);
                match target {
                    DirectTarget::Free { start_offset_bytes } => {
                        plan_input["targetKind"] = json!("free");
                        plan_input["startOffsetBytes"] = json!(start_offset_bytes);
                    }
                    DirectTarget::Shrink {
                        partition_number,
                        partition_guid,
                    } => {
                        plan_input["targetKind"] = json!("shrink");
                        plan_input["shrinkPartitionNumber"] = json!(partition_number);
                        plan_input["shrinkPartitionGuid"] = json!(partition_guid);
                    }
                    DirectTarget::Delete {
                        partition_number,
                        partition_guid,
                        start_offset_bytes,
                        partition_size_bytes,
                        confirmation,
                    } => {
                        plan_input["targetKind"] = json!("delete");
                        plan_input["deletePartitionNumber"] = json!(partition_number);
                        plan_input["deletePartitionGuid"] = json!(partition_guid);
                        plan_input["deleteOffsetBytes"] = json!(start_offset_bytes);
                        plan_input["deleteSizeBytes"] = json!(partition_size_bytes);
                        plan_input["deleteConfirmation"] = json!(confirmation);
                    }
                }
                let plan_request = write_request(&workspace, "plan-request.json", &plan_input)?;
                let mut command =
                    provider_process::direct_command(&runtime, &manifest, "plan", &plan_request)?;
                provider_process::privileged_environment(&mut command, &workspace)?;
                stage(
                    output,
                    "planning",
                    "Checking the final installation plan…",
                    false,
                );
                let planned = provider_process::run_with_staging_key(
                    command,
                    staging_key.as_ref(),
                    Arc::clone(&cancel),
                    None,
                    false,
                    callback,
                )?;
                crate::setup_protocol::validate_direct_plan(&request, &id, &built, &planned)?;
                let partitions = planned["partitions"]
                    .as_array()
                    .filter(|items| items.len() == 2)
                    .ok_or("The installation plan omitted its exact partitions")?;
                if planned["allocationBytes"].as_u64() != Some(*allocation_bytes) {
                    return Err("The provider changed the requested allocation".into());
                }
                let resize_review = match target {
                    DirectTarget::Free { .. } => {
                        "Existing partitions keep their current sizes.".to_owned()
                    }
                    DirectTarget::Shrink {
                        partition_number,
                        partition_guid,
                    } => {
                        let shrink = &planned["shrink"];
                        let before = shrink["beforeSizeBytes"]
                            .as_u64()
                            .ok_or("The plan omitted the current Windows size")?;
                        let after = shrink["afterSizeBytes"]
                            .as_u64()
                            .ok_or("The plan omitted the new Windows size")?;
                        let released = before
                            .checked_sub(after)
                            .ok_or("The plan did not shrink Windows")?;
                        let expected_after = before / 1_048_576 * 1_048_576;
                        let expected_after = expected_after
                            .checked_sub(*allocation_bytes)
                            .ok_or("The plan shrinks beyond the Windows partition")?;
                        if shrink["partitionNumber"].as_u64() != Some(u64::from(*partition_number))
                            || shrink["partitionGuid"].as_str() != Some(partition_guid.as_str())
                            || shrink["shrinkBytes"].as_u64() != Some(released)
                            || after != expected_after
                        {
                            return Err("The provider changed the selected shrink operation".into());
                        }
                        format!("Partition {partition_number} ({}): {:.2} GiB → {:.2} GiB.\nIts files are kept.", safe_label(shrink.get("driveLetter")), before as f64 / 1_073_741_824.0, after as f64 / 1_073_741_824.0)
                    }
                    DirectTarget::Delete {
                        partition_number,
                        partition_guid,
                        start_offset_bytes,
                        partition_size_bytes,
                        confirmation,
                    } => {
                        let deletion = &planned["delete"];
                        if planned["targetKind"] != "delete"
                            || !planned["shrink"].is_null()
                            || deletion["partitionNumber"].as_u64()
                                != Some(u64::from(*partition_number))
                            || deletion["partitionGuid"].as_str() != Some(partition_guid.as_str())
                            || deletion["offsetBytes"].as_u64() != Some(*start_offset_bytes)
                            || deletion["sizeBytes"].as_u64() != Some(*partition_size_bytes)
                            || deletion["confirmation"].as_str() != Some(confirmation.as_str())
                            || deletion["allocationBytes"].as_u64() != Some(*allocation_bytes)
                            || partitions[0]["offsetBytes"].as_u64() != Some(*start_offset_bytes)
                        {
                            return Err(
                                "The provider changed the confirmed partition deletion".into()
                            );
                        }
                        format!("Delete Disk {disk_number} Partition {partition_number}: {} · {} · {:.2} GiB.\nAll its files and any operating system will be permanently lost, including space not allocated to Omarchy.", safe_label(deletion.get("label")), safe_label(deletion.get("fileSystem")), *partition_size_bytes as f64 / 1_073_741_824.0)
                    }
                };
                let suspend = planned["bitLocker"]["suspendVolumes"]
                    .as_array()
                    .ok_or("The plan omitted the BitLocker protection changes")?;
                let bitlocker_review = if suspend.is_empty() {
                    "No BitLocker protection changes are required.".to_owned()
                } else {
                    let names = suspend
                        .iter()
                        .map(|volume| {
                            safe_label(
                                volume
                                    .get("driveLetter")
                                    .filter(|value| {
                                        value.as_str().is_some_and(|name| !name.is_empty())
                                    })
                                    .or_else(|| volume.get("volumeId")),
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("Temporarily suspend BitLocker on {names}; restore it when installation finishes. Data stays encrypted. Automatic recovery is prepared in case of interruption.")
                };
                if planned["bootMenu"] != json!(boot_menu)
                    || planned["bootPolicy"].as_str()
                        != Some("menu-first-preserve-existing-entries")
                {
                    return Err("The startup settings changed while preparing the plan".into());
                }
                let summary = format!("Install Omarchy on Disk {disk_number}?\n\nSpace for Omarchy: {:.2} GiB\n{resize_review}\n\n{bitlocker_review}\n\n{boot_review}\nThis menu becomes the first boot option; existing boot entries are kept.\n\nKeep the computer powered on until installation finishes.", *allocation_bytes as f64 / 1_073_741_824.0);
                confirm(output, confirmations, &cancel, &summary, &planned)?;
                cancelled(&cancel)?;
                let deploy_request = write_request(
                    &workspace,
                    "deploy-request.json",
                    &json!({"planPath":planned.get("planPath").ok_or("No direct-install plan path")?,"planSha256":planned.get("planSha256").ok_or("No direct-install plan digest")?,"manifestPath":built["manifestPath"],"manifestSha256":built["manifestSha256"]}),
                )?;
                let mut command = provider_process::direct_command(
                    &runtime,
                    &manifest,
                    "deploy",
                    &deploy_request,
                )?;
                provider_process::privileged_environment(&mut command, &workspace)?;
                stage(
                    output,
                    "allocating",
                    "Installing Omarchy into the selected space…",
                    false,
                );
                // This invocation has no cooperative cancellation channel. Do not
                // advertise the provider's pre-allocation cancel flag to the UI.
                let deployed = provider_process::run_with_staging_key(
                    command,
                    staging_key.as_ref(),
                    Arc::clone(&cancel),
                    None,
                    false,
                    |mut value| {
                        if let Some(fields) = value.as_object_mut() {
                            fields.insert("cancelAvailable".into(), json!(false));
                        }
                        provider_event(output, value);
                    },
                )?;
                crate::setup_protocol::validate_direct_receipt(&planned, &deployed)?;
                deployed
            }
        };
        // Durable helper-owned receipt. Keep work products for explicit recovery;
        // no broad cleanup or automatic host reboot is performed.
        let record = json!({"protocol":1,"operationId":id,"sourceSha256":request.source.sha256,"destination":request.destination,"receipt":receipt,"workspace":workspace});
        let receipt_path = write_request(&workspace, "receipt.json", &record)?;
        Ok(json!({"operationId":id,"receipt":receipt,"receiptPath":receipt_path}))
    })();
    let mut cleanup = crate::operation_cleanup::finish(
        &workspace,
        &id,
        &request.source.file_name,
        &manifest,
        outcome.is_ok(),
    );
    let _ = write_request(&workspace, "cleanup.json", &cleanup);
    let mut workspace_removed = false;
    let mut record_warning = None;
    if outcome.is_ok() && workspace.join("receipt.json").is_file() && records_directory.is_some() {
        let exported = (|| -> Result<(), String> {
            let receipt: Value = serde_json::from_slice(
                &std::fs::read(workspace.join("receipt.json")).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let plan = json!({"kind":"record_export","operationId":id,"records":{"receipt":receipt,"cleanup":cleanup}});
            if serde_json::to_vec(&plan).map_err(|e| e.to_string())?.len()
                > crate::setup_protocol::MAX_FRAME - 4096
            {
                return Err("Operation records exceed the export limit".into());
            }
            confirm(
                output,
                confirmations,
                &cancel,
                "Save the completed operation records",
                &plan,
            )
        })();
        if exported.is_ok() {
            if matches!(
                request.destination,
                Destination::Usb { .. } | Destination::UsbPreserve { .. }
            ) && cleanup["complete"] == true
            {
                match crate::operation_cleanup::remove_completed_workspace(&workspace, &id) {
                    Ok(()) => workspace_removed = true,
                    Err(error) => {
                        record_warning = Some(format!(
                        "Records were exported, but some temporary operation files remain: {error}"
                    ))
                    }
                }
            }
        } else {
            record_warning = Some("Completed records could not be exported. The protected originals have been retained.".to_owned());
        }
    } else if outcome.is_ok()
        && matches!(
            request.destination,
            Destination::InspectDirect
                | Destination::InspectUsb { .. }
                | Destination::PrepareRuntime
        )
        && cleanup["complete"] == true
    {
        workspace_removed =
            crate::operation_cleanup::remove_completed_workspace(&workspace, &id).is_ok();
    }
    cleanup["workspaceRemoved"] = json!(workspace_removed);
    let recovery = json!({"filesPath":workspace,"mutationStarted":workspace.join("image/deployment-started.json").is_file(),
    "cleanup":cleanup,"message":if workspace_removed { "Temporary operation files were removed." } else if outcome.is_ok() { "Known temporary files were cleaned up. Protected operation records are retained for recovery." } else {
        "Keep the operation records. If storage changes started, inspect the recorded target before retrying. Use the existing Windows Boot Manager firmware entry to return to Windows if needed."
    }});
    emit(
        output,
        json!({"protocol":1,"type":"event","stage":"finished","recovery":recovery,"cancelAvailable":false}),
    );
    match outcome {
        Ok(mut result) => {
            result["cleanup"] = cleanup;
            result["workspaceRemoved"] = json!(workspace_removed);
            if let Some(warning) = record_warning {
                result["recordWarning"] = json!(warning);
            }
            Ok(result)
        }
        Err(error) => {
            let _ = write_request(
                &workspace,
                "failure.json",
                &json!({"operationId":id,"error":error,"recovery":recovery}),
            );
            Err(format!("{error}\nOperation files: {}", workspace.display()))
        }
    }
}
