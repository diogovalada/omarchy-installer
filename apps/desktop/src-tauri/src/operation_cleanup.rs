//! Remove only known transient files from this helper's newly created workspace.
//! No recursive deletion, partition cleanup, recovery-task deletion or input paths.
use crate::provider_runtime::{checked_path, RuntimeManifest};
use serde_json::{json, Value};
use std::fs;
use std::path::{Component, Path};

fn remove_file(root: &Path, relative: &str) -> Result<u64, String> {
    let target = checked_path(root, relative)?;
    let mut cursor = root.to_path_buf();
    let mut components = vec![None];
    components.extend(Path::new(relative).components().map(Some));
    for component in components {
        if let Some(Component::Normal(part)) = component {
            cursor.push(part);
        }
        let metadata = match fs::symlink_metadata(&cursor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.to_string()),
        };
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let reparse = false;
        if metadata.file_type().is_symlink() || reparse {
            return Err("Transient path contains a link".into());
        }
        if cursor == target {
            if !metadata.is_file() {
                return Err("Transient path is not a regular file".into());
            }
            let length = metadata.len();
            fs::remove_file(&target).map_err(|error| error.to_string())?;
            return Ok(length);
        }
        if !metadata.is_dir() {
            return Err("Transient parent is not a directory".into());
        }
    }
    Err("Invalid transient path".into())
}

pub fn finish(
    root: &Path,
    operation: &str,
    source_name: &str,
    manifest: &RuntimeManifest,
    success: bool,
) -> Value {
    if uuid::Uuid::parse_str(operation).is_err()
        || root.file_name().and_then(|name| name.to_str())
            != Some(format!("Omarchy-Setup-{operation}").as_str())
    {
        return json!({"complete":false,"message":"Workspace identity could not be established; temporary files retained."});
    }
    let mut paths = vec![source_name.to_owned(), format!("{source_name}.sig")];
    paths.extend(
        manifest
            .files
            .iter()
            .map(|file| format!("providers/{}", file.path)),
    );
    // A failed provider must positively record that its exact Docker container
    // stopped. Do not unlink its backing files based solely on a lost pipe.
    let build_stopped = root.join("image-build-stopped.json").is_file();
    if !success && !build_stopped && root.join("image").exists() {
        return json!({"complete":false,"removedBytes":0,"recordsPreserved":true,"secureErasureClaimed":false,
                      "message":"Construction stop was not confirmed. Source, runtime and temporary images are retained."});
    }
    if success || build_stopped {
        paths.extend(
            [
                "image/image.building.qcow2",
                "image/esp.img.enc",
                "image/root.img.enc",
                "image/cidata.img",
                "image/vmlinuz-linux",
                "image/initramfs-linux.img",
                "image/OVMF_VARS.fd",
            ]
            .map(str::to_owned),
        );
    }
    let mut removed_bytes = 0u64;
    let mut retained = vec![];
    for path in paths {
        match remove_file(root, &path) {
            Ok(bytes) => removed_bytes = removed_bytes.saturating_add(bytes),
            Err(_) => retained.push(path),
        }
    }
    if !success && !build_stopped && root.join("image/image.building.qcow2").exists() {
        retained.push("image/image.building.qcow2 (construction stop not confirmed)".into());
    }
    json!({"complete":retained.is_empty(),"removedBytes":removed_bytes,"retained":retained,
           "recordsPreserved":true,"secureErasureClaimed":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_only_removes_the_named_file() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("image")).unwrap();
        fs::write(directory.path().join("image/root.img.enc"), b"ciphertext").unwrap();
        fs::write(directory.path().join("image/plan.json"), b"preserve").unwrap();
        assert_eq!(
            remove_file(directory.path(), "image/root.img.enc").unwrap(),
            10
        );
        assert!(directory.path().join("image/plan.json").is_file());
        assert!(remove_file(directory.path(), "../outside").is_err());
        assert!(remove_file(directory.path(), "image").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn cleanup_rejects_a_link_to_another_directory() {
        let directory = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        fs::write(other.path().join("root.img.enc"), b"keep").unwrap();
        std::os::unix::fs::symlink(other.path(), directory.path().join("image")).unwrap();
        assert!(remove_file(directory.path(), "image/root.img.enc").is_err());
        assert!(other.path().join("root.img.enc").exists());
    }
}
