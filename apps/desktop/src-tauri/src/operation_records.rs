//! User-owned receipt export. Never pass this destination to privileged writes.
use crate::{provider_process, provider_runtime, setup_protocol::Destination};
use serde_json::{json, Value};
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc};
#[cfg(not(windows))]
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

pub struct Export {
    pub directory: PathBuf,
    pending: Option<tempfile::NamedTempFile>,
    // Keep checked paths stable until the operation and export have finished.
    _guards: Vec<File>,
}

#[cfg(windows)]
fn installer_directory() -> Result<PathBuf, String> {
    let executable = std::env::var_os("OMARCHY_PORTABLE_EXE")
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(std::env::current_exe)
        .map_err(|e| e.to_string())?;
    if !executable.is_absolute() {
        return Err("The portable executable location is unavailable".into());
    }
    executable
        .parent()
        .map(Path::to_path_buf)
        .ok_or("The portable executable folder is unavailable".into())
}

fn lock_path(path: &Path) -> Result<Vec<File>, String> {
    let mut guards = Vec::new();
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if !current.has_root() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&current).map_err(|e| e.to_string())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("Choose a records folder without symbolic links".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::{fs::MetadataExt, fs::OpenOptionsExt};
            use windows_sys::Win32::Storage::FileSystem::*;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err("Choose a records folder without reparse points".into());
            }
            let file = std::fs::OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&current)
                .map_err(|e| e.to_string())?;
            if file
                .metadata()
                .map_err(|e| e.to_string())?
                .file_attributes()
                & FILE_ATTRIBUTE_REPARSE_POINT
                != 0
            {
                return Err("The records folder changed during validation".into());
            }
            guards.push(file);
        }
    }
    Ok(guards)
}

fn outside_usb(directory: &Path, destination: &Destination) -> Result<(), String> {
    let identity = match destination {
        Destination::Usb { identity } | Destination::UsbPreserve { identity, .. } => identity,
        _ => return Ok(()),
    };
    let manifest = provider_runtime::manifest()?;
    let root = provider_runtime::locate(&manifest)?;
    let cancel = Arc::new(AtomicBool::new(false));
    provider_runtime::verify_inspection(&root, &manifest, &cancel)?;
    let inventory = provider_process::run(
        provider_process::inspection_command(&root, &manifest)?,
        Some(json!({"protocol":1,"action":"list","sourcePath":directory})),
        cancel,
        None,
        false,
        |_| {},
    )?;
    let drive = inventory["drives"]
        .as_array()
        .and_then(|drives| drives.iter().find(|drive| drive["identity"] == *identity))
        .ok_or("The selected USB drive changed; refresh the drive list")?;
    if drive["reasons"]
        .as_array()
        .is_some_and(|reasons| reasons.iter().any(|reason| reason == "SOURCE_DEVICE"))
    {
        return Err(
            "The records folder is on the selected USB. Choose a folder on another drive".into(),
        );
    }
    Ok(())
}

impl Export {
    fn prepare(parent: &Path, destination: &Destination) -> Result<Self, String> {
        let mut guards = lock_path(parent)?;
        outside_usb(parent, destination)?;
        let directory = parent.join("Omarchy-Setup-Records");
        match std::fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(format!(
                    "Cannot create records beside the installer: {error}"
                ))
            }
        }
        guards.extend(lock_path(&directory)?);
        let pending = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
        Ok(Self {
            directory,
            pending: Some(pending),
            _guards: guards,
        })
    }

    pub fn choose(
        app: &tauri::AppHandle,
        destination: &Destination,
    ) -> Result<Option<Self>, String> {
        if !matches!(
            destination,
            Destination::Usb { .. }
                | Destination::UsbPreserve { .. }
                | Destination::DirectX86 { .. }
        ) {
            return Ok(None);
        }
        #[cfg(windows)]
        let mut candidate = installer_directory();
        // Installed Linux binaries and signed .app bundles are not receipt
        // folders. Keep user-owned records outside application resources.
        #[cfg(not(windows))]
        let mut candidate = app.path().document_dir().map_err(|error| error.to_string());
        loop {
            let result = candidate.and_then(|parent| Self::prepare(&parent, destination));
            match result {
                Ok(export) => return Ok(Some(export)),
                Err(error) => {
                    app.dialog()
                        .message(format!(
                            "{error}. Choose where to keep the operation records."
                        ))
                        .title("Operation records")
                        .blocking_show();
                    candidate = app
                        .dialog()
                        .file()
                        .set_title("Choose a folder for Omarchy operation records")
                        .blocking_pick_folder()
                        .ok_or("No records folder was selected")?
                        .into_path()
                        .map_err(|e| e.to_string());
                }
            }
        }
    }

    pub fn save(&mut self, plan: &Value) -> Result<PathBuf, String> {
        let id = plan["operationId"]
            .as_str()
            .ok_or("Missing operation record identity")?;
        let id = uuid::Uuid::parse_str(id).map_err(|_| "Invalid operation record identity")?;
        let records = &plan["records"];
        if !records["receipt"].is_object() || !records["cleanup"].is_object() {
            return Err("Incomplete operation records".into());
        }
        let bytes = serde_json::to_vec_pretty(records).map_err(|e| e.to_string())?;
        let file = self
            .pending
            .as_mut()
            .ok_or("Operation records were already exported")?;
        file.as_file_mut().set_len(0).map_err(|e| e.to_string())?;
        file.as_file_mut().rewind().map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.as_file_mut().rewind().map_err(|e| e.to_string())?;
        let mut readback = Vec::new();
        file.read_to_end(&mut readback).map_err(|e| e.to_string())?;
        if readback != bytes {
            return Err("Operation record readback failed".into());
        }
        let path = self.directory.join(format!("operation-{id}.json"));
        let published = self
            .pending
            .take()
            .unwrap()
            .persist_noclobber(&path)
            .map_err(|e| e.to_string())?;
        published.sync_all().map_err(|e| e.to_string())?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipt_export_is_readable_and_never_replaces_existing_records() {
        let temp = tempfile::tempdir().unwrap();
        let id = uuid::Uuid::new_v4();
        let plan = json!({"operationId":id,"records":{"receipt":{"verified":true},"cleanup":{"complete":true}}});
        let mut export = Export::prepare(temp.path(), &Destination::InspectDirect).unwrap();
        let path = export.save(&plan).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(),
            plan["records"]
        );
        let mut duplicate = Export::prepare(temp.path(), &Destination::InspectDirect).unwrap();
        assert!(duplicate.save(&plan).is_err());
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap(),
            plan["records"]
        );
    }
    #[test]
    fn abandoned_export_does_not_leave_a_pending_file() {
        let temp = tempfile::tempdir().unwrap();
        let export = Export::prepare(temp.path(), &Destination::InspectDirect).unwrap();
        let path = export.directory.clone();
        drop(export);
        assert_eq!(std::fs::read_dir(path).unwrap().count(), 0);
    }
}
