use omarchy_release_client::{DownloadPhase, DownloadedImage, Release};
use serde::Serialize;
use sha2::{Digest, Sha256};
#[cfg(all(test, windows))]
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Idle,
    Resolving,
    Ready,
    Preparing,
    Downloading,
    Verifying,
    Saving,
    Complete,
    Cancelled,
    Failed,
}

impl Status {
    fn active(&self) -> bool {
        matches!(
            self,
            Self::Resolving | Self::Preparing | Self::Downloading | Self::Verifying | Self::Saving
        )
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    status: Status,
    release: Option<Release>,
    received_bytes: u64,
    total_bytes: u64,
    image_path: Option<PathBuf>,
    image_locked: bool,
    error: Option<String>,
    cancel_requested: bool,
    destination_directory: PathBuf,
    existing_image: bool,
    host_os: &'static str,
    host_architecture: &'static str,
}

struct Inner {
    snapshot: Snapshot,
    cancel: Arc<AtomicBool>,
    #[cfg(windows)]
    verified_iso: Option<Arc<crate::iso_source::HeldIso>>,
}

struct VerifiedImage {
    path: PathBuf,
    #[cfg(windows)]
    guard: Arc<crate::iso_source::HeldIso>,
}

#[derive(Clone)]
pub struct Downloads {
    inner: Arc<Mutex<Inner>>,
    cache: PathBuf,
}

impl Downloads {
    /// The webview cannot supply this lease or mark an image verified. Retain it
    /// through the complete helper exchange even if download state is reset.
    #[cfg(windows)]
    pub(crate) fn verified_lease(
        &self,
        source: &crate::setup_protocol::SourceImage,
    ) -> Option<Arc<crate::iso_source::HeldIso>> {
        let inner = self.inner.lock().ok()?;
        let release = inner.snapshot.release.as_ref()?;
        if inner.snapshot.status != Status::Complete
            || inner.snapshot.image_path.as_ref() != Some(&source.path)
            || release.file_name() != source.file_name
            || release.length() != source.length
            || release.sha256() != source.sha256
            || release.signature() != source.signature
        {
            return None;
        }
        inner.verified_iso.clone()
    }

    fn complete(&self, image: VerifiedImage, existing: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.cancel.load(Ordering::Relaxed) {
                inner.snapshot.status = Status::Cancelled;
                inner.snapshot.error = Some("Download cancelled".into());
                inner.snapshot.image_path = None;
                inner.snapshot.image_locked = false;
                return;
            }
            #[cfg(windows)]
            {
                inner.verified_iso = Some(image.guard);
                inner.snapshot.image_locked = true;
            }
            inner.snapshot.status = Status::Complete;
            inner.snapshot.image_path = Some(image.path);
            inner.snapshot.existing_image = existing;
            inner.snapshot.error = None;
        }
    }
    pub(crate) fn verified_source(&self) -> Result<(PathBuf, Release), String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "Download state is unavailable")?;
        if inner.snapshot.status != Status::Complete {
            return Err("Download and verify an image first".into());
        }
        Ok((
            inner
                .snapshot
                .image_path
                .clone()
                .ok_or("Verified image path is unavailable")?,
            inner
                .snapshot
                .release
                .clone()
                .ok_or("Verified release is unavailable")?,
        ))
    }
    pub fn new(cache: PathBuf, destination: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                snapshot: Snapshot {
                    status: Status::Idle,
                    release: None,
                    received_bytes: 0,
                    total_bytes: 0,
                    image_path: None,
                    image_locked: false,
                    error: None,
                    cancel_requested: false,
                    destination_directory: destination.clone(),
                    existing_image: false,
                    host_os: std::env::consts::OS,
                    host_architecture: std::env::consts::ARCH,
                },
                cancel: Arc::new(AtomicBool::new(false)),
                #[cfg(windows)]
                verified_iso: None,
            })),
            cache,
        }
    }

    fn snapshot(&self) -> Result<Snapshot, String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| "Download state is unavailable")?
            .snapshot
            .clone())
    }

    fn update(&self, f: impl FnOnce(&mut Snapshot)) {
        if let Ok(mut inner) = self.inner.lock() {
            f(&mut inner.snapshot);
        }
    }

    fn finish_error(&self, error: String, cancelled: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            #[cfg(windows)]
            {
                inner.verified_iso = None;
            }
            inner.snapshot.image_locked = false;
            inner.snapshot.status = if cancelled {
                Status::Cancelled
            } else {
                Status::Failed
            };
            inner.snapshot.error = Some(error);
            inner.snapshot.image_path = None;
        }
    }

    fn begin_resolution(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Download state is unavailable")?;
        if inner.snapshot.status.active() {
            return Err("An operation is already running".into());
        }
        inner.snapshot.status = Status::Resolving;
        #[cfg(windows)]
        {
            inner.verified_iso = None;
        }
        inner.snapshot.image_locked = false;
        inner.snapshot.cancel_requested = false;
        inner.snapshot.error = None;
        inner.snapshot.release = None;
        inner.snapshot.existing_image = false;
        inner.snapshot.image_path = None;
        inner.snapshot.received_bytes = 0;
        inner.snapshot.total_bytes = 0;
        Ok(())
    }

    fn set_destination(&self, path: PathBuf) -> Result<Snapshot, String> {
        if !path.is_absolute() || !path.is_dir() {
            return Err("Choose an existing folder".into());
        }
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Download state is unavailable")?;
        if inner.snapshot.status.active() {
            return Err("Wait for the current operation to stop before changing folders".into());
        }
        if inner.snapshot.destination_directory != path {
            #[cfg(windows)]
            {
                inner.verified_iso = None;
            }
            inner.snapshot.image_locked = false;
            inner.snapshot.destination_directory = path;
            inner.snapshot.image_path = None;
            inner.snapshot.error = None;
            inner.snapshot.cancel_requested = false;
            inner.snapshot.received_bytes = 0;
            inner.snapshot.status = if inner.snapshot.release.is_some() {
                Status::Ready
            } else {
                Status::Idle
            };
        }
        detect_existing(&mut inner.snapshot);
        Ok(inner.snapshot.clone())
    }

    fn begin_transfer(&self) -> Result<(Release, Arc<AtomicBool>, PathBuf), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Download state is unavailable")?;
        if inner.snapshot.status.active() {
            return Err("An operation is already running".into());
        }
        let release = inner
            .snapshot
            .release
            .clone()
            .ok_or("Check the official release first")?;
        inner.cancel = Arc::new(AtomicBool::new(false));
        #[cfg(windows)]
        {
            inner.verified_iso = None;
        }
        inner.snapshot.image_locked = false;
        inner.snapshot.status = Status::Preparing;
        inner.snapshot.cancel_requested = false;
        inner.snapshot.error = None;
        inner.snapshot.image_path = None;
        inner.snapshot.received_bytes = 0;
        inner.snapshot.total_bytes = release.length();
        Ok((
            release,
            Arc::clone(&inner.cancel),
            inner.snapshot.destination_directory.clone(),
        ))
    }
}

#[tauri::command]
pub fn download_status(downloads: State<'_, Downloads>) -> Result<Snapshot, String> {
    downloads.snapshot()
}

#[tauri::command]
pub async fn choose_download_directory(
    app: tauri::AppHandle,
    downloads: State<'_, Downloads>,
) -> Result<Snapshot, String> {
    let current = downloads.snapshot()?;
    if current.status.active() {
        return Err("Wait for the current operation to stop before changing folders".into());
    }
    let selected = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Save Omarchy image to")
            .set_directory(current.destination_directory)
            .blocking_pick_folder()
    })
    .await
    .map_err(|error| format!("Folder chooser failed: {error}"))?;
    match selected {
        Some(folder) => {
            downloads.set_destination(folder.into_path().map_err(|error| error.to_string())?)
        }
        None => downloads.snapshot(),
    }
}

#[tauri::command]
pub async fn resolve_download(downloads: State<'_, Downloads>) -> Result<Snapshot, String> {
    downloads.begin_resolution()?;
    let controller = downloads.inner().clone();
    let result =
        tauri::async_runtime::spawn_blocking(omarchy_release_client::resolve_current).await;
    match result {
        Ok(Ok(release)) => controller.update(|snapshot| {
            snapshot.total_bytes = release.length();
            snapshot.release = Some(release);
            detect_existing(snapshot);
            snapshot.status = Status::Ready;
        }),
        Ok(Err(error)) => controller.finish_error(error.to_string(), false),
        Err(error) => controller.finish_error(format!("Release lookup failed: {error}"), false),
    }
    controller.snapshot()
}

#[tauri::command]
pub fn start_download(downloads: State<'_, Downloads>) -> Result<Snapshot, String> {
    let (release, cancel, destination) = downloads.begin_transfer()?;
    let controller = downloads.inner().clone();
    // No path, release metadata, key, or URL is accepted from the webview.
    let task_controller = controller.clone();
    tauri::async_runtime::spawn(async move {
        let task = tauri::async_runtime::spawn_blocking(move || {
            let existing = expected_image_path(&destination, release.file_name())
                .ok()
                .filter(|path| fs::symlink_metadata(path).is_ok());
            let progress = |progress: omarchy_release_client::DownloadProgress| {
                controller.update(|snapshot| {
                    snapshot.received_bytes = progress.received_bytes;
                    snapshot.total_bytes = progress.total_bytes;
                    snapshot.status = match progress.phase {
                        DownloadPhase::Preparing => Status::Preparing,
                        DownloadPhase::Downloading => Status::Downloading,
                        DownloadPhase::VerifyingSignature | DownloadPhase::Complete => {
                            Status::Verifying
                        }
                    };
                });
            };
            let result = (|| -> Result<VerifiedImage, String> {
                if let Some(path) = &existing {
                    return verify_and_retain(&release, path, &cancel, progress);
                }
                let image = omarchy_release_client::download(
                    &release,
                    &controller.cache,
                    &cancel,
                    progress,
                )
                .map_err(|error| error.to_string())?;
                controller.update(|snapshot| snapshot.status = Status::Saving);
                let path = save_verified_image(&image, &destination, &cancel)?;
                #[cfg(windows)]
                {
                    // The cache and saved ISO are different files. Authenticate
                    // the final file under its lifetime lock before showing Verified.
                    verify_and_retain(&release, &path, &cancel, progress)
                }
                #[cfg(not(windows))]
                {
                    Ok(VerifiedImage { path })
                }
            })();
            match result {
                Ok(image) => controller.complete(image, existing.is_some()),
                Err(error) => {
                    controller.finish_error(error, cancel.load(Ordering::Relaxed));
                }
            }
        });
        if let Err(error) = task.await {
            task_controller.finish_error(format!("Download worker stopped: {error}"), false);
        }
    });
    downloads.snapshot()
}

fn verify_and_retain(
    release: &Release,
    path: &Path,
    cancel: &AtomicBool,
    progress: impl FnMut(omarchy_release_client::DownloadProgress),
) -> Result<VerifiedImage, String> {
    #[cfg(windows)]
    let guard = Arc::new(crate::iso_source::HeldIso::open(path, release.length())?);
    omarchy_release_client::verify_existing(release, path, cancel, progress)
        .map_err(|error| error.to_string())?;
    Ok(VerifiedImage {
        path: path.to_path_buf(),
        #[cfg(windows)]
        guard,
    })
}

#[tauri::command]
pub fn cancel_download(downloads: State<'_, Downloads>) -> Result<Snapshot, String> {
    let mut inner = downloads
        .inner
        .lock()
        .map_err(|_| "Download state is unavailable")?;
    if matches!(
        inner.snapshot.status,
        Status::Preparing | Status::Downloading | Status::Verifying | Status::Saving
    ) {
        inner.cancel.store(true, Ordering::Relaxed);
        inner.snapshot.cancel_requested = true;
    }
    Ok(inner.snapshot.clone())
}

fn expected_image_path(directory: &Path, filename: &str) -> Result<PathBuf, String> {
    if filename.is_empty() || filename.contains(['/', '\\', ':']) || !filename.ends_with(".iso") {
        return Err("Invalid official image filename".into());
    }
    Ok(directory.join(filename))
}

// This is only a UI hint. Authentication happens after the user chooses Verify.
fn detect_existing(snapshot: &mut Snapshot) {
    snapshot.existing_image = snapshot
        .release
        .as_ref()
        .and_then(|release| {
            expected_image_path(&snapshot.destination_directory, release.file_name()).ok()
        })
        .is_some_and(|path| fs::symlink_metadata(path).is_ok());
}

fn save_verified_image(
    image: &DownloadedImage,
    destination: &Path,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    export_image(
        &image.path,
        destination,
        &image.file_name,
        image.length,
        &image.sha256,
        cancel,
    )
}

fn export_image(
    source: &Path,
    directory: &Path,
    filename: &str,
    length: u64,
    sha256: &str,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let target = expected_image_path(directory, filename)?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    if target.try_exists().map_err(|error| error.to_string())? {
        hash_file(&target, length, sha256, cancel)?;
        return Ok(target);
    }
    // Copy to a newly created temporary file; never overwrite a user's ISO.
    // The second hash also detects source changes after cache verification.
    let metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Verified source is not a regular file".into());
    }
    let mut input = File::open(source).map_err(|error| error.to_string())?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".omarchy-")
        .suffix(".part")
        .tempfile_in(directory)
        .map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut written = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Download cancelled; verified cache retained".into());
        }
        let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        written = written
            .checked_add(read as u64)
            .ok_or("Image length overflow")?;
        if written > length {
            return Err("Image changed after verification".into());
        }
        hasher.update(&buffer[..read]);
        temporary
            .write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
    }
    if written != length || format!("{:x}", hasher.finalize()) != sha256 {
        return Err("Image changed after verification".into());
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Download cancelled; verified cache retained".into());
    }
    temporary
        .persist_noclobber(&target)
        .map_err(|error| format!("Cannot save without replacing an existing file: {error}"))?;
    Ok(target)
}

fn hash_file(path: &Path, length: u64, expected: &str, cancel: &AtomicBool) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != length {
        return Err(
            "A different file already exists at the download destination; it was left unchanged"
                .into(),
        );
    }
    let mut input = File::open(path).map_err(|error| error.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut read_total = 0_u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Download cancelled".into());
        }
        let count = input.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        read_total += count as u64;
        if read_total > length {
            return Err("Destination file changed during verification".into());
        }
        hash.update(&buffer[..count]);
    }
    if read_total != length || format!("{:x}", hash.finalize()) != expected {
        return Err(
            "A different file already exists at the download destination; it was left unchanged"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn download_lock_lives_through_operation_and_releases_on_reset_or_cancellation() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("image.iso");
        fs::write(&path, b"image").unwrap();
        let controller = Downloads::new(temp.path().join("cache"), temp.path().to_path_buf());
        let guard = Arc::new(crate::iso_source::HeldIso::open(&path, 5).unwrap());
        let operation_lease = Arc::clone(&guard);
        controller.complete(
            VerifiedImage {
                path: path.clone(),
                guard,
            },
            true,
        );
        assert!(controller.snapshot().unwrap().image_locked);
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        controller.begin_resolution().unwrap();
        assert!(!controller.snapshot().unwrap().image_locked);
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        drop(operation_lease);
        assert!(OpenOptions::new().write(true).open(&path).is_ok());

        let guard = Arc::new(crate::iso_source::HeldIso::open(&path, 5).unwrap());
        controller
            .inner
            .lock()
            .unwrap()
            .cancel
            .store(true, Ordering::Relaxed);
        controller.complete(
            VerifiedImage {
                path: path.clone(),
                guard,
            },
            true,
        );
        assert_eq!(controller.snapshot().unwrap().status, Status::Cancelled);
        assert!(!controller.snapshot().unwrap().image_locked);
        assert!(OpenOptions::new().write(true).open(&path).is_ok());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Reads an existing official ISO and current release metadata; no administrator access or disk writes"]
    fn existing_official_image_retains_its_first_verification() {
        let path = PathBuf::from(std::env::var_os("OMARCHY_TEST_ISO").expect("ISO path required"));
        let release = omarchy_release_client::resolve_current().unwrap();
        let mut passes = 0;
        let timer = std::time::Instant::now();
        let image = verify_and_retain(&release, &path, &AtomicBool::new(false), |progress| {
            if progress.phase == DownloadPhase::VerifyingSignature && progress.received_bytes == 0 {
                passes += 1;
            }
        })
        .unwrap();
        assert_eq!(passes, 1);
        let elapsed = timer.elapsed();
        let source = crate::setup_protocol::SourceImage {
            path: path.clone(),
            file_name: release.file_name().into(),
            length: release.length(),
            sha256: release.sha256().into(),
            signature: release.signature().to_vec(),
        };
        let controller = Downloads::new(PathBuf::new(), path.parent().unwrap().to_path_buf());
        controller.update(|snapshot| snapshot.release = Some(release));
        controller.complete(image, true);
        let lease = controller.verified_lease(&source).unwrap();
        let timer = std::time::Instant::now();
        let binding = lease.verified_binding(&source).unwrap();
        let helper_guard = crate::iso_source::HeldIso::open(&source.path, source.length).unwrap();
        helper_guard
            .validate_verified_binding(&source, &binding, std::process::id())
            .unwrap();
        println!("One locked verification of {} bytes in {:.3}s; helper identity handoff in {:.6}s with no ISO scan", source.length, elapsed.as_secs_f64(), timer.elapsed().as_secs_f64());
        let mut changed = source.clone();
        changed.sha256 = "00".repeat(32);
        assert!(controller.verified_lease(&changed).is_none());
        assert!(OpenOptions::new().write(true).open(&path).is_err());
    }

    #[test]
    fn rejects_overlapping_operations() {
        let controller = Downloads::new(PathBuf::from("cache"), PathBuf::from("downloads"));
        assert!(controller.begin_transfer().is_err());
        controller.begin_resolution().unwrap();
        assert!(controller.begin_resolution().is_err());
        assert!(controller.begin_transfer().is_err());
    }

    #[test]
    fn export_checks_content_and_never_replaces_an_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::write(&source, b"image").unwrap();
        let destination = temp.path().join("downloads");
        let hash = format!("{:x}", Sha256::digest(b"image"));
        let cancel = AtomicBool::new(false);
        let target =
            export_image(&source, &destination, "omarchy-1.0.iso", 5, &hash, &cancel).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"image");
        assert_eq!(
            export_image(&source, &destination, "omarchy-1.0.iso", 5, &hash, &cancel).unwrap(),
            target
        );
        fs::write(&target, b"other").unwrap();
        assert!(export_image(&source, &destination, "omarchy-1.0.iso", 5, &hash, &cancel).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"other");
        assert!(export_image(&source, &destination, "../escape.iso", 5, &hash, &cancel).is_err());
    }

    #[test]
    fn folder_changes_invalidate_completion_but_leave_saved_files_intact() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        let saved = old.join("image.iso");
        fs::write(&saved, b"image").unwrap();
        let controller = Downloads::new(temp.path().join("cache"), old.clone());
        controller.update(|snapshot| {
            snapshot.status = Status::Complete;
            snapshot.image_path = Some(saved.clone());
        });
        assert_eq!(
            controller.set_destination(old).unwrap().status,
            Status::Complete
        );
        let changed = controller.set_destination(new.clone()).unwrap();
        assert_eq!(changed.destination_directory, new);
        assert_eq!(changed.status, Status::Idle);
        assert!(changed.image_path.is_none());
        assert_eq!(fs::read(saved).unwrap(), b"image");
    }

    #[test]
    fn folder_changes_reject_invalid_paths_and_running_operations() {
        let temp = tempfile::tempdir().unwrap();
        let controller = Downloads::new(temp.path().join("cache"), temp.path().to_path_buf());
        assert!(controller
            .set_destination(PathBuf::from("relative"))
            .is_err());
        assert!(controller
            .set_destination(temp.path().join("missing"))
            .is_err());
        let file = temp.path().join("file");
        fs::write(&file, b"file").unwrap();
        assert!(controller.set_destination(file).is_err());
        controller.begin_resolution().unwrap();
        assert!(controller
            .set_destination(temp.path().to_path_buf())
            .is_err());
        assert_eq!(controller.snapshot().unwrap().status, Status::Resolving);
    }

    #[test]
    fn changed_source_and_cancellation_never_publish_an_iso() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::write(&source, b"other").unwrap();
        let destination = temp.path().join("downloads");
        let hash = format!("{:x}", Sha256::digest(b"image"));
        assert!(export_image(
            &source,
            &destination,
            "omarchy-1.0.iso",
            5,
            &hash,
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(export_image(
            &source,
            &destination,
            "omarchy-1.0.iso",
            5,
            &hash,
            &AtomicBool::new(true)
        )
        .is_err());
        assert!(!destination.join("omarchy-1.0.iso").exists());
        assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
    }
}
