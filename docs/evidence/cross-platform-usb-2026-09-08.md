# Linux and macOS USB implementation

Date: 2026-09-08. Implementation model: `gpt-6-astra`.
Engine: pinned `etcher-sdk` 10.2.14; existing Windows native geometry extension
retained. This update implements the missing host handling in the shared USB
erase-and-write path. No physical disks were written during this work.

## User-visible scope

Linux and macOS can select an eligible USB, review its exact identity and the
erase warning, approve administrator access, and run the shared verified writer.
The keep-existing-files method remains Windows x64 only and is disabled with an
explanation elsewhere. Apple Silicon hosts create x86 installation media for
another compatible computer. Direct installation remains a separate method.

Unix receipts default to Documents, outside installed application resources.
Linux provider lookup includes Tauri installed and AppImage resource layouts.
macOS uses the app bundle Resources directory. Existing runtime hashes, protected
source staging and native elevation boundaries remain mandatory.

## Host corrections

- Linux traces source, system and swap storage through partition parents,
  device-mapper/LUKS/LVM, RAID members and loop files. Btrfs resolution includes
  every member, even when subvolume device numbers are synthetic. Unknown backing
  storage is refused. Active holders on any target partition also prevent writing.
- Linux unmounts exact device-number matches, including aliases and bind mounts,
  deepest first. There is no lazy/forced fallback. Remaining mounts and remounts
  after exclusive acquisition stop the operation before writes. Completion reports
  unmounting, followed by the operating system safely-remove action.
- macOS parses IORegistry XML with binary data properties using the already locked
  `@balena/apple-plist` 0.0.3 dependency, now declared directly. Real physical sector
  properties supplement drivelist's logical-only geometry; descriptor geometry
  still has to match. APFS containers/snapshots map to all physical stores for
  source/system exclusions. A USB backing an APFS container is refused as in use.
- Display labels no longer participate in hardware fingerprints: unmounting and
  replacing partitions change Linux labels. Hardware serial/attachment, paths,
  geometry and capacity remain bound. POSIX raw opens reject partition paths,
  aliases and symlinks.
- Unix child commands use a fixed system PATH and omit dynamic-loader overrides.
  The packaged inspector includes its complete plist/SAX dependency closure.

## Executed checks

| Check | Result |
| --- | --- |
| Windows media build and test suite, Node 22.13.1 x64 | 57 passed, 3 POSIX-only skips, no failures |
| Native Linux media build and test suite, Ubuntu under WSL, Node 24.11.1 x64 | 49 passed, 11 Windows-only skips, no failures |
| Linux native dependency loading | All eight probed modules loaded |
| Linux staged writer and isolated inspector | Loaded successfully; 3,278 runtime files staged |
| Windows staged writer and isolated inspector | Loaded successfully; 3,388 runtime files staged |
| Desktop USB UI tests | 13 passed, including Linux, Intel Mac and Apple Silicon erase review |
| Desktop Svelte/TypeScript check | No errors; four existing warnings |
| Native Rust runtime-location and inspection-authentication tests on Windows | 2 passed |

The writer tests execute the actual SDK pipeline against ordinary temporary
files, with device discovery, unmount/eject and raw-device handles substituted.
They exercise 512/4096-byte alignment, bounded zero padding, full SHA-256 readback,
write/geometry/identity/flush/readback failures, cancellation and cleanup. Linux
and macOS inventory fixtures cover layered storage, mount membership, binary plist
properties and APFS mappings. Fixtures are synthetic, not captured hardware reports.

CI now runs native desktop tests, native media tests, dependency loading and
staged-runtime checks on Windows, Linux and macOS. Configuration of a job is not
evidence that it has passed; consult the associated CI run.

## Remaining qualification and prerequisites

No macOS host was available for local native execution. No Linux/macOS physical
USB preparation, firmware boot or completed installation was tested. The README
hardware table deliberately retains Untested for these hosts. No full desktop
release executable was rebuilt in this task.

Each host requires dependencies staged natively for its architecture with Node 22
or 24. Linux requires sysfs/procfs, util-linux (`lsblk`, `findmnt`, `umount`) and a
working polkit administrator prompt (`pkexec`). macOS requires its native diskutil,
IORegistry and Disk Arbitration services and the existing administrator prompt.
Only unambiguously identified whole USB disks with hardware serials and consistent
geometry are admitted. Unsupported virtual/network-backed source layouts and
macOS APFS target containers fail closed. These software checks do not qualify a
signed distributable or establish hardware compatibility.
