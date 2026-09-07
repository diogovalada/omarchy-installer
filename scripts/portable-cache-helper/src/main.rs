//! Freshly extracted verifier. Never execute code from an unchecked cache.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, PathBuf};
use windows_sys::Win32::Foundation::{CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_FLAG_SEQUENTIAL_SCAN, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE};
use windows_sys::Win32::UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    files: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    path: String,
    size_bytes: u64,
    sha256: String,
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn valid_relative(s: &str) -> bool {
    !s.is_empty()
        && s.len() < 512
        && s.split('/').all(|part| {
            let base = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            !part.is_empty()
                && ![".", ".."].contains(&part)
                && !part.ends_with(['.', ' '])
                && !part
                    .chars()
                    .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
                && ![
                    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
                    "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
                    "LPT8", "LPT9",
                ]
                .contains(&base.as_str())
        })
}
fn load_manifest(path: &Path, expected: &str) -> Result<Manifest> {
    if !valid_hash(expected) {
        return Err("Invalid embedded manifest digest".into());
    }
    let mut file = File::open(path)?;
    if file.metadata()?.len() > 16 * 1024 * 1024 {
        return Err("Manifest is too large".into());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if hex_digest(&bytes) != expected {
        return Err("Embedded manifest digest mismatch".into());
    }
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    if manifest.schema_version != 1 || manifest.files.is_empty() || manifest.files.len() > 50_000 {
        return Err("Invalid payload manifest".into());
    }
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    for entry in &manifest.files {
        if !valid_relative(&entry.path)
            || !valid_hash(&entry.sha256)
            || entry.size_bytes > 256 * 1024 * 1024
            || !seen.insert(entry.path.to_lowercase())
        {
            return Err("Invalid payload entry".into());
        }
        total = total
            .checked_add(entry.size_bytes)
            .ok_or("Payload size overflow")?;
    }
    if total > 1024 * 1024 * 1024 || !seen.contains("omarchy setup.exe") {
        return Err("Invalid application payload".into());
    }
    Ok(manifest)
}

fn open_directory(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_dir() || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err("Cache directories must be ordinary directories, without links".into());
    }
    Ok(file)
}
// Hold ancestors without DELETE sharing, so they cannot be redirected during use.
fn directory_chain(path: &Path, create: bool) -> Result<Vec<File>> {
    if !path.is_absolute() {
        return Err("An absolute cache path is required".into());
    }
    let mut current = PathBuf::new();
    let mut handles = Vec::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => {
                current.push(component);
                continue;
            }
            Component::RootDir | Component::Normal(_) => current.push(component),
            _ => return Err("Invalid cache path component".into()),
        }
        if create && !current.exists() {
            fs::create_dir(&current)?;
        }
        handles.push(open_directory(&current)?);
    }
    Ok(handles)
}

fn verify(root: &Path, manifest: &Manifest) -> Result<Vec<File>> {
    let mut handles = directory_chain(root, false)?;
    let mut expected_files = BTreeMap::new();
    let mut expected_dirs = BTreeSet::new();
    for entry in &manifest.files {
        expected_files.insert(entry.path.to_lowercase(), entry);
        let mut parent = Path::new(&entry.path).parent();
        while let Some(dir) = parent.filter(|p| !p.as_os_str().is_empty()) {
            expected_dirs.insert(dir.to_string_lossy().replace('\\', "/").to_lowercase());
            parent = dir.parent();
        }
    }
    let mut todo = vec![PathBuf::new()];
    let mut visited = BTreeSet::new();
    while let Some(relative) = todo.pop() {
        for found in fs::read_dir(root.join(&relative))? {
            let found = found?;
            let child = relative.join(found.file_name());
            let key = child.to_string_lossy().replace('\\', "/").to_lowercase();
            if !visited.insert(key.clone()) {
                return Err("Duplicate cache entry".into());
            }
            if visited.len() > expected_files.len() + expected_dirs.len() {
                return Err("Unexpected cache entries".into());
            }
            let meta = fs::symlink_metadata(found.path())?;
            if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err("Cache links are not permitted".into());
            }
            if meta.is_dir() {
                if !expected_dirs.contains(&key) {
                    return Err("Unexpected cache directory".into());
                }
                handles.push(open_directory(&found.path())?);
                todo.push(child);
            } else {
                let expected = expected_files.get(&key).ok_or("Unexpected cache file")?;
                let mut file = OpenOptions::new()
                    .read(true)
                    .share_mode(FILE_SHARE_READ)
                    .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_SEQUENTIAL_SCAN)
                    .open(found.path())?;
                let meta = file.metadata()?;
                if !meta.is_file()
                    || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                    || meta.len() != expected.size_bytes
                {
                    return Err("Cache file type or size mismatch".into());
                }
                let mut digest = Sha256::new();
                let mut buffer = [0_u8; 128 * 1024];
                loop {
                    let count = file.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    digest.update(&buffer[..count]);
                }
                if format!("{:x}", digest.finalize()) != expected.sha256 {
                    return Err("Cache file hash mismatch".into());
                }
                handles.push(file);
            }
        }
    }
    if visited.len() != expected_files.len() + expected_dirs.len() {
        return Err("Incomplete cache".into());
    }
    Ok(handles)
}

// Transfer verified file and directory handles to the launcher. Windows keeps
// their write/delete protection until that launcher (and its app) exits.
fn lease_to_launcher(handles: &[File], parent: u32) -> Result<()> {
    if parent == 0 {
        return Err("Missing launcher process".into());
    }
    unsafe {
        let process = OpenProcess(PROCESS_DUP_HANDLE, 0, parent);
        if process.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        for file in handles {
            let mut duplicate = std::ptr::null_mut();
            if DuplicateHandle(
                GetCurrentProcess(),
                file.as_raw_handle(),
                process,
                &mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            ) == 0
            {
                let error = std::io::Error::last_os_error();
                CloseHandle(process);
                // Already transferred handles close when the failed launcher exits.
                return Err(error.into());
            }
        }
        CloseHandle(process);
    }
    Ok(())
}

fn local_cache_base() -> Result<PathBuf> {
    unsafe {
        let mut wide = std::ptr::null_mut();
        if SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, std::ptr::null_mut(), &mut wide) < 0 {
            return Err("Local application data folder is unavailable".into());
        }
        let mut length = 0;
        while length < 32768 && *wide.add(length) != 0 {
            length += 1;
        }
        let path = PathBuf::from(std::ffi::OsString::from_wide(std::slice::from_raw_parts(
            wide, length,
        )));
        CoTaskMemFree(wide.cast());
        Ok(path.join("OmarchySetup").join("p"))
    }
}
fn check_cache_path(root: &Path, hash: &str) -> Result<()> {
    let expected = local_cache_base()?.join(&hash[..20]);
    if root.to_string_lossy().to_lowercase() != expected.to_string_lossy().to_lowercase() {
        return Err("Cache path is outside the designated application cache".into());
    }
    Ok(())
}
fn staging_path(root: &Path) -> PathBuf {
    root.with_file_name(format!(
        "{}.part",
        root.file_name().unwrap_or_default().to_string_lossy()
    ))
}
fn quarantine(path: &Path, category: &str) -> Result<()> {
    // Check the exact resolved parent and entry before a directory rename.
    let _parents = directory_chain(path.parent().ok_or("Missing cache parent")?, false)?;
    let guard = open_directory(path)?;
    drop(guard);
    let target = path.with_file_name(format!(
        "{}-{}-{}",
        category,
        path.file_name().unwrap_or_default().to_string_lossy(),
        &uuid::Uuid::new_v4().to_string()[..8]
    ));
    fs::rename(path, target)?;
    Ok(())
}
fn prepare(root: &Path) -> Result<()> {
    let _parents = directory_chain(root.parent().ok_or("Missing cache parent")?, true)?;
    let stage = staging_path(root);
    if stage.try_exists()? {
        quarantine(&stage, "interrupted")?;
    }
    fs::create_dir(stage)?;
    Ok(())
}
fn publish(root: &Path, manifest: &Manifest, parent: u32) -> Result<()> {
    let _parents = directory_chain(root.parent().ok_or("Missing cache parent")?, false)?;
    let stage = staging_path(root);
    // Verify the complete new generation before making it the reusable cache.
    drop(verify(&stage, manifest)?);
    if root.try_exists()? {
        quarantine(root, "invalid")?;
    }
    fs::rename(&stage, root)?;
    // Reopen at the final name and retain exact verified handles during execution.
    let handles = verify(root, manifest)?;
    lease_to_launcher(&handles, parent)
}
fn run() -> Result<i32> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 6 {
        return Err("Invalid cache verifier arguments".into());
    }
    let command = args[1].to_str().ok_or("Invalid operation")?;
    let hash = args[3].to_str().ok_or("Invalid manifest digest")?;
    let manifest = load_manifest(Path::new(&args[2]), hash)?;
    let root = Path::new(&args[4]);
    let parent = args[5]
        .to_str()
        .ok_or("Invalid launcher ID")?
        .parse::<u32>()?;
    if command == "verify" {
        drop(verify(root, &manifest)?);
        return Ok(0);
    }
    if parent == 0 {
        return Err("Missing launcher process".into());
    }
    check_cache_path(root, hash)?;
    let ancestors = directory_chain(root.parent().ok_or("Missing cache parent")?, true)?;
    match command {
        "check" => {
            match verify(root, &manifest) {
                Ok(handles) => lease_to_launcher(&handles, parent)?,
                Err(_) => return Ok(10), // Cache miss or damage: extract a fresh generation.
            }
        }
        "prepare" => {
            prepare(root)?;
            lease_to_launcher(&ancestors, parent)?;
        }
        "publish" => publish(root, &manifest, parent)?,
        _ => return Err("Unknown cache operation".into()),
    }
    Ok(0)
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "Portable cache: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path) -> Manifest {
        fs::create_dir_all(root.join("providers")).unwrap();
        fs::write(root.join("Omarchy Setup.exe"), b"application").unwrap();
        fs::write(root.join("providers/runtime.bin"), b"runtime").unwrap();
        Manifest {
            schema_version: 1,
            files: vec![
                Entry {
                    path: "Omarchy Setup.exe".into(),
                    size_bytes: 11,
                    sha256: hex_digest(b"application"),
                },
                Entry {
                    path: "providers/runtime.bin".into(),
                    size_bytes: 7,
                    sha256: hex_digest(b"runtime"),
                },
            ],
        }
    }
    #[test]
    fn complete_cache_is_reusable_without_changing_files() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = fixture(dir.path());
        let before = fs::metadata(dir.path().join("Omarchy Setup.exe"))
            .unwrap()
            .created()
            .unwrap();
        drop(verify(dir.path(), &manifest).unwrap());
        drop(verify(dir.path(), &manifest).unwrap());
        assert_eq!(
            before,
            fs::metadata(dir.path().join("Omarchy Setup.exe"))
                .unwrap()
                .created()
                .unwrap()
        );
    }
    #[test]
    fn same_size_tampering_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = fixture(dir.path());
        fs::write(dir.path().join("providers/runtime.bin"), b"changed").unwrap();
        assert!(verify(dir.path(), &manifest).is_err());
    }
    #[test]
    fn a_new_payload_manifest_does_not_accept_an_older_cache() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = fixture(dir.path());
        manifest.files[1].sha256 = hex_digest(b"updated");
        assert!(verify(dir.path(), &manifest).is_err());
    }
    #[test]
    fn missing_files_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = fixture(dir.path());
        fs::remove_file(dir.path().join("providers/runtime.bin")).unwrap();
        assert!(verify(dir.path(), &manifest).is_err());
    }
    #[test]
    fn unexpected_dll_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = fixture(dir.path());
        fs::write(dir.path().join("unexpected.dll"), b"dll").unwrap();
        assert!(verify(dir.path(), &manifest).is_err());
    }
    #[test]
    fn checked_files_cannot_be_modified_or_renamed_while_leased() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = fixture(dir.path());
        let handles = verify(dir.path(), &manifest).unwrap();
        assert!(fs::write(dir.path().join("Omarchy Setup.exe"), b"changed").is_err());
        assert!(fs::rename(dir.path().join("providers"), dir.path().join("moved")).is_err());
        drop(handles);
        assert!(fs::write(dir.path().join("Omarchy Setup.exe"), b"changed").is_ok());
    }
    #[test]
    fn interrupted_staging_is_preserved_and_restarted() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("cache");
        prepare(&root).unwrap();
        fs::write(staging_path(&root).join("partial"), b"partial").unwrap();
        prepare(&root).unwrap();
        assert_eq!(fs::read_dir(staging_path(&root)).unwrap().count(), 0);
        assert!(fs::read_dir(dir.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("interrupted-")));
        assert!(!root.exists());
    }
    #[test]
    fn wrong_embedded_hash_and_unsafe_names_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        fs::write(&path, b"{}").unwrap();
        assert!(load_manifest(&path, &"0".repeat(64)).is_err());
        for name in [
            "../escape",
            "C:/escape",
            "/absolute",
            "dir\\file",
            "file.",
            "NUL.txt",
            "file:stream",
        ] {
            assert!(!valid_relative(name));
        }
    }
}
