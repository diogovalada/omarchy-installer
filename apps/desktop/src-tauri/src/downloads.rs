use crate::image_release::ImageRelease;
use omarchy_release_client::{DownloadPhase, Release};
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
    release: Option<ImageRelease>,
    testing_build: bool,
    received_bytes: u64,
    total_bytes: u64,
    image_path: Option<PathBuf>,
    image_locked: bool,
    verification_skipped: bool,
    error: Option<String>,
    cancel_requested: bool,
    destination_directory: PathBuf,
    existing_image: bool,
    replacement_available: bool,
    host_os: &'static str,
    host_architecture: &'static str,
}

struct Inner {
    snapshot: Snapshot,
    replacement: Option<ReplacementTarget>,
    cancel: Arc<AtomicBool>,
    #[cfg(windows)]
    verified_iso: Option<Arc<crate::iso_source::HeldIso>>,
}

struct VerifiedImage {
    path: PathBuf,
    #[cfg(windows)]
    guard: Arc<crate::iso_source::HeldIso>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReplacementTarget {
    path: PathBuf,
    length: u64,
    modified: std::time::SystemTime,
    created: Option<std::time::SystemTime>,
    identity: (u64, u64),
}

impl ReplacementTarget {
    fn inspect(path: &Path) -> Result<Self, String> {
        let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("Only an ordinary ISO file can be replaced. Choose another folder.".into());
        }
        #[cfg(windows)]
        let identity = {
            use std::os::windows::fs::MetadataExt;
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Storage::FileSystem::{
                GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            };
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("Linked ISO files cannot be replaced. Choose another folder.".into());
            }
            let file = File::open(path).map_err(|e| e.to_string())?;
            let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            (
                u64::from(info.dwVolumeSerialNumber),
                (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            )
        };
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            (metadata.dev(), metadata.ino())
        };
        Ok(Self {
            path: path.to_path_buf(),
            length: metadata.len(),
            modified: metadata.modified().map_err(|e| e.to_string())?,
            created: metadata.created().ok(),
            identity,
        })
    }

    fn check(&self, path: &Path) -> Result<(), String> {
        if Self::inspect(path)? != *self {
            return Err("The existing ISO changed. Verify it again before replacing it.".into());
        }
        Ok(())
    }
}

// A signature-policy, network, access or cancellation error alone is never
// evidence that the user's file is damaged. Confirm a size/digest mismatch.
fn replacement_candidate(
    before: &ReplacementTarget,
    length: u64,
    expected: &str,
    cancel: &AtomicBool,
) -> Result<Option<ReplacementTarget>, String> {
    before.check(&before.path)?;
    let differs = if before.length != length {
        true
    } else {
        let mut file = File::open(&before.path).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0_u8; 1024 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        format!("{:x}", hash.finalize()) != expected
    };
    before.check(&before.path)?;
    Ok((differs && !cancel.load(Ordering::Relaxed)).then(|| before.clone()))
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
            || inner.snapshot.verification_skipped
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
            inner.snapshot.verification_skipped = false;
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
            inner.replacement = None;
            inner.snapshot.replacement_available = false;
            inner.snapshot.error = None;
        }
    }
    pub(crate) fn verified_source(&self) -> Result<(PathBuf, ImageRelease), String> {
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
                    testing_build: cfg!(all(windows, feature = "staged-iso-testing")),
                    received_bytes: 0,
                    total_bytes: 0,
                    image_path: None,
                    image_locked: false,
                    verification_skipped: false,
                    error: None,
                    cancel_requested: false,
                    destination_directory: destination.clone(),
                    existing_image: false,
                    replacement_available: false,
                    host_os: std::env::consts::OS,
                    host_architecture: std::env::consts::ARCH,
                },
                cancel: Arc::new(AtomicBool::new(false)),
                replacement: None,
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

    fn reuse_testing_image(&self) -> bool {
        let Ok(mut inner) = self.inner.lock() else {
            return false;
        };
        let Some(release) = inner.snapshot.release.as_ref() else {
            return false;
        };
        if release.official().is_err() {
            return false;
        }
        let (name, length) = (release.file_name().to_owned(), release.length());
        Self::use_testing_file(&mut inner, &name, length)
    }

    // Only the explicitly compiled testing executable can skip the startup
    // read. This is NOT a verified lease: the write helper still authenticates it.
    fn use_testing_file(inner: &mut Inner, name: &str, length: u64) -> bool {
        if !cfg!(feature = "staged-iso-testing") || inner.snapshot.status != Status::Ready {
            return false;
        }
        let Ok(path) = expected_image_path(&inner.snapshot.destination_directory, name) else {
            return false;
        };
        let Ok(file) = ReplacementTarget::inspect(&path) else {
            return false;
        };
        if file.length != length {
            return false;
        }
        #[cfg(windows)]
        {
            let Ok(guard) = crate::iso_source::HeldIso::open(&path, length) else {
                return false;
            };
            inner.verified_iso = Some(Arc::new(guard));
            inner.snapshot.image_locked = true;
        }
        inner.snapshot.verification_skipped = true;
        inner.snapshot.status = Status::Complete;
        inner.snapshot.image_path = Some(path);
        inner.snapshot.existing_image = true;
        inner.snapshot.received_bytes = length;
        inner.snapshot.error = None;
        inner.snapshot.replacement_available = false;
        inner.replacement = None;
        true
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
            inner.snapshot.verification_skipped = false;
            inner.snapshot.status = if cancelled {
                Status::Cancelled
            } else {
                Status::Failed
            };
            inner.snapshot.error = Some(error);
            inner.snapshot.image_path = None;
            inner.snapshot.replacement_available = inner.replacement.is_some();
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
        inner.snapshot.verification_skipped = false;
        inner.snapshot.cancel_requested = false;
        inner.snapshot.error = None;
        inner.snapshot.release = None;
        inner.snapshot.existing_image = false;
        inner.replacement = None;
        inner.snapshot.replacement_available = false;
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
            if inner
                .snapshot
                .release
                .as_ref()
                .is_some_and(|r| r.official().is_err())
            {
                inner.snapshot.release = None;
            }
            inner.replacement = None;
            inner.snapshot.replacement_available = false;
            #[cfg(windows)]
            {
                inner.verified_iso = None;
            }
            inner.snapshot.image_locked = false;
            inner.snapshot.verification_skipped = false;
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
        drop(inner);
        self.reuse_testing_image();
        self.snapshot()
    }

    fn begin_transfer(&self, replace: bool) -> Result<Transfer, String> {
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
            .ok_or("Check the official release first")?
            .official()?
            .clone();
        let replacement = if replace {
            let candidate = inner
                .replacement
                .clone()
                .ok_or("Verify the existing ISO before requesting a replacement")?;
            let target =
                expected_image_path(&inner.snapshot.destination_directory, release.file_name())?;
            if let Err(error) = candidate.check(&target) {
                inner.replacement = None;
                inner.snapshot.replacement_available = false;
                return Err(error);
            }
            Some(candidate)
        } else {
            inner.replacement = None;
            None
        };
        inner.cancel = Arc::new(AtomicBool::new(false));
        #[cfg(windows)]
        {
            inner.verified_iso = None;
        }
        inner.snapshot.image_locked = false;
        inner.snapshot.verification_skipped = false;
        inner.snapshot.status = Status::Preparing;
        inner.snapshot.replacement_available = false;
        if replace {
            inner.snapshot.existing_image = false;
        }
        inner.snapshot.cancel_requested = false;
        inner.snapshot.error = None;
        inner.snapshot.image_path = None;
        inner.snapshot.received_bytes = 0;
        inner.snapshot.total_bytes = release.length();
        Ok(Transfer {
            release,
            cancel: Arc::clone(&inner.cancel),
            destination: inner.snapshot.destination_directory.clone(),
            replacement,
        })
    }
}

struct Transfer {
    release: Release,
    cancel: Arc<AtomicBool>,
    destination: PathBuf,
    replacement: Option<ReplacementTarget>,
}

#[tauri::command]
pub fn download_status(downloads: State<'_, Downloads>) -> Result<Snapshot, String> {
    downloads.snapshot()
}

#[cfg(feature = "staged-iso-testing")]
pub(crate) fn testing_iso_digest(
    path: &Path,
    length: u64,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<String, String> {
    if length == 0 || length > 64 * 1024 * 1024 * 1024 {
        return Err("Choose a nonempty ISO smaller than 64 GiB.".into());
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Choose an ordinary ISO file.".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 8 * 1024 * 1024];
    let mut total = 0;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Image selection cancelled".into());
        }
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > length {
            return Err("The selected ISO changed size.".into());
        }
        hash.update(&buffer[..count]);
        progress(total);
    }
    if total != length {
        return Err("The selected ISO changed size.".into());
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(all(windows, feature = "staged-iso-testing"))]
impl Downloads {
    fn import_testing_iso(&self, path: PathBuf, cancel: Arc<AtomicBool>) -> Result<(), String> {
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("iso"))
        {
            return Err("Choose an ISO file.".into());
        }
        let before = ReplacementTarget::inspect(&path)?;
        let guard = Arc::new(crate::iso_source::HeldIso::open(&path, before.length)?);
        self.update(|s| s.total_bytes = before.length);
        let sha256 = testing_iso_digest(&path, before.length, &cancel, |bytes| {
            self.update(|s| s.received_bytes = bytes);
        })?;
        before.check(&path)?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Download state is unavailable")?;
        if cancel.load(Ordering::Relaxed) {
            return Err("Image selection cancelled".into());
        }
        inner.snapshot.release = Some(ImageRelease::Local(crate::image_release::LocalImage {
            version: "local test image".into(),
            // Internal handoff name; the selected file is never renamed or copied.
            file_name: "omarchy-local-test.iso".into(),
            length: before.length,
            sha256,
            local_image: true,
        }));
        inner.snapshot.status = Status::Complete;
        inner.snapshot.image_path = Some(path);
        inner.snapshot.image_locked = true;
        inner.snapshot.verification_skipped = false;
        inner.snapshot.existing_image = true;
        inner.verified_iso = Some(guard);
        Ok(())
    }
}

#[tauri::command]
pub async fn choose_testing_iso(
    app: tauri::AppHandle,
    downloads: State<'_, Downloads>,
) -> Result<Snapshot, String> {
    #[cfg(not(all(windows, feature = "staged-iso-testing")))]
    {
        let _ = (app, downloads);
        Err("Local unsigned ISOs are available only in Windows testing builds.".into())
    }
    #[cfg(all(windows, feature = "staged-iso-testing"))]
    {
        if crate::operation_active(&app) || downloads.snapshot()?.status.active() {
            return Err("Wait for the current operation to finish before changing images.".into());
        }
        let picker_app = app.clone();
        let selected = tauri::async_runtime::spawn_blocking(move || {
            picker_app
                .dialog()
                .file()
                .set_title("Choose a local test ISO")
                .add_filter("ISO image", &["iso"])
                .blocking_pick_file()
        })
        .await
        .map_err(|e| e.to_string())?;
        let Some(selected) = selected else {
            return downloads.snapshot();
        };
        if crate::operation_active(&app) {
            return Err("Wait for setup to finish before changing images.".into());
        }
        let path = selected.into_path().map_err(|e| e.to_string())?;
        downloads.begin_resolution()?;
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut inner = downloads
                .inner
                .lock()
                .map_err(|_| "Download state is unavailable")?;
            inner.cancel = Arc::clone(&cancel);
            inner.snapshot.status = Status::Verifying;
        }
        let controller = downloads.inner().clone();
        std::thread::spawn(move || {
            if let Err(error) = controller.import_testing_iso(path, Arc::clone(&cancel)) {
                controller.finish_error(error, cancel.load(Ordering::Relaxed));
            }
        });
        downloads.snapshot()
    }
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
            snapshot.release = Some(release.into());
            detect_existing(snapshot);
            snapshot.status = Status::Ready;
        }),
        Ok(Err(error)) => controller.finish_error(error.to_string(), false),
        Err(error) => controller.finish_error(format!("Release lookup failed: {error}"), false),
    }
    controller.reuse_testing_image();
    controller.snapshot()
}

#[tauri::command]
pub fn start_download(downloads: State<'_, Downloads>) -> Result<Snapshot, String> {
    start_transfer(downloads.inner(), false)
}

#[tauri::command]
pub fn replace_download(downloads: State<'_, Downloads>) -> Result<Snapshot, String> {
    start_transfer(downloads.inner(), true)
}

fn start_transfer(downloads: &Downloads, replace: bool) -> Result<Snapshot, String> {
    if !replace && downloads.reuse_testing_image() {
        return downloads.snapshot();
    }
    let Transfer {
        release,
        cancel,
        destination,
        replacement,
    } = downloads.begin_transfer(replace)?;
    let controller = downloads.clone();
    // No path, release metadata, key, or URL is accepted from the webview.
    let task_controller = controller.clone();
    tauri::async_runtime::spawn(async move {
        let task = tauri::async_runtime::spawn_blocking(move || {
            let existing = expected_image_path(&destination, release.file_name())
                .ok()
                .filter(|path| !replace && fs::symlink_metadata(path).is_ok());
            controller.update(|snapshot| snapshot.existing_image = existing.is_some());
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
                    let before = ReplacementTarget::inspect(path)?;
                    #[cfg(windows)]
                    let guard = Arc::new(crate::iso_source::HeldIso::open(path, before.length)?);
                    let verified =
                        omarchy_release_client::verify_existing(&release, path, &cancel, progress);
                    if matches!(
                        &verified,
                        Err(omarchy_release_client::Error::Signature(_))
                            | Err(omarchy_release_client::Error::Cache(_))
                    ) && !cancel.load(Ordering::Relaxed)
                    {
                        let candidate = replacement_candidate(
                            &before,
                            release.length(),
                            release.sha256(),
                            &cancel,
                        )
                        .ok()
                        .flatten();
                        if let Ok(mut inner) = controller.inner.lock() {
                            inner.replacement = candidate;
                        }
                    }
                    verified.map_err(|e| e.to_string())?;
                    return Ok(VerifiedImage {
                        path: path.clone(),
                        #[cfg(windows)]
                        guard,
                    });
                }
                let image = omarchy_release_client::download(
                    &release,
                    &controller.cache,
                    &cancel,
                    progress,
                )
                .map_err(|error| error.to_string())?;
                controller.update(|snapshot| snapshot.status = Status::Saving);
                let path = export_image_with_replacement(
                    &image.path,
                    &destination,
                    &image.file_name,
                    image.length,
                    &image.sha256,
                    &cancel,
                    replacement.as_ref(),
                )?;
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

#[cfg(windows)]
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

#[cfg(test)]
fn export_image(
    source: &Path,
    directory: &Path,
    filename: &str,
    length: u64,
    sha256: &str,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    export_image_with_replacement(source, directory, filename, length, sha256, cancel, None)
}

fn export_image_with_replacement(
    source: &Path,
    directory: &Path,
    filename: &str,
    length: u64,
    sha256: &str,
    cancel: &AtomicBool,
    replacement: Option<&ReplacementTarget>,
) -> Result<PathBuf, String> {
    let target = expected_image_path(directory, filename)?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    if let Some(original) = replacement {
        original.check(&target)?;
    } else if target.try_exists().map_err(|error| error.to_string())? {
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
    if let Some(original) = replacement {
        original.check(&target)?;
        // The user requested replacement. Publish the fully checked copy with
        // an atomic replacement; never delete the old ISO before downloading.
        temporary.persist(&target).map_err(|error| {
            format!("Cannot replace the ISO; the existing file was retained: {error}")
        })?;
    } else {
        temporary
            .persist_noclobber(&target)
            .map_err(|error| format!("Cannot save without replacing an existing file: {error}"))?;
    }
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

    #[cfg(all(windows, feature = "staged-iso-testing"))]
    #[test]
    fn local_testing_image_is_locked_hashed_and_never_treated_as_official() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("My custom image.iso");
        fs::write(&path, b"ISO").unwrap();
        let controller = Downloads::new(temp.path().join("cache"), temp.path().to_path_buf());
        controller
            .import_testing_iso(path.clone(), Arc::new(AtomicBool::new(false)))
            .unwrap();
        let (selected, release) = controller.verified_source().unwrap();
        assert_eq!(selected, path);
        assert_eq!(release.sha256(), format!("{:x}", Sha256::digest(b"ISO")));
        assert!(release.signature().is_empty());
        assert!(release.official().is_err());
        assert!(controller.begin_transfer(false).is_err());
        assert!(!controller.reuse_testing_image());
        assert!(controller.snapshot().unwrap().image_locked);
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        let source = crate::setup_protocol::SourceImage {
            path: path.clone(),
            file_name: release.file_name().into(),
            length: release.length(),
            sha256: release.sha256().into(),
            signature: vec![],
        };
        assert!(controller.verified_lease(&source).is_some());
        let mut changed = source;
        changed.sha256 = "0".repeat(64);
        assert!(controller.verified_lease(&changed).is_none());
        controller.begin_resolution().unwrap();
        assert!(controller.snapshot().unwrap().release.is_none());
        assert!(OpenOptions::new().write(true).open(&path).is_ok());
    }

    #[cfg(all(windows, feature = "staged-iso-testing"))]
    #[test]
    fn cancelled_local_import_does_not_publish_an_image() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("local.iso");
        fs::write(&path, b"ISO").unwrap();
        let controller = Downloads::new(temp.path().join("cache"), temp.path().to_path_buf());
        assert!(controller
            .import_testing_iso(path.clone(), Arc::new(AtomicBool::new(true)))
            .is_err());
        assert!(controller.verified_source().is_err());
        assert!(OpenOptions::new().write(true).open(&path).is_ok());
    }

    #[test]
    fn testing_restart_reuses_existing_file_without_claiming_verification() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("image.iso");
        // Deliberately not a valid signed image: the testing shortcut only checks
        // the expected filename, size and ordinary-file constraints.
        fs::write(&path, b"image").unwrap();
        for _ in 0..2 {
            let controller = Downloads::new(temp.path().join("cache"), temp.path().to_path_buf());
            let mut inner = controller.inner.lock().unwrap();
            inner.snapshot.status = Status::Ready;
            let reused = Downloads::use_testing_file(&mut inner, "image.iso", 5);
            assert_eq!(reused, cfg!(feature = "staged-iso-testing"));
            assert_eq!(inner.snapshot.verification_skipped, reused);
            assert_eq!(
                inner.snapshot.status,
                if reused {
                    Status::Complete
                } else {
                    Status::Ready
                }
            );
            assert_eq!(inner.snapshot.image_path.as_ref(), reused.then_some(&path));
            drop(inner);
            controller.begin_resolution().unwrap();
            assert!(!controller.snapshot().unwrap().verification_skipped);
        }
    }

    #[test]
    fn testing_reuse_rejects_wrong_size_missing_files_and_active_transfers() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("image.iso"), b"partial").unwrap();
        let controller = Downloads::new(temp.path().join("cache"), temp.path().to_path_buf());
        let mut inner = controller.inner.lock().unwrap();
        inner.snapshot.status = Status::Ready;
        assert!(!Downloads::use_testing_file(&mut inner, "image.iso", 100));
        assert!(!Downloads::use_testing_file(&mut inner, "missing.iso", 7));
        assert!(!Downloads::use_testing_file(&mut inner, "../image.iso", 7));
        inner.snapshot.status = Status::Downloading;
        assert!(!Downloads::use_testing_file(&mut inner, "image.iso", 7));
        assert!(!inner.snapshot.verification_skipped);
    }

    #[test]
    fn replacement_requires_wrong_bytes_and_never_follows_links_or_cancellation() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("image.iso");
        fs::write(&path, b"image").unwrap();
        let hash = format!("{:x}", Sha256::digest(b"image"));
        let before = ReplacementTarget::inspect(&path).unwrap();
        assert!(
            replacement_candidate(&before, 5, &hash, &AtomicBool::new(false))
                .unwrap()
                .is_none()
        );
        assert!(
            replacement_candidate(&before, 6, &hash, &AtomicBool::new(true))
                .unwrap()
                .is_none()
        );
        let wrong_hash = format!("{:x}", Sha256::digest(b"other"));
        assert!(
            replacement_candidate(&before, 5, &wrong_hash, &AtomicBool::new(false))
                .unwrap()
                .is_some()
        );
        assert!(ReplacementTarget::inspect(temp.path()).is_err());
        #[cfg(unix)]
        {
            let link = temp.path().join("link.iso");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(ReplacementTarget::inspect(&link).is_err());
        }
    }

    #[test]
    fn replacement_preserves_original_until_a_valid_copy_is_ready() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("image.iso");
        let old_version = temp.path().join("older.iso");
        fs::write(&target, b"broken").unwrap();
        fs::write(&old_version, b"previous release").unwrap();
        let original = ReplacementTarget::inspect(&target).unwrap();
        let hash = format!("{:x}", Sha256::digest(b"image"));
        let save = |cancel: &AtomicBool| {
            export_image_with_replacement(
                &source,
                temp.path(),
                "image.iso",
                5,
                &hash,
                cancel,
                Some(&original),
            )
        };
        fs::write(&source, b"wrong").unwrap();
        assert!(save(&AtomicBool::new(false)).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"broken");
        fs::write(&source, b"image").unwrap();
        assert!(save(&AtomicBool::new(true)).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"broken");
        save(&AtomicBool::new(false)).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"image");
        assert_eq!(fs::read(&old_version).unwrap(), b"previous release");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 3);
    }

    #[test]
    fn replacement_refuses_a_changed_target_and_folder_changes_clear_the_offer() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("image.iso");
        let source = temp.path().join("source");
        fs::write(&path, b"broken").unwrap();
        fs::write(&source, b"image").unwrap();
        let original = ReplacementTarget::inspect(&path).unwrap();
        fs::write(&path, b"another file").unwrap();
        assert!(export_image_with_replacement(
            &source,
            temp.path(),
            "image.iso",
            5,
            &format!("{:x}", Sha256::digest(b"image")),
            &AtomicBool::new(false),
            Some(&original)
        )
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), b"another file");
        let downloads = Downloads::new(temp.path().join("cache"), temp.path().to_path_buf());
        {
            let mut inner = downloads.inner.lock().unwrap();
            inner.replacement = Some(original);
            inner.snapshot.replacement_available = true;
        }
        let folder = temp.path().join("new");
        fs::create_dir(&folder).unwrap();
        assert!(
            !downloads
                .set_destination(folder)
                .unwrap()
                .replacement_available
        );
        assert!(downloads.inner.lock().unwrap().replacement.is_none());
        assert!(downloads.begin_transfer(true).is_err());
    }

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
        controller.update(|snapshot| snapshot.release = Some(release.into()));
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
        assert!(controller.begin_transfer(false).is_err());
        controller.begin_resolution().unwrap();
        assert!(controller.begin_resolution().is_err());
        assert!(controller.begin_transfer(false).is_err());
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
