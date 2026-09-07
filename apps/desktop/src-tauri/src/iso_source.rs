//! Retain the original Windows ISO and every ancestor until consumers stop.
use std::fs::{File, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf, Prefix};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

pub(crate) struct HeldIso {
    path: PathBuf,
    _file: File,
    _directories: Vec<File>,
}

impl HeldIso {
    pub(crate) fn open(path: &Path, length: u64) -> Result<Self, String> {
        let parts: Vec<_> = path.components().collect();
        if !matches!(parts.first(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)))
            || !matches!(parts.get(1), Some(Component::RootDir))
            || parts.len() < 3
            || parts[2..].iter().any(|part| !matches!(part, Component::Normal(name) if !name.to_string_lossy().contains(':')))
        {
            return Err("The ISO must use an absolute local drive path without traversal or alternate streams.".into());
        }
        let mut cursor = PathBuf::new();
        let mut directories = Vec::new();
        for part in &parts[..parts.len() - 1] {
            cursor.push(part.as_os_str());
            if matches!(part, Component::Prefix(_)) {
                continue;
            }
            let directory = OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&cursor)
                .map_err(|e| format!("Cannot hold the ISO folder open: {e}"))?;
            let metadata = directory.metadata().map_err(|e| e.to_string())?;
            if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            {
                return Err(
                    "The ISO path contains a link, junction or unexpected directory.".into(),
                );
            }
            directories.push(directory);
        }
        // READ sharing permits the verifier, Node writer and container reader;
        // existing or new write/delete handles are incompatible with this guard.
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|e| format!("Cannot hold the ISO open. Close programs modifying or moving it and try again: {e}"))?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file()
            || metadata.len() != length
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err("The ISO changed, is a link, or has an unexpected size.".into());
        }
        Ok(Self {
            path: path.to_path_buf(),
            _file: file,
            _directories: directories,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn original_remains_readable_but_cannot_be_written_deleted_or_moved_until_release() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("original folder");
        fs::create_dir(&parent).unwrap();
        let path = parent.join("installer.iso");
        fs::write(&path, b"original ISO").unwrap();
        let held = HeldIso::open(&path, 12).unwrap();
        assert_eq!(held.path(), path);
        assert_eq!(fs::read(&path).unwrap(), b"original ISO");
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(fs::remove_file(&path).is_err());
        assert!(fs::rename(&path, parent.join("moved.iso")).is_err());
        assert!(fs::rename(&parent, dir.path().join("moved folder")).is_err());
        assert_eq!(fs::read_dir(&parent).unwrap().count(), 1);
        drop(held);
        fs::write(&path, b"editable again").unwrap();
        fs::rename(&parent, dir.path().join("moved folder")).unwrap();
    }

    #[test]
    fn conflicting_writer_or_wrong_length_fails_and_releases_partial_guards() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("installer.iso");
        fs::write(&path, b"ISO").unwrap();
        let writer = OpenOptions::new().write(true).open(&path).unwrap();
        assert!(HeldIso::open(&path, 3).is_err());
        drop(writer);
        assert!(HeldIso::open(&path, 4).is_err());
        fs::rename(&path, dir.path().join("renamed.iso")).unwrap();
    }

    #[test]
    fn rejects_alternate_namespaces_streams_and_traversal() {
        for path in [
            r"C:installer.iso",
            r"\\server\share\installer.iso",
            r"\\.\PhysicalDrive1",
            r"\\?\C:\installer.iso",
            r"C:\folder\..\installer.iso",
            r"C:\installer.iso:stream",
        ] {
            assert!(HeldIso::open(Path::new(path), 1).is_err(), "{path}");
        }
    }

    #[test]
    #[ignore = "requires the pinned local Docker runtime; reads disposable fixture files only"]
    fn container_reads_separate_held_files_with_read_only_mounts() {
        let dir = tempfile::tempdir().unwrap();
        let iso = dir.path().join("original name.iso");
        let signatures = dir.path().join("protected signature");
        fs::create_dir(&signatures).unwrap();
        let signature = signatures.join("omarchy-4.0.2.iso.sig");
        fs::write(&iso, b"fixture ISO").unwrap();
        fs::write(&signature, b"fixture signature").unwrap();
        let _held = HeldIso::open(&iso, 11).unwrap();
        let docker = std::env::var_os("OMARCHY_TEST_DOCKER").expect("Docker executable required");
        let image = std::env::var("OMARCHY_TEST_RUNTIME").expect("Pinned runtime required");
        assert!(image.starts_with("sha256:"));
        let output = std::process::Command::new(docker)
            .args(["run", "--rm", "--pull=never", "--network", "none", "--read-only", "--cap-drop", "ALL",
                "--security-opt", "no-new-privileges", "--user", "1000:1000", "--mount"])
            .arg(format!("type=bind,source={},target=/input/omarchy-4.0.2.iso,readonly", iso.display()))
            .arg("--mount")
            .arg(format!("type=bind,source={},target=/input/omarchy-4.0.2.iso.sig,readonly", signature.display()))
            .args(["--entrypoint", "python3", &image, "-c",
                "from pathlib import Path\np=Path('/input/omarchy-4.0.2.iso')\nassert p.read_bytes()==b'fixture ISO'\nassert p.with_suffix('.iso.sig').read_bytes()==b'fixture signature'\ntry:\n p.open('wb')\nexcept OSError:\n pass\nelse:\n raise AssertionError('input mount was writable')\nprint('separate held inputs readable; ISO mount read-only')"])
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(&iso).unwrap(), b"fixture ISO");
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
}
