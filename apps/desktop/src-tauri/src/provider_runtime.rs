use crate::setup_protocol::Destination;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const EMBEDDED: &str = include_str!(concat!(env!("OUT_DIR"), "/provider-lock.json"));

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeManifest {
    pub schema: u32,
    pub platform: String,
    pub architecture: String,
    pub media: MediaRuntime,
    pub media_inspection: Option<MediaRuntime>,
    pub direct_x86: Option<ScriptRuntime>,
    pub apple: Option<ScriptRuntime>,
    pub files: Vec<RuntimeFile>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaRuntime {
    pub executable: String,
    pub entrypoint: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptRuntime {
    pub entrypoint: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeFile {
    pub path: String,
    pub length: u64,
    pub sha256: String,
}

pub fn manifest() -> Result<RuntimeManifest, String> {
    let manifest: RuntimeManifest = serde_json::from_str(EMBEDDED)
        .map_err(|_| "Native providers are not packaged. Run pnpm providers:stage and rebuild the desktop app.".to_string())?;
    let platform = match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    if manifest.schema != 1
        || manifest.platform != platform
        || manifest.architecture != arch
        || manifest.files.is_empty()
        || manifest.files.len() > 50000
    {
        return Err("Packaged providers do not match this platform".into());
    }
    let mut files = HashSet::new();
    for record in &manifest.files {
        checked_path(Path::new("."), &record.path)?;
        if record.length > 256 * 1024 * 1024
            || record.sha256.len() != 64
            || !record
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("Invalid compiled runtime file constraint".into());
        }
        let key = if cfg!(windows) {
            record.path.to_lowercase()
        } else {
            record.path.clone()
        };
        if !files.insert(key) {
            return Err("Compiled runtime contains duplicate file paths".into());
        }
    }
    for entry in std::iter::once(&manifest.media.executable)
        .chain(std::iter::once(&manifest.media.entrypoint))
        .chain(
            manifest
                .media_inspection
                .iter()
                .flat_map(|media| [&media.executable, &media.entrypoint]),
        )
        .chain(manifest.direct_x86.iter().map(|script| &script.entrypoint))
        .chain(manifest.apple.iter().map(|script| &script.entrypoint))
    {
        checked_path(Path::new("."), entry)?;
        if !manifest.files.iter().any(|record| &record.path == entry) {
            return Err(
                "A provider executable or entrypoint is not covered by compiled hashes".into(),
            );
        }
    }
    Ok(manifest)
}

pub fn checked_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains('\0')
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || relative.contains(':')
        || relative.contains('\\')
        || relative
            .split('/')
            .any(|part| part.is_empty() || part.ends_with(['.', ' ']))
    {
        return Err("Invalid packaged provider path".into());
    }
    #[cfg(windows)]
    if relative.split('/').any(|part| {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    }) {
        return Err("Packaged provider contains a reserved device filename".into());
    }
    Ok(root.join(path))
}

pub fn locate(manifest: &RuntimeManifest) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut roots = packaged_roots(&exe, std::env::consts::OS)?;
    if cfg!(debug_assertions) {
        roots.push(PathBuf::from(env!("OMARCHY_PROVIDER_ROOT")));
    }
    roots
        .into_iter()
        .find(|root| {
            checked_path(root, &manifest.media.executable).is_ok_and(|path| path.is_file())
        })
        .ok_or_else(|| "Packaged native provider runtime was not found".into())
}

fn packaged_roots(exe: &Path, platform: &str) -> Result<Vec<PathBuf>, String> {
    let parent = exe.parent().ok_or("Executable directory is unavailable")?;
    let mut roots = vec![parent.join("providers")];
    if platform == "macos" {
        roots.push(parent.join("../Resources/providers"));
    } else if platform == "linux" {
        // Tauri deb/rpm and AppDir layouts put resources in usr/lib/<package>,
        // while the executable is in usr/bin. A portable directory can keep
        // providers next to the executable. All candidates remain hash-checked.
        roots.push(parent.join(format!("../lib/{}/providers", env!("CARGO_PKG_NAME"))));
        roots.push(PathBuf::from(format!(
            "/usr/lib/{}/providers",
            env!("CARGO_PKG_NAME")
        )));
    }
    Ok(roots)
}

fn open_regular(root: &Path, relative: &str) -> Result<File, String> {
    let full = checked_path(root, relative)?;
    let mut cursor = root.to_path_buf();
    for part in Path::new(relative).components() {
        cursor.push(part);
        if fs::symlink_metadata(&cursor)
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Provider runtime contains a symbolic link".into());
        }
    }
    if !fs::symlink_metadata(&full)
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Provider runtime entry is not a regular file".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let file = options.open(full).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("Provider runtime entry is not a regular file".into());
    }
    Ok(file)
}

#[cfg(test)]
fn verify(root: &Path, manifest: &RuntimeManifest, cancel: &AtomicBool) -> Result<(), String> {
    for record in &manifest.files {
        copy_one(root, None, record, cancel, &mut |_| {})?;
    }
    Ok(())
}

pub fn verify_inspection(
    root: &Path,
    manifest: &RuntimeManifest,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let inspection = manifest
        .media_inspection
        .as_ref()
        .ok_or("Read-only USB inspector is not packaged")?;
    let (prefix, suffix) = inspection
        .entrypoint
        .split_once("/inspection/")
        .ok_or("Invalid inspector entrypoint")?;
    if suffix != "dist/inspection-cli.js"
        || inspection.executable
            != format!(
                "{prefix}/{}",
                if cfg!(windows) { "node.exe" } else { "node" }
            )
    {
        return Err("Invalid inspector runtime layout".into());
    }
    let subtree = format!("{prefix}/inspection/");
    for record in manifest
        .files
        .iter()
        .filter(|file| file.path == inspection.executable || file.path.starts_with(&subtree))
    {
        copy_one(root, None, record, cancel, &mut |_| {})?;
    }
    Ok(())
}

#[cfg(test)]
mod inspection_tests {
    use super::*;

    #[test]
    fn packaged_resources_follow_each_host_layout() {
        let linux = packaged_roots(
            Path::new("/opt/AppDir/usr/bin/omarchy-setup-desktop"),
            "linux",
        )
        .unwrap();
        assert!(linux.contains(&PathBuf::from(
            "/opt/AppDir/usr/bin/../lib/omarchy-setup-desktop/providers"
        )));
        assert!(linux.contains(&PathBuf::from("/usr/lib/omarchy-setup-desktop/providers")));
        let mac = packaged_roots(
            Path::new("/Applications/Omarchy Installer.app/Contents/MacOS/omarchy-setup-desktop"),
            "macos",
        )
        .unwrap();
        assert!(mac.contains(&PathBuf::from(
            "/Applications/Omarchy Installer.app/Contents/MacOS/../Resources/providers"
        )));
        assert_eq!(
            packaged_roots(Path::new("portable/app.exe"), "windows").unwrap(),
            vec![PathBuf::from("portable/providers")]
        );
    }

    #[test]
    fn inspector_authenticates_its_closure_and_full_verification_still_checks_writer() {
        let directory = tempfile::tempdir().unwrap();
        let node = format!("media/{}", if cfg!(windows) { "node.exe" } else { "node" });
        let entry = "media/inspection/dist/inspection-cli.js";
        let paths = [
            node.as_str(),
            entry,
            "media/inspection/node_modules/native.node",
            "media/writer.js",
        ];
        let files = paths
            .iter()
            .map(|path| {
                let destination = directory.path().join(path);
                fs::create_dir_all(destination.parent().unwrap()).unwrap();
                fs::write(destination, b"good").unwrap();
                RuntimeFile {
                    path: (*path).into(),
                    length: 4,
                    sha256: format!("{:x}", Sha256::digest(b"good")),
                }
            })
            .collect();
        let manifest = RuntimeManifest {
            schema: 1,
            platform: "test".into(),
            architecture: "test".into(),
            media: MediaRuntime {
                executable: node.clone(),
                entrypoint: "media/writer.js".into(),
            },
            media_inspection: Some(MediaRuntime {
                executable: node,
                entrypoint: entry.into(),
            }),
            direct_x86: None,
            apple: None,
            files,
        };
        let cancel = AtomicBool::new(false);
        verify_inspection(directory.path(), &manifest, &cancel).unwrap();
        fs::write(directory.path().join("media/writer.js"), b"evil").unwrap();
        verify_inspection(directory.path(), &manifest, &cancel).unwrap();
        assert!(verify(directory.path(), &manifest, &cancel).is_err());
        fs::write(
            directory
                .path()
                .join("media/inspection/node_modules/native.node"),
            b"evil",
        )
        .unwrap();
        assert!(verify_inspection(directory.path(), &manifest, &cancel).is_err());
    }
}

// The destination must be owned by the privileged helper before this function
// runs. Hash the bytes actually copied, never reopen untrusted source code after
// validating a separate read of it.
pub fn copy_protected(
    root: &Path,
    destination: &Path,
    manifest: &RuntimeManifest,
    operation: &Destination,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    let files = operation_files(manifest, operation)?;
    let total = files.iter().map(|file| file.length).sum();
    let mut copied = 0;
    let mut last_update = Instant::now();
    progress(0, total);
    for record in files {
        copy_one(root, Some(destination), record, cancel, &mut |bytes| {
            if last_update.elapsed() >= Duration::from_millis(100) {
                progress(copied + bytes, total);
                last_update = Instant::now();
            }
        })?;
        copied += record.length;
    }
    progress(copied, total);
    Ok(())
}

fn operation_files<'a>(
    manifest: &'a RuntimeManifest,
    operation: &Destination,
) -> Result<Vec<&'a RuntimeFile>, String> {
    let direct = matches!(
        operation,
        Destination::InspectDirect
            | Destination::PrepareFirmware
            | Destination::PrepareRuntime
            | Destination::DirectX86 { .. }
    );
    let prefix = if direct {
        let script = manifest
            .direct_x86
            .as_ref()
            .ok_or("The direct-install provider is not packaged")?;
        if script.entrypoint != "direct-x86/Invoke-DirectX86.ps1" {
            return Err("Invalid direct-install runtime layout".into());
        }
        "direct-x86/".to_owned()
    } else {
        let (prefix, _) = manifest
            .media
            .executable
            .rsplit_once('/')
            .ok_or("Invalid media runtime layout")?;
        format!("{prefix}/")
    };
    let inspection = matches!(
        operation,
        Destination::InspectDirect | Destination::PrepareFirmware
    );
    Ok(manifest
        .files
        .iter()
        .filter(|file| {
            file.path.starts_with(&prefix)
                || (direct
                    && if inspection {
                        // Probe loads the direct provider's scripts and checks runtime
                        // metadata. The archive is needed only when importing Docker.
                        matches!(
                            file.path.as_str(),
                            "image-builder-x86/Dockerfile"
                                | "image-builder-x86/runtime-lock.json"
                                | "image-builder-x86/runtime-packages.lock"
                                | "image-builder-x86/runtime-distribution.json"
                        )
                    } else {
                        file.path.starts_with("image-builder-x86/")
                    })
                || (matches!(operation, Destination::UsbPreserve { .. })
                    && file.path.starts_with("usb-preserve/"))
        })
        .collect())
}

fn copy_one(
    root: &Path,
    destination: Option<&Path>,
    record: &RuntimeFile,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(u64),
) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Operation cancelled".into());
    }
    if record.length > 256 * 1024 * 1024 || record.sha256.len() != 64 {
        return Err("Invalid runtime file constraint".into());
    }
    let mut source = open_regular(root, &record.path)?;
    if source.metadata().map_err(|error| error.to_string())?.len() != record.length {
        return Err(format!("Packaged runtime changed: {}", record.path));
    }
    let mut target = if let Some(destination) = destination {
        let path = checked_path(destination, &record.path)?;
        fs::create_dir_all(path.parent().ok_or("Invalid runtime parent")?)
            .map_err(|error| error.to_string())?;
        Some(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut total = 0_u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Operation cancelled".into());
        }
        let read = source
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > record.length {
            return Err("Packaged file grew during staging".into());
        }
        hash.update(&buffer[..read]);
        if let Some(target) = target.as_mut() {
            target
                .write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
        }
        progress(total);
    }
    if total != record.length || format!("{:x}", hash.finalize()) != record.sha256 {
        return Err(format!("Packaged runtime digest mismatch: {}", record.path));
    }
    // These are disposable process inputs, not recovery records. Completed
    // File writes are visible to the child without a physical flush per file;
    // no operation resumes from this directory after a crash.
    drop(target);
    #[cfg(unix)]
    if let Some(destination) = destination {
        use std::os::unix::fs::PermissionsExt;
        let executable = record.path.ends_with("/node") || record.path.ends_with(".sh");
        fs::set_permissions(
            checked_path(destination, &record.path)?,
            fs::Permissions::from_mode(if executable { 0o700 } else { 0o600 }),
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod staging_tests {
    use super::*;

    fn fixture(root: &Path) -> RuntimeManifest {
        let paths = [
            "media/node",
            "media/writer.js",
            "media/node_modules/dependency.js",
            "direct-x86/Invoke-DirectX86.ps1",
            "direct-x86/NativeDisk.cs",
            "image-builder-x86/Dockerfile",
            "image-builder-x86/runtime-lock.json",
            "image-builder-x86/runtime-packages.lock",
            "image-builder-x86/runtime-distribution.json",
            "image-builder-x86/runtime.tar",
            "image-builder-x86/product_builder.py",
            "usb-preserve/boot/BOOTX64.EFI",
        ];
        let files = paths
            .iter()
            .map(|path| {
                let target = root.join(path);
                fs::create_dir_all(target.parent().unwrap()).unwrap();
                fs::write(target, b"good").unwrap();
                RuntimeFile {
                    path: (*path).into(),
                    length: 4,
                    sha256: format!("{:x}", Sha256::digest(b"good")),
                }
            })
            .collect();
        RuntimeManifest {
            schema: 1,
            platform: "test".into(),
            architecture: "test".into(),
            media: MediaRuntime {
                executable: "media/node".into(),
                entrypoint: "media/writer.js".into(),
            },
            media_inspection: None,
            direct_x86: Some(ScriptRuntime {
                entrypoint: "direct-x86/Invoke-DirectX86.ps1".into(),
            }),
            apple: None,
            files,
        }
    }

    #[test]
    fn disk_check_skips_unneeded_payloads_and_reports_verified_bytes() {
        let source = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let manifest = fixture(source.path());
        // Neither an unavailable USB writer nor the large archive prevents a
        // partition check. Their manifest records still describe the package.
        fs::remove_file(source.path().join("media/node")).unwrap();
        fs::remove_file(source.path().join("image-builder-x86/runtime.tar")).unwrap();
        let mut updates = Vec::new();
        copy_protected(
            source.path(),
            output.path(),
            &manifest,
            &Destination::InspectDirect,
            &AtomicBool::new(false),
            |bytes, total| updates.push((bytes, total)),
        )
        .unwrap();
        assert_eq!(updates.first(), Some(&(0, 24)));
        assert_eq!(updates.last(), Some(&(24, 24)));
        assert!(updates.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        assert!(!output.path().join("media").exists());
        assert!(!output.path().join("usb-preserve").exists());
        assert!(!output.path().join("image-builder-x86/runtime.tar").exists());
        assert!(!output
            .path()
            .join("image-builder-x86/product_builder.py")
            .exists());
        assert_eq!(
            fs::read(output.path().join("direct-x86/NativeDisk.cs")).unwrap(),
            b"good"
        );
        assert_eq!(
            fs::read(output.path().join("image-builder-x86/runtime-lock.json")).unwrap(),
            b"good"
        );

        let import_output = tempfile::tempdir().unwrap();
        assert!(copy_protected(
            source.path(),
            import_output.path(),
            &manifest,
            &Destination::PrepareRuntime,
            &AtomicBool::new(false),
            |_, _| {}
        )
        .is_err());
    }

    #[test]
    fn staging_rejects_tampered_dependencies_and_cancellation() {
        let source = tempfile::tempdir().unwrap();
        let manifest = fixture(source.path());
        let output = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(true);
        assert!(copy_protected(
            source.path(),
            output.path(),
            &manifest,
            &Destination::InspectDirect,
            &cancelled,
            |_, _| {}
        )
        .unwrap_err()
        .contains("cancelled"));
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
        fs::write(source.path().join("direct-x86/NativeDisk.cs"), b"evil").unwrap();
        assert!(copy_protected(
            source.path(),
            output.path(),
            &manifest,
            &Destination::InspectDirect,
            &AtomicBool::new(false),
            |_, _| {}
        )
        .unwrap_err()
        .contains("digest mismatch"));
    }

    #[test]
    fn usb_staging_keeps_writer_dependencies_without_direct_tools() {
        let source = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let manifest = fixture(source.path());
        copy_protected(
            source.path(),
            output.path(),
            &manifest,
            &Destination::Usb {
                identity: serde_json::json!({}),
            },
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        assert_eq!(
            fs::read(output.path().join("media/node_modules/dependency.js")).unwrap(),
            b"good"
        );
        assert!(!output.path().join("direct-x86").exists());
        assert!(!output.path().join("image-builder-x86").exists());
        assert!(!output.path().join("usb-preserve").exists());
    }

    #[test]
    #[ignore = "Measures locally staged provider inputs; no host inspection or installation"]
    fn profile_packaged_disk_check_staging() {
        let manifest = manifest().unwrap();
        let source = locate(&manifest).unwrap();
        let files = operation_files(&manifest, &Destination::InspectDirect).unwrap();
        let total: u64 = files.iter().map(|file| file.length).sum();
        let output = tempfile::tempdir().unwrap();
        let started = Instant::now();
        copy_protected(
            &source,
            output.path(),
            &manifest,
            &Destination::InspectDirect,
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        eprintln!(
            "Disk-check staging: {} files, {total} bytes, {:?}",
            files.len(),
            started.elapsed()
        );
    }
}
