use crate::provider_runtime::{self, RuntimeManifest};
use crate::setup_protocol::{read_frame, write_frame};
use base64::Engine;
use serde_json::{json, Value};
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

pub fn hide(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH")
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONSTARTUP");
}

pub fn system_powershell() -> Result<std::path::PathBuf, String> {
    #[cfg(windows)]
    {
        let mut buffer = vec![0_u16; 32768];
        let size = unsafe {
            windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            )
        };
        if size == 0 || size as usize >= buffer.len() {
            return Err("Windows system directory is unavailable".into());
        }
        return Ok(
            std::path::PathBuf::from(String::from_utf16_lossy(&buffer[..size as usize]))
                .join("WindowsPowerShell/v1.0/powershell.exe"),
        );
    }
    #[cfg(not(windows))]
    {
        Err("The x86 direct-install provider currently requires Windows".into())
    }
}

#[cfg(windows)]
pub fn program_files() -> Result<std::path::PathBuf, String> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_ProgramFiles)
}

#[cfg(windows)]
pub fn program_data() -> Result<std::path::PathBuf, String> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_ProgramData)
}

#[cfg(windows)]
fn known_folder(id: &windows_sys::core::GUID) -> Result<std::path::PathBuf, String> {
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;
    let mut pointer = std::ptr::null_mut();
    if unsafe { SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut pointer) } < 0
        || pointer.is_null()
    {
        return Err("Windows application directory is unavailable".into());
    }
    let mut length = 0;
    unsafe {
        while *pointer.add(length) != 0 {
            length += 1;
        }
    }
    let result = std::path::PathBuf::from(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(pointer, length)
    }));
    unsafe {
        windows_sys::Win32::System::Com::CoTaskMemFree(pointer.cast());
    }
    Ok(result)
}

pub fn media_command(root: &Path, manifest: &RuntimeManifest) -> Result<Command, String> {
    let mut command = Command::new(provider_runtime::checked_path(
        root,
        &manifest.media.executable,
    )?);
    command
        .arg(provider_runtime::checked_path(
            root,
            &manifest.media.entrypoint,
        )?)
        .current_dir(root);
    hide(&mut command);
    Ok(command)
}

pub fn inspection_command(root: &Path, manifest: &RuntimeManifest) -> Result<Command, String> {
    let media = manifest
        .media_inspection
        .as_ref()
        .ok_or("Read-only USB inspector is not packaged")?;
    let mut command = Command::new(provider_runtime::checked_path(root, &media.executable)?);
    command
        .arg(provider_runtime::checked_path(root, &media.entrypoint)?)
        .current_dir(root);
    hide(&mut command);
    Ok(command)
}

pub fn direct_command(
    root: &Path,
    manifest: &RuntimeManifest,
    action: &str,
    request: &Path,
) -> Result<Command, String> {
    if ![
        "probe",
        "build",
        "plan",
        "deploy",
        "firmware-info",
        "prepare-firmware",
        "prepare-runtime",
    ]
    .contains(&action)
    {
        return Err("Unknown direct-install operation".into());
    }
    let script = manifest
        .direct_x86
        .as_ref()
        .ok_or("The direct-install provider is not packaged")?;
    let mut command = Command::new(system_powershell()?);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(provider_runtime::checked_path(root, &script.entrypoint)?)
        .args(["-Action", action, "-RequestPath"])
        .arg(request)
        .current_dir(root);
    hide(&mut command);
    Ok(command)
}

// Child code and native modules have already been copied and checked inside a
// protected operation directory before this is used for privileged execution.
pub fn privileged_environment(command: &mut Command, workspace: &Path) -> Result<(), String> {
    command.env_clear();
    let temporary = workspace.join("temporary");
    let profile = workspace.join("profile");
    std::fs::create_dir_all(&temporary).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&profile).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        let system = system_powershell()?
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or("Windows directory is unavailable")?
            .to_path_buf();
        let windows = system.parent().ok_or("Windows directory is unavailable")?;
        command
            .env("SystemRoot", windows)
            .env("windir", windows)
            .env("ComSpec", system.join("cmd.exe"));
        // Docker is invoked by its installed system path, not a user PATH entry.
        let program_files = program_files()?;
        let docker = program_files.join("Docker/Docker/resources/bin");
        command
            .env("ProgramFiles", &program_files)
            .env("ProgramW6432", &program_files)
            .env("OS", "Windows_NT")
            .env(
                "PROCESSOR_ARCHITECTURE",
                if cfg!(target_arch = "x86_64") {
                    "AMD64"
                } else {
                    "ARM64"
                },
            );
        command.env(
            "PATH",
            std::env::join_paths([
                system.clone(),
                system.join("WindowsPowerShell/v1.0"),
                system.join("Wbem"),
                docker,
            ])
            .map_err(|e| e.to_string())?,
        );
        // -NoProfile alone does not prevent module autoload from a user's
        // Documents directory. Resolve Get-Disk/Storage only from OS modules.
        command.env(
            "PSModulePath",
            system.join("WindowsPowerShell/v1.0/Modules"),
        );
        command
            .env("TEMP", &temporary)
            .env("TMP", &temporary)
            .env("USERPROFILE", &profile)
            .env("APPDATA", profile.join("AppData/Roaming"))
            .env("LOCALAPPDATA", profile.join("AppData/Local"))
            .env("DOCKER_HOST", "npipe:////./pipe/dockerDesktopLinuxEngine")
            .env("DOCKER_CONFIG", profile.join(".docker"));
    }
    #[cfg(unix)]
    {
        command
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .env("HOME", &profile)
            .env("TMPDIR", &temporary)
            .env("LANG", "C.UTF-8");
    }
    hide(command);
    Ok(())
}

fn drain_diagnostics(mut pipe: impl Read) -> String {
    // Retain a bounded tail while draining to EOF. Closing a full stderr pipe
    // can break child writes; stopping reads can instead deadlock the provider.
    const LIMIT: usize = 16384;
    let mut retained = Vec::with_capacity(LIMIT);
    let mut buffer = [0_u8; 8192];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                let overflow = retained.len().saturating_add(count).saturating_sub(LIMIT);
                if overflow > 0 {
                    retained.drain(..overflow);
                }
                retained.extend_from_slice(&buffer[..count]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&retained).into_owned()
}

/// Keep stdin alive until process exit. A result record alone is not success.
pub fn run(
    command: Command,
    input: Option<Value>,
    cancel: Arc<AtomicBool>,
    cancellation_file: Option<std::path::PathBuf>,
    allow_abort: bool,
    event: impl FnMut(Value),
) -> Result<Value, String> {
    run_inner(
        command,
        input,
        None,
        cancel,
        cancellation_file,
        allow_abort,
        event,
    )
}

/// The per-operation staging key exists only in the privileged process and
/// private child stdin. It is never an argument, environment entry or file.
pub fn run_with_staging_key(
    command: Command,
    key: &[u8],
    cancel: Arc<AtomicBool>,
    cancellation_file: Option<std::path::PathBuf>,
    allow_abort: bool,
    event: impl FnMut(Value),
) -> Result<Value, String> {
    if key.len() != 32 {
        return Err("Invalid staging key size".into());
    }
    let encoded = Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(key));
    let frame = Zeroizing::new(
        format!(
            "{{\"protocolVersion\":2,\"stagingKey\":\"{}\"}}\n",
            encoded.as_str()
        )
        .into_bytes(),
    );
    run_inner(
        command,
        None,
        Some(frame),
        cancel,
        cancellation_file,
        allow_abort,
        event,
    )
}

fn run_inner(
    mut command: Command,
    input: Option<Value>,
    secret_frame: Option<Zeroizing<Vec<u8>>>,
    cancel: Arc<AtomicBool>,
    cancellation_file: Option<std::path::PathBuf>,
    allow_abort: bool,
    mut event: impl FnMut(Value),
) -> Result<Value, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Operation cancelled before provider launch".into());
    }
    // Serialize and enforce the frame bound before spawning anything.
    let initial = input
        .as_ref()
        .map(|value| {
            let mut bytes = Vec::new();
            write_frame(&mut bytes, value)?;
            Ok::<_, String>(Zeroizing::new(bytes))
        })
        .transpose()?
        .or(secret_frame);
    let media = input.is_some();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start provider: {e}"))?;
    // Stdio::piped guarantees these handles on a successful spawn.
    let mut stdin = child.stdin.take().expect("piped provider stdin");
    let stdout = child.stdout.take().expect("piped provider stdout");
    let stderr = child.stderr.take().expect("piped provider stderr");
    let stderr_reader = std::thread::spawn(move || drain_diagnostics(stderr));
    let (sender, receiver) = mpsc::sync_channel(32);
    let stdout_reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_frame(&mut reader) {
                Ok(Some(value)) => {
                    if sender.send(Ok(value)).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = sender.send(Err(error));
                    // Even malformed output must remain drained while a direct
                    // deployment finishes. Never induce SIGPIPE after allocation.
                    let _ = std::io::copy(&mut reader, &mut std::io::sink());
                    break;
                }
            }
        }
    });
    let mut result = None;
    let mut failure = initial
        .as_ref()
        .and_then(|bytes| stdin.write_all(bytes).and_then(|_| stdin.flush()).err())
        .map(|error| format!("Could not send provider request: {error}"));
    drop(initial);
    let mut cancelled = false;
    let mut stop_requested = false;
    let mut hard_stop = None;
    let mut output_closed = false;
    let status;
    loop {
        let user_cancel = allow_abort && cancel.load(Ordering::Relaxed);
        // A media worker can be stopped safely. A direct build uses its
        // cooperative cancellation file. Noninterruptible deployment continues
        // to completion even if its output protocol is broken.
        if !stop_requested && (user_cancel || (failure.is_some() && (media || allow_abort))) {
            cancelled |= user_cancel;
            stop_requested = true;
            if let Some(path) = cancellation_file.as_ref() {
                if let Err(error) = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                {
                    if error.kind() != std::io::ErrorKind::AlreadyExists {
                        failure = Some(format!("Could not request cancellation: {error}"));
                    }
                }
            } else {
                let _ = write_frame(&mut stdin, &json!({"protocol":1,"action":"abort"}));
                // The media CLI's process-exit boundary releases its raw handle.
                // EOF also releases the fixed volume-lock child on Windows.
                hard_stop = Some(Instant::now() + Duration::from_secs(3));
            }
        }
        if hard_stop.is_some_and(|deadline| Instant::now() >= deadline) {
            let _ = child.kill();
            hard_stop = None;
        }
        if !output_closed {
            match receiver.recv_timeout(Duration::from_millis(150)) {
                Ok(Ok(value)) => {
                    let expected_version = if media { "protocol" } else { "protocolVersion" };
                    if value.get(expected_version).and_then(Value::as_u64) != Some(1) {
                        failure.get_or_insert_with(|| "Provider protocol version mismatch".into());
                        continue;
                    }
                    match value.get("type").and_then(Value::as_str) {
                        Some("result") => {
                            if result.is_some() {
                                failure.get_or_insert_with(|| {
                                    "Provider returned multiple completion records".into()
                                });
                                continue;
                            }
                            if media
                                && value.get("action")
                                    != input.as_ref().and_then(|input| input.get("action"))
                            {
                                failure.get_or_insert_with(|| {
                                    "Provider completion action mismatch".into()
                                });
                                continue;
                            }
                            let mut body = value
                                .get("result")
                                .cloned()
                                .unwrap_or_else(|| value.clone());
                            if let Some(object) = body.as_object_mut() {
                                for key in [
                                    "manifestPath",
                                    "manifestSha256",
                                    "planPath",
                                    "planSha256",
                                    "receiptPath",
                                ] {
                                    if let Some(field) = value.get(key) {
                                        object.insert(key.to_owned(), field.clone());
                                    }
                                }
                            }
                            result = Some(body);
                        }
                        Some("error") => {
                            let mut message = value
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("Provider operation failed")
                                .to_owned();
                            if let Some(recovery) = value.get("recovery").and_then(Value::as_str) {
                                message.push('\n');
                                message.push_str(recovery);
                            }
                            if value.get("possiblyModified").and_then(Value::as_bool) == Some(true)
                            {
                                message.push_str(if media {
                                    "\nThe selected USB may be partially written."
                                } else {
                                    "\nThe selected disk may contain incomplete installation changes."
                                });
                            }
                            failure = Some(message);
                            event(value);
                        }
                        Some("event" | "progress") => event(value),
                        _ => {
                            failure.get_or_insert_with(|| "Unknown provider event type".into());
                        }
                    }
                }
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => output_closed = true,
            }
        } else {
            std::thread::sleep(Duration::from_millis(150));
        }
        match child.try_wait() {
            Ok(Some(exit)) if output_closed => {
                status = exit;
                break;
            }
            Ok(_) => {}
            Err(error) => {
                failure.get_or_insert_with(|| format!("Could not observe provider exit: {error}"));
                // Keep a noninterruptible deployment alive. Reap it before
                // returning; keep draining its output while retrying observation.
                if media {
                    let _ = child.kill();
                }
                std::thread::sleep(Duration::from_millis(150));
            }
        }
    }
    drop(stdin);
    let _ = stdout_reader.join();
    let diagnostics = stderr_reader.join().unwrap_or_default();
    if let Some(error) = failure {
        return Err(error);
    }
    if cancelled {
        return Err("Operation cancelled; no completion receipt is accepted".into());
    }
    if !status.success() {
        return Err(if diagnostics.trim().is_empty() {
            format!("Provider exited with {status}")
        } else {
            diagnostics.trim().to_owned()
        });
    }
    result.ok_or_else(|| "Provider exited without a completion receipt".into())
}
