//! File-only USB preparation. No formatting, partition editing or sector writes.
use crate::provider_process;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const ISO_HASH: &str = "2ef8e624aa1bec7e277e28056b8535a6c9373ba48d7ede3f1a01cb6d2373cfb8";
const RESERVE: u64 = 64 * 1024 * 1024;
const NAMESPACE: &str = "Omarchy-USB-Recovery";
const FALLBACK: &str = "EFI/BOOT/BOOTX64.EFI";
const CONFIG: &str = "EFI/Omarchy/grub.cfg";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Volume {
    root: PathBuf,
    partition: u32,
    guid: String,
    offset: u64,
    size: u64,
    file_system: String,
    free_bytes: u64,
    partition_type: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    data: Volume,
    efi: Volume,
    old_boot_hash: Option<String>,
    recovery: Option<Record>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    schema: u32,
    operation: String,
    disk_fingerprint: String,
    data_guid: String,
    efi_guid: String,
    old_boot_hash: Option<String>,
    loader_hash: String,
    config_hash: String,
    iso_hash: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub eligible: bool,
    pub reasons: Vec<String>,
    pub free_bytes: Option<u64>,
    pub required_bytes: u64,
    pub replaces_bootloader: bool,
    pub needs_administrator: bool,
    pub plan: Option<Plan>,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn io_error(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        "Administrator access is required to inspect this USB’s EFI files.".into()
    } else {
        error.to_string()
    }
}
fn no_link(path: &Path) -> Result<fs::Metadata, String> {
    let meta = fs::symlink_metadata(path).map_err(io_error)?;
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    if meta.file_type().is_symlink() || reparse {
        return Err(
            "A USB path contains a link or junction; preserving files is unavailable.".into(),
        );
    }
    Ok(meta)
}
fn require_plain_storage(metadata: &fs::Metadata) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & (0x800 | 0x4000) != 0 {
            return Err(
                "Compressed or EFS-encrypted USB storage is not supported for keeping files."
                    .into(),
            );
        }
    }
    #[cfg(not(windows))]
    let _ = metadata;
    Ok(())
}
// Hold all existing ancestors without FILE_SHARE_DELETE. Windows cannot replace
// a checked directory with a junction while these guards remain alive.
fn hold_directory(path: &Path) -> Result<File, String> {
    if !no_link(path)?.is_dir() {
        return Err("Expected a USB directory".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x02200000);
    }
    let file = options.open(path).map_err(io_error)?;
    if !no_link(path)?.is_dir() {
        return Err("USB directory changed".into());
    }
    Ok(file)
}
fn hold_path(
    root: &Path,
    relative: &str,
    create: bool,
    guards: &mut Vec<File>,
) -> Result<PathBuf, String> {
    let parts: Vec<_> = relative.split('/').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains([':', '\\']))
    {
        return Err("Invalid owned USB path".into());
    }
    let mut path = root.to_path_buf();
    guards.push(hold_directory(&path)?);
    for part in &parts[..parts.len() - 1] {
        path.push(part);
        if !path.try_exists().map_err(|e| e.to_string())? && create {
            fs::create_dir(&path).map_err(|e| e.to_string())?;
        }
        guards.push(hold_directory(&path)?);
    }
    path.push(parts.last().unwrap());
    if path.try_exists().map_err(|e| e.to_string())? && !no_link(&path)?.is_file() {
        return Err("A reserved USB filename is occupied by a directory".into());
    }
    Ok(path)
}
fn read_small(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let meta = no_link(path)?;
    if !meta.is_file() || meta.len() > limit {
        return Err("USB boot metadata is not a bounded regular file".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1).custom_flags(0x00200000);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    let mut bytes = vec![];
    std::io::Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("USB boot metadata grew unexpectedly".into());
    }
    Ok(bytes)
}
fn optional_hash(root: &Path, relative: &str) -> Result<Option<String>, String> {
    let mut path = root.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
        if !path.try_exists().map_err(|e| e.to_string())? {
            return Ok(None);
        }
        no_link(&path)?;
    }
    Ok(Some(digest(&read_small(&path, 32 * 1024 * 1024)?)))
}

pub fn inspect(identity: &Value, length: u64, sha256: &str) -> Inspection {
    let mut result = Inspection {
        eligible: false,
        reasons: vec![],
        free_bytes: None,
        required_bytes: length.saturating_add(RESERVE),
        replaces_bootloader: false,
        needs_administrator: false,
        plan: None,
    };
    match inspect_plan_observe(identity, &mut |free| {
        result.free_bytes = Some(free);
    }) {
        Ok(plan) => {
            result.free_bytes = Some(plan.data.free_bytes);
            result.replaces_bootloader = plan.old_boot_hash.is_some();
            if sha256 != ISO_HASH {
                result
                    .reasons
                    .push("Keeping files is currently qualified only for Omarchy 4.0.2.".into());
            }
            if plan.efi.free_bytes < 16 * 1024 * 1024 {
                result
                    .reasons
                    .push("The EFI partition needs at least 16 MiB free.".into());
            }
            if plan.recovery.is_some() {
                result.reasons.push("An Omarchy preparation record already exists. Adding another installer is unavailable; automatic cleanup and restoration are deferred.".into());
            }
            result.eligible = result.reasons.is_empty();
            result.plan = Some(plan);
        }
        Err(error) => {
            result.needs_administrator =
                error.contains("Access") || error.contains("access") || error.contains("denied");
            result.reasons.push(error);
        }
    }
    // Report capacity independently, including when the partition layout fails.
    if result
        .free_bytes
        .is_some_and(|free| free < result.required_bytes)
    {
        result
            .reasons
            .push("There is not enough free space for the ISO and a 64 MiB safety reserve.".into());
        result.eligible = false;
    }
    result
}

fn inspect_plan(identity: &Value) -> Result<Plan, String> {
    inspect_plan_observe(identity, &mut |_| {})
}
fn powershell_query(script: &str, input: Option<&str>) -> Result<Vec<u8>, String> {
    let executable = provider_process::system_powershell()?;
    let mut command = std::process::Command::new(&executable);
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        script,
    ]);
    command.env(
        "PSModulePath",
        executable
            .parent()
            .ok_or("Missing Windows PowerShell directory")?
            .join("Modules"),
    );
    provider_process::hide(&mut command);
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(io_error)?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .ok_or("Missing inspector input")?
            .write_all(input.as_bytes())
            .map_err(io_error)?;
    } else {
        drop(child.stdin.take());
    }
    let stdout = child.stdout.take().ok_or("Missing inspector output")?;
    let stderr = child.stderr.take().ok_or("Missing inspector diagnostics")?;
    let read = |mut stream: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = vec![];
            stream
                .by_ref()
                .take(256 * 1024)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
                .map_err(io_error)
        })
    };
    let output = read(Box::new(stdout));
    let errors = read(Box::new(stderr));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(io_error)? {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                "USB compatibility inspection timed out. Reconnect the USB and check again.".into(),
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };
    let output = output.join().map_err(|_| "USB inspector output failed")??;
    let errors = errors
        .join()
        .map_err(|_| "USB inspector diagnostics failed")??;
    if !status.success() {
        return Err(String::from_utf8_lossy(&errors)
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" "));
    }
    if output.len() >= 256 * 1024 {
        return Err("USB inventory exceeded its size limit".into());
    }
    Ok(output)
}
fn inspect_plan_observe(
    identity: &Value,
    observe_free: &mut impl FnMut(u64),
) -> Result<Plan, String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("Keeping existing files currently requires Windows x64.".into());
    }
    let device = identity["device"]
        .as_str()
        .ok_or("USB device identity is missing")?;
    let number: u32 = device
        .strip_prefix(r"\\.\PhysicalDrive")
        .ok_or("Invalid physical USB identity")?
        .parse()
        .map_err(|_| "Invalid USB number")?;
    // Only the integer disk number is interpolated. All commands and module paths
    // come from Windows; no filesystem path or user text is evaluated as code.
    let script = format!(
        r#"$ErrorActionPreference='Stop';
Import-Module (Join-Path $PSHOME 'Modules/Storage/Storage.psd1');
$d=Get-Disk -Number {number};
if($d.BusType -ne 'USB' -or $d.IsBoot -or $d.IsSystem -or $d.IsReadOnly -or $d.IsOffline){{throw 'The selected USB is protected or unavailable.'}}
if($d.PartitionStyle -ne 'GPT'){{throw 'Keeping files requires a GPT USB with one NTFS data partition and one FAT32 EFI partition.'}}
$parts=@(Get-Partition -DiskNumber {number});
if($parts.Count -eq 0 -or $parts.Count -gt 16){{throw 'The USB partition layout is not supported for keeping files.'}}
$volumes=@(foreach($p in $parts){{
$v=$p|Get-Volume;
if(!$v -or $v.HealthStatus -ne 'Healthy'){{throw 'A USB volume is unavailable or unhealthy.'}}
[pscustomobject]@{{root=$v.Path;partition=[uint32]$p.PartitionNumber;guid=[string]$p.Guid;offset=[uint64]$p.Offset;size=[uint64]$p.Size;fileSystem=[string]$v.FileSystem;freeBytes=[uint64]$v.SizeRemaining;partitionType=[string]$p.GptType}}
}});
@{{uniqueId=$d.UniqueId.Trim();serialNumber=$d.SerialNumber.Trim();path=$d.Path;size=[uint64]$d.Size;volumes=$volumes}}|ConvertTo-Json -Compress -Depth 5
"#
    );
    let output = powershell_query(&script, None)?;
    let value: Value =
        serde_json::from_slice(&output).map_err(|_| "USB volume information is unavailable")?;
    let hardware: Value = serde_json::from_str(
        identity["hardwareId"]
            .as_str()
            .ok_or("Missing USB hardware identity")?,
    )
    .map_err(|_| "Invalid USB hardware identity")?;
    for field in ["uniqueId", "serialNumber", "path"] {
        if value[field] != hardware[field] {
            return Err("The selected USB changed. Refresh disks.".into());
        }
    }
    if value["size"] != identity["size"] {
        return Err("USB capacity changed".into());
    }
    let volumes: Vec<Volume> = serde_json::from_value(value["volumes"].clone())
        .map_err(|_| "Invalid USB volume information")?;
    if let Some(volume) = volumes.iter().find(|v| {
        !v.partition_type
            .eq_ignore_ascii_case("{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}")
    }) {
        observe_free(volume.free_bytes);
    }
    if volumes.len() != 2 {
        return Err("Keeping files requires an existing NTFS data partition and a separate FAT32 EFI partition. This layout cannot be prepared without changing partitions.".into());
    }
    let data = volumes
        .iter()
        .find(|v| {
            v.file_system == "NTFS"
                && v.partition_type
                    .eq_ignore_ascii_case("{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}")
        })
        .ok_or(
            "This layout needs an existing NTFS data partition; FAT32 cannot hold the Omarchy ISO.",
        )?
        .clone();
    let efi = volumes.iter().find(|v| v.file_system == "FAT32" && v.partition_type.eq_ignore_ascii_case("{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}")).ok_or("This USB has no supported FAT32 EFI partition. Keeping files cannot create or resize partitions.")?.clone();
    for v in [&data, &efi] {
        let root = v.root.to_str().ok_or("Invalid volume root")?;
        let guid = root
            .strip_prefix(r"\\?\Volume{")
            .and_then(|s| s.strip_suffix("}\\"))
            .ok_or("An exact Windows volume GUID is required")?;
        uuid::Uuid::parse_str(guid).map_err(|_| "Invalid volume GUID")?;
        hold_directory(&v.root)?;
    }
    require_plain_storage(&no_link(&data.root)?)?;
    // BitLocker volumes cannot be mounted by the live ISO. A locked/encrypted
    // volume is never qualified merely because Windows can currently read it.
    if powershell_query(r#"$ErrorActionPreference='Stop'; Import-Module (Join-Path $PSHOME 'Modules/BitLocker/BitLocker.psd1'); $p=[Console]::In.ReadToEnd(); $v=Get-BitLockerVolume -MountPoint $p; if(!$v -or $v.VolumeStatus -ne 'FullyDecrypted'){throw 'Encrypted USB volumes cannot be used for keeping files.'}"#,Some(&data.root.to_string_lossy())).is_err() {
        return Err("Administrator access is needed to confirm that this USB is unencrypted. If BitLocker inspection is unavailable, keeping files remains disabled.".into());
    }
    let old_boot_hash = optional_hash(&efi.root, FALLBACK)?;
    let recovery_dir = data.root.join(NAMESPACE);
    let recovery = if recovery_dir
        .join("record.json")
        .try_exists()
        .map_err(|e| e.to_string())?
    {
        let mut guards = vec![];
        let path = hold_path(
            &data.root,
            &format!("{NAMESPACE}/record.json"),
            false,
            &mut guards,
        )?;
        let record: Record=serde_json::from_slice(&read_small(&path, 16384)?).map_err(|_| "The USB recovery record is incomplete or invalid; existing files have been left untouched")?;
        validate_record(&record, identity, &data, &efi)?;
        let backup = optional_hash(&data.root, &backup_relative(&data.root, &record)?)?;
        if backup != record.old_boot_hash {
            return Err("The previous bootloader backup is missing or has changed; recovery is unavailable.".into());
        }
        if old_boot_hash.is_some()
            && old_boot_hash != Some(record.loader_hash.clone())
            && old_boot_hash != record.old_boot_hash
        {
            return Err("The USB bootloader changed after Omarchy preparation. Automatic restoration would overwrite another change.".into());
        }
        Some(record)
    } else {
        if recovery_dir.try_exists().map_err(|e| e.to_string())? {
            hold_directory(&recovery_dir)?;
            if recovery_dir.join("previous-BOOTX64.EFI").exists() {
                return Err(
                    "An incomplete boot backup is present. Keeping files will not overwrite it."
                        .into(),
                );
            }
        }
        let config_dir = efi.root.join("EFI/Omarchy");
        if config_dir.try_exists().map_err(|e| e.to_string())? {
            hold_directory(&config_dir)?;
            if fs::read_dir(&config_dir)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
            {
                return Err("The EFI/Omarchy folder already contains files. Keeping files will not overwrite them.".into());
            }
        }
        None
    };
    Ok(Plan {
        data,
        efi,
        old_boot_hash,
        recovery,
    })
}
fn backup_relative(root: &Path, record: &Record) -> Result<String, String> {
    let active = format!("{NAMESPACE}/previous-BOOTX64.EFI");
    if root.join(&active).try_exists().map_err(|e| e.to_string())? {
        return Ok(active);
    }
    Ok(format!(
        "{NAMESPACE}/previous-BOOTX64-{}.EFI",
        record.operation
    ))
}
fn validate_record(
    record: &Record,
    identity: &Value,
    data: &Volume,
    efi: &Volume,
) -> Result<(), String> {
    if record.schema != 1
        || !uuid::Uuid::parse_str(&record.operation)
            .is_ok_and(|id| id.to_string() == record.operation)
        || record.disk_fingerprint != identity["fingerprint"].as_str().unwrap_or("")
        || record.data_guid != data.guid
        || record.efi_guid != efi.guid
        || record.iso_hash != ISO_HASH
    {
        return Err("The recovery record does not belong to this USB layout".into());
    }
    for hash in [&record.loader_hash, &record.config_hash, &record.iso_hash]
        .into_iter()
        .chain(record.old_boot_hash.iter())
    {
        if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid recovery hash".into());
        }
    }
    Ok(())
}

fn same_layout(left: &Plan, right: &Plan) -> bool {
    let mut left = left.clone();
    let mut right = right.clone();
    left.data.free_bytes = 0;
    right.data.free_bytes = 0;
    left.efi.free_bytes = 0;
    right.efi.free_bytes = 0;
    left == right
}
pub fn recheck(identity: &Value, expected: &Plan) -> Result<Plan, String> {
    let fresh = inspect_plan(identity)?;
    if !same_layout(&fresh, expected) {
        return Err(
            "USB layout or boot files changed after inspection. Select the drive again.".into(),
        );
    }
    Ok(fresh)
}
pub fn validate_addition(plan: &Plan, length: u64, sha256: &str) -> Result<(), String> {
    if sha256 != ISO_HASH {
        return Err("Keeping files is currently qualified only for Omarchy 4.0.2.".into());
    }
    if plan.recovery.is_some() {
        return Err(
            "An Omarchy preparation record already exists. Automatic cleanup and restoration are not currently available.".into(),
        );
    }
    if plan.data.free_bytes < length.saturating_add(RESERVE) {
        return Err("The USB no longer has enough free space to keep existing files.".into());
    }
    if plan.efi.free_bytes < 16 * 1024 * 1024 {
        return Err("The EFI partition needs at least 16 MiB free.".into());
    }
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}
struct AlignedBuffer {
    pointer: std::ptr::NonNull<u8>,
    layout: std::alloc::Layout,
}
fn file_identity(file: &File) -> Result<(u64, u64), String> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        let mut information = unsafe {
            std::mem::zeroed::<windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION>(
            )
        };
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle(
                file.as_raw_handle(),
                &mut information,
            )
        } == 0
        {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        Ok((
            u64::from(information.dwVolumeSerialNumber),
            (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
        ))
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata().map_err(io_error)?;
        Ok((metadata.dev(), metadata.ino()))
    }
}
fn publish_iso(temporary: &Path, final_path: &Path, writer: &File) -> Result<File, String> {
    #[cfg(windows)]
    {
        fs::rename(temporary, final_path).map_err(io_error)?;
    }
    #[cfg(unix)]
    {
        fs::hard_link(temporary, final_path).map_err(io_error)?;
        fs::remove_file(temporary).map_err(io_error)?;
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x00200000);
    }
    let guard = options.open(final_path).map_err(io_error)?;
    if file_identity(writer)? != file_identity(&guard)? {
        return Err(
            "The copied ISO changed before publication. The previous boot setup is still active."
                .into(),
        );
    }
    Ok(guard)
}
impl AlignedBuffer {
    fn new() -> Result<Self, String> {
        let layout =
            std::alloc::Layout::from_size_align(1024 * 1024, 4096).map_err(|e| e.to_string())?;
        let pointer = std::ptr::NonNull::new(unsafe { std::alloc::alloc_zeroed(layout) })
            .ok_or("Readback buffer allocation failed")?;
        Ok(Self { pointer, layout })
    }
    fn bytes(&mut self) -> &mut [u8] {
        // The allocation is initialized, unique and alive for this borrow.
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.layout.size()) }
    }
}
impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(self.pointer.as_ptr(), self.layout);
        }
    }
}
fn copy_verified(
    source: &Path,
    target: &Path,
    length: u64,
    expected: &str,
    cancel: &AtomicBool,
    emit: &mut impl FnMut(Value),
) -> Result<File, String> {
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(5);
    }
    let mut output = options.open(target).map_err(|e| e.to_string())?;
    require_plain_storage(&output.metadata().map_err(io_error)?)?;
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut total = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled before changing USB boot files. The previous boot setup is still active.".into());
        }
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > length {
            return Err("Source image grew".into());
        }
        output
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        if total % (64 * 1024 * 1024) < buffer.len() as u64 {
            emit(
                json!({"stage":"copying","message":"Copying the installer alongside existing files…","bytes":total,"totalBytes":length,"cancelAvailable":true}),
            );
        }
    }
    output.sync_all().map_err(|e| e.to_string())?;
    if total != length {
        return Err("Source image ended early".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    // The writer guard denies other writes; handle identities detect replacement. Read through a
    // separate uncached handle so this verifies the USB, not cached write data.
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x20000000 | 0x08000000);
    }
    let mut readback = options.open(target).map_err(|e| e.to_string())?;
    if file_identity(&output)? != file_identity(&readback)? {
        return Err("USB temporary file changed during readback".into());
    }
    let mut aligned = AlignedBuffer::new()?;
    let mut hash = Sha256::new();
    total = 0;
    while total < length {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled before changing USB boot files.".into());
        }
        let count = readback.read(aligned.bytes()).map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("USB ISO readback ended early".into());
        }
        hash.update(&aligned.bytes()[..count]);
        total += count as u64;
        if total > length {
            return Err("USB ISO length changed during readback".into());
        }
        if total % (64 * 1024 * 1024) < buffer.len() as u64 {
            emit(
                json!({"stage":"verifying","message":"Reading back the copied installer…","bytes":total,"totalBytes":length,"cancelAvailable":true}),
            );
        }
    }
    if format!("{:x}", hash.finalize()) != expected {
        return Err("USB ISO readback failed. The previous boot setup is still active.".into());
    }
    Ok(output)
}

pub fn install(
    identity: &Value,
    plan: &Plan,
    source: &Path,
    length: u64,
    sha256: &str,
    loader: &Path,
    cancel: &AtomicBool,
    mut emit: impl FnMut(Value),
) -> Result<Value, String> {
    let fresh = recheck(identity, plan)?;
    validate_addition(&fresh, length, sha256)?;
    let loader_bytes = read_small(loader, 32 * 1024 * 1024)?;
    install_files(
        identity,
        plan,
        source,
        length,
        sha256,
        &loader_bytes,
        cancel,
        &mut emit,
    )
}

fn install_files(
    identity: &Value,
    plan: &Plan,
    source: &Path,
    length: u64,
    sha256: &str,
    loader_bytes: &[u8],
    cancel: &AtomicBool,
    mut emit: impl FnMut(Value),
) -> Result<Value, String> {
    let operation = uuid::Uuid::new_v4().to_string();
    let iso_name = format!("omarchy-4.0.2-{operation}.iso");
    let config = include_str!("../../../../providers/usb-preserve/boot/grub.cfg.template")
        .replace("@ISO_NAME@", &iso_name);
    let mut guards = vec![];
    let iso = hold_path(&plan.data.root, &iso_name, false, &mut guards)?;
    let iso_temporary = hold_path(
        &plan.data.root,
        &format!("{iso_name}.partial"),
        false,
        &mut guards,
    )?;
    let record_path = hold_path(
        &plan.data.root,
        &format!("{NAMESPACE}/record.json"),
        true,
        &mut guards,
    )?;
    let backup = hold_path(
        &plan.data.root,
        &format!("{NAMESPACE}/previous-BOOTX64.EFI"),
        false,
        &mut guards,
    )?;
    let boot = hold_path(&plan.efi.root, FALLBACK, true, &mut guards)?;
    let boot_temp = hold_path(&plan.efi.root, "EFI/BOOT/OMARCHY.NEW", false, &mut guards)?;
    let config_path = hold_path(&plan.efi.root, CONFIG, true, &mut guards)?;
    if boot_temp.exists() || config_path.exists() {
        return Err(
            "An Omarchy staging path already exists; no existing file was overwritten".into(),
        );
    }
    if let Some(hash) = &plan.old_boot_hash {
        let bytes = read_small(&boot, 32 * 1024 * 1024)?;
        if digest(&bytes) != *hash {
            return Err("Previous USB bootloader changed".into());
        }
        write_new(&backup, &bytes)?;
        if digest(&read_small(&backup, 32 * 1024 * 1024)?) != *hash {
            return Err("USB boot backup verification failed".into());
        }
    }
    let record = Record {
        schema: 1,
        operation,
        disk_fingerprint: identity["fingerprint"]
            .as_str()
            .ok_or("Missing identity")?
            .into(),
        data_guid: plan.data.guid.clone(),
        efi_guid: plan.efi.guid.clone(),
        old_boot_hash: plan.old_boot_hash.clone(),
        loader_hash: digest(&loader_bytes),
        config_hash: digest(config.as_bytes()),
        iso_hash: sha256.into(),
    };
    write_new(
        &record_path,
        &serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?,
    )?;
    let iso_writer = copy_verified(source, &iso_temporary, length, sha256, cancel, &mut emit)?;
    let _iso_guard = publish_iso(&iso_temporary, &iso, &iso_writer)?;
    write_new(&config_path, config.as_bytes())?;
    if digest(&read_small(&config_path, 16384)?) != record.config_hash {
        return Err("USB boot configuration verification failed".into());
    }
    write_new(&boot_temp, &loader_bytes)?;
    if digest(&read_small(&boot_temp, 32 * 1024 * 1024)?) != record.loader_hash {
        return Err("USB bootloader verification failed".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled before activating the new USB boot setup. Preparation files and the boot backup have been retained; automatic cleanup is not currently available.".into());
    }
    emit(
        json!({"stage":"activating","message":"Activating the verified USB boot setup…","cancelAvailable":false}),
    );
    if optional_hash(&plan.efi.root, FALLBACK)? != plan.old_boot_hash {
        return Err("Previous USB bootloader changed before activation".into());
    }
    replace_file(&boot_temp, &boot)?;
    if optional_hash(&plan.efi.root, FALLBACK)? != Some(record.loader_hash) {
        return Err(
            "Boot activation could not be verified. Use the saved USB boot backup to restore."
                .into(),
        );
    }
    Ok(
        json!({"mode":"preserve","backupPath":record_path.parent(),"message":"Installer copied and read back. Existing files and partitions were retained. Use x64 UEFI with Secure Boot off. Safely eject the USB before unplugging it."}),
    )
}

fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                0x1 | 0x8,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, target).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copy_readback_checks_bytes_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        fs::write(&source, b"known installer").unwrap();
        let cancel = AtomicBool::new(false);
        copy_verified(
            &source,
            &target,
            15,
            &digest(b"known installer"),
            &cancel,
            &mut |_| {},
        )
        .unwrap();
        assert!(copy_verified(
            &source,
            &target,
            15,
            &digest(b"known installer"),
            &cancel,
            &mut |_| {}
        )
        .is_err());
        assert_eq!(fs::read(&target).unwrap(), b"known installer");
    }
    #[test]
    fn cancel_does_not_change_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        fs::write(&source, b"installer").unwrap();
        assert!(copy_verified(
            &source,
            &target,
            9,
            &digest(b"installer"),
            &AtomicBool::new(true),
            &mut |_| {}
        )
        .is_err());
        assert_eq!(fs::read(source).unwrap(), b"installer");
    }
    #[test]
    fn rejects_parent_traversal() {
        let dir = tempfile::tempdir().unwrap();
        assert!(hold_path(dir.path(), "../outside", true, &mut vec![]).is_err());
    }
    fn fixture(dir: &Path) -> (Value, Plan, PathBuf) {
        let data = dir.join("data");
        let efi = dir.join("efi");
        fs::create_dir_all(efi.join("EFI/BOOT")).unwrap();
        fs::create_dir(&data).unwrap();
        fs::write(efi.join(FALLBACK), b"original bootloader").unwrap();
        fs::write(data.join("family photos.txt"), b"precious user data").unwrap();
        let source = dir.join("source.iso");
        fs::write(&source, b"test ISO contents").unwrap();
        let volume = |root, partition| Volume {
            root,
            partition,
            guid: format!("partition-{partition}"),
            offset: partition as u64 * 1024 * 1024,
            size: 10 * 1024 * 1024 * 1024,
            file_system: "NTFS".into(),
            free_bytes: 9 * 1024 * 1024 * 1024,
            partition_type: "fixture".into(),
        };
        (
            json!({"fingerprint":"fixture-disk"}),
            Plan {
                data: volume(data, 2),
                efi: volume(efi, 1),
                old_boot_hash: Some(digest(b"original bootloader")),
                recovery: None,
            },
            source,
        )
    }
    #[test]
    fn install_preserves_user_files_and_verifies_original_boot_backup() {
        let dir = tempfile::tempdir().unwrap();
        let (identity, plan, source) = fixture(dir.path());
        install_files(
            &identity,
            &plan,
            &source,
            17,
            &digest(b"test ISO contents"),
            b"new loader",
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(
            fs::read(plan.efi.root.join(FALLBACK)).unwrap(),
            b"new loader"
        );
        assert_eq!(
            fs::read(plan.data.root.join("family photos.txt")).unwrap(),
            b"precious user data"
        );
        assert_eq!(
            fs::read(
                plan.data
                    .root
                    .join(format!("{NAMESPACE}/previous-BOOTX64.EFI"))
            )
            .unwrap(),
            b"original bootloader"
        );
        let record: Record = serde_json::from_slice(
            &fs::read(plan.data.root.join(format!("{NAMESPACE}/record.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(record.old_boot_hash, Some(digest(b"original bootloader")));
        assert_eq!(
            fs::read(
                plan.data
                    .root
                    .join(format!("omarchy-4.0.2-{}.iso", record.operation))
            )
            .unwrap(),
            b"test ISO contents"
        );
        assert_eq!(
            digest(&fs::read(plan.efi.root.join(CONFIG)).unwrap()),
            record.config_hash
        );
    }
    #[test]
    fn failed_copy_keeps_original_boot_user_files_and_backup() {
        let dir = tempfile::tempdir().unwrap();
        let (identity, plan, source) = fixture(dir.path());
        assert!(install_files(
            &identity,
            &plan,
            &source,
            17,
            &digest(b"wrong ISO"),
            b"new loader",
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err());
        assert_eq!(
            fs::read(plan.efi.root.join(FALLBACK)).unwrap(),
            b"original bootloader"
        );
        assert_eq!(
            fs::read(plan.data.root.join("family photos.txt")).unwrap(),
            b"precious user data"
        );
        assert_eq!(
            fs::read(
                plan.data
                    .root
                    .join(format!("{NAMESPACE}/previous-BOOTX64.EFI"))
            )
            .unwrap(),
            b"original bootloader"
        );
        assert!(!plan.efi.root.join(CONFIG).exists());
    }
    #[cfg(windows)]
    #[test]
    fn holds_checked_directories_until_activation_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let (identity, plan, source) = fixture(dir.path());
        let mut checked = false;
        install_files(
            &identity,
            &plan,
            &source,
            17,
            &digest(b"test ISO contents"),
            b"new loader",
            &AtomicBool::new(false),
            |event| {
                if event["stage"] == "activating" {
                    checked = true;
                    assert!(
                        fs::rename(plan.efi.root.join("EFI"), plan.efi.root.join("changed"))
                            .is_err()
                    );
                }
            },
        )
        .unwrap();
        assert!(checked);
    }
    #[cfg(windows)]
    #[test]
    fn loads_only_the_windows_storage_module_for_inspection() {
        let output=powershell_query(r#"$ErrorActionPreference='Stop';Import-Module (Join-Path $PSHOME 'Modules/Storage/Storage.psd1');@{name=(Get-Command Get-Disk).Name}|ConvertTo-Json -Compress"#,None).unwrap();
        let result: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(result["name"], "Get-Disk");
    }
}
