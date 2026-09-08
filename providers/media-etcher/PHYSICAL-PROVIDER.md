# Bounded physical USB provider

Implementation provenance: **gpt-6-astra**, 2026-09-06. Engine:
**etcher-sdk 10.2.14**, installed from the pinned npm lockfile. Windows builds
apply a source-hash-guarded compatibility extension to **@ronomon/direct-io
3.0.1** and compile it locally; its upstream MIT license remains in the runtime.
`scripts/prepare-direct-io.cjs` contains the pinned upstream SHA-256 and
`scripts/native/windows-geometry.h` contains the replacement Windows query.
`npm run build` and desktop staging both prepare this extension. Staging checks
its native version marker and authenticates the modified source and binary.
The original file-only API (`dist/adapter.js`) remains unchanged. The new API
is `dist/physical-adapter.js`; the native application invokes
`dist/physical-cli.js` using its packaged Node executable.

The provider has TypeScript, file-backed SDK and Windows device-path regression
checks. A read-only Windows probe reproduced the September 7 raw-device open
failure: the correct device name opened, whereas an appended backslash produced
Win32 error 31. Full physical writing and host qualification remain outstanding.
See [the diagnostic evidence](../../docs/evidence/windows-usb-open-2026-09-07.md).
The subsequent [USB path audit](../../docs/evidence/windows-usb-path-audit-2026-09-07.md)
adds execution of the actual USB pipeline against temporary files, native-volume
helper scenarios, and CLI cancellation checks. This covers software behavior
without claiming physical USB or boot qualification.

The [September 8 cross-platform update](../../docs/evidence/cross-platform-usb-2026-09-08.md)
adds native Linux file-backed writer execution, Linux storage ancestry and exact
unmount handling, macOS plist/APFS discovery fixtures, and three-host CI.
Linux/macOS physical USB writes and boots remain unqualified.

## Protocol 1

stdin accepts one bounded, newline-terminated JSON object:

```json
{"protocol":1,"action":"probe"}
{"protocol":1,"action":"list","sourcePath":"<optional absolute local source file>"}
{"protocol":1,"action":"write","sourcePath":"<authenticated raw file>","length":1048576,"sha256":"<64 hex characters>","target":{"fingerprint":"<64 hex characters>","device":"<from list>","raw":"<from list>","devicePath":null,"hardwareId":"<from list>","size":16000000000,"blockSize":512,"logicalBlockSize":512,"description":"<from list>","busType":"USB"}}
```

The examples above are separate operations, not a command batch. The caller must
preserve the exact identity returned by `list`; it must not synthesize a drive
identity from a path. The fingerprint hashes the canonical identity fields,
including the hardware identity, capacity, sector geometry and OS device paths.
It is an identity comparison token, not an authorization signature.

stdout contains only NDJSON `event`, `result` or `error` records, each with
`protocol: 1`. TypeScript shapes are defined in `src/physical-contracts.ts`.
`probe` reports `available`, prerequisite `reason`, and capability flags.
`list` returns `{drives:[{identity,mountpoints,eligible,reasons}]}`. All discovered
drives are listed for honest explanations; only eligible USB whole devices can
be selected. A source path enables source-disk exclusions in the list as well.
Writes always enforce source exclusions, whether listing supplied a source or not.
Unknown protocol fields and alternate actions are rejected.

Keep write stdin open until a terminal record and **process exit**. Closing stdin,
parent exit, SIGTERM, SIGINT, or the exact subsequent line
`{"protocol":1,"action":"abort"}` terminates the worker. The SDK has no AbortSignal
multi-write API; process termination is the cancellation boundary. The native
caller must wait for exit before acknowledging cancellation, and must also manage
its entire privileged process tree on forced cancellation. An interrupted write
can leave an incomplete, unbootable image. Exit 0 plus a matching result record
is necessary for success. Exit 2 denotes cancellation; other failures use exit 1.
stderr is reserved for diagnostics. The TypeScript physical adapter enforces the
same child-exit requirement for callers that use its API.

## Enforced boundaries

- Raw local regular source only: no URL, decompression, image configuration,
  downloads, arbitrary command, disk-partitioning, firmware or boot-entry API.
- The privileged caller authenticates the ISO and runtime independently. On
  Windows it retains the original ISO with read-only sharing and holds every
  ancestor against renaming until the provider process finishes. Other hosts
  retain the administrator-owned immutable source directory. This provider
  rejects linked source components, opens once, requires exact length, and uses
  that same descriptor in the SDK. The Windows helper authenticates the signature
  and SHA-256 in one pass, then supplies a `sourceVerification` binding containing
  its PID, the volume serial and the exact 64-bit file index. The writer matches
  that binding against its opened descriptor using bigint metadata, confirms the
  retaining parent is alive, and checks source identity and metadata again before
  writing and before success. The helper retains its deny-write/delete guard
  until the child exits, including cancellation and failure. This removes two
  redundant ISO scans without changing USB verification.
  Callers without this Windows binding still hash the source before and after
  writing. A supplied but invalid binding is rejected. The binding is an internal
  handoff for a live operation, never a persisted verification cache or a field
  accepted from the webview. Neither it nor the SHA-256 argument independently
  establishes upstream authenticity; the privileged caller must authenticate first.
- Fresh drivelist discovery is cross-checked with OS hardware identity. Windows
  uses fixed read-only Get-Disk inventory (serial, UniqueId, device interface,
  boot/system state and geometry); Linux uses sysfs device ancestry and hardware
  serial; macOS uses the IOUSBHostDevice registry ancestry and USB serial. Missing
  serials or ambiguous mappings make a drive ineligible. No hardware-serial scheme
  provides cryptographic device identity; pathological cloned identifiers remain
  a limitation of these OS interfaces.
- Only USB whole-device paths are accepted. System, source, virtual, read-only,
  uncertain, and unsupported-sector devices are excluded. Mounted filesystem
  device IDs protect the root/boot/runtime/source filesystems, including aliases.
  An unresolvable source filesystem aborts the operation. Linux storage holders
  are rejected. Source images on unenumerated network/LVM/other ambiguous storage
  may therefore require relocation to a mapped local filesystem.
- Reidentification occurs before source verification, before unmounting, after unmounting,
  and after acquiring the exclusive raw descriptor. Geometry is queried from the
  opened handle. Failure to re-enumerate a locked device aborts without relaxing
  identity policy. Only mounted-path stat failures caused by held Windows volume
  locks are exempted at the latter checkpoints.
  The Windows lock helper uses a filtered CIM partition query, which accepts a
  genuinely empty partition set for a blank disk. Query failures still abort,
  and the result count must match `Get-Disk.NumberOfPartitions` before locking.
- Some USB drivers reject `StorageAccessAlignmentProperty` with Win32 error 1
  or 50. Only these unsupported-query responses allow a Windows fallback.
  `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX` must still return the capacity and logical
  sector size from the retained descriptor. The physical sector size then comes
  from fresh, reidentified `Get-Disk` data, whose capacity and logical sectors
  must agree with that handle. It is never assumed to be 512. All resolved
  geometry must match the selected identity before writes are enabled. Other
  query errors, incomplete descriptors, conflicting geometry, and nonzero
  sector-alignment offsets abort the write. Native query errors include the
  Win32 error number. See [the geometry evidence](../../docs/evidence/windows-usb-geometry-2026-09-07.md).
- Physical sectors must be powers of two from 512 through 4096 bytes; logical
  sectors must divide physical sectors. The physical write span rounds the image
  length up to the next physical sector and must fit target capacity. Only the
  final image chunk may add padding: at most one physical sector minus one byte,
  always zero. Original image bytes and their authenticated SHA-256 never change.
  Every SDK write is checked against the original image boundary before this
  bounded tail extension; no other tail, decompression, sparse write or
  capacity-wide erase is authorized.
- The existing SDK BlockDevice supplies aligned writing and mandatory SDK verify.
  Its default open is bypassed because Windows diskpart clean happens before its
  exclusive handle acquisition. No diskpart clean or shell-generated partition
  script is used. The provider retains its descriptor through SDK verify, fsync,
  direct/uncached SHA-256 readback of every image byte, explicit zero-padding
  readback, and source revalidation. Final close/flush errors prevent a success
  receipt. Destination capacity beyond the rounded write span is not erased or
  verified.
- Windows opens the exact selected `PhysicalDriveN` global DOS-device alias via
  `\\?\GLOBALROOT\GLOBAL??\PhysicalDriveN`. Node 22.13.1 otherwise resolves the
  bare `\\.\PhysicalDriveN` name as a UNC root and adds a slash, causing EIO.
  The conversion only accepts an exact physical disk name; it does not accept
  caller-supplied namespace paths or change discovery/confirmation identity.
  Exclusive access, volume locks, handle geometry and reidentification still
  apply to the operation.

For a 6,227,752,960-byte ISO and 4096-byte physical sectors, the write span is
6,227,755,008 bytes and padding is 2048 zero bytes. Receipts keep `bytesWritten`,
`readbackBytes` and `sha256` tied to the original image. `writeSpanBytes`,
`paddingBytes`, `readbackSpanBytes` and `paddingVerification: true` separately
describe the physical boundary and verified zero tail. Zero padding is also
checked when present on 512-byte physical-sector devices.

Pinned SDK inspection: `BlockReadStream` and `BlockTransformStream` slice the
last emitted buffer to the original logical length, without adding zero bytes.
`BlockDevice.alignedWrite` would read-modify-write the physical sector and retain
old tail contents; the provider replaces only this final tail behavior with an
explicitly zeroed aligned buffer. `BlockWriteStream` increments progress by the
original buffer length, and `multi-write` reports that source-stream byte position
as `result.bytesWritten`. Its mandatory verifier covers the original image
length. The provider therefore compares the SDK count to the original source
length and separately reads and verifies the entire rounded span.

## Host behavior and limits

Windows retains FSCTL_LOCK_VOLUME and FSCTL_DISMOUNT_VOLUME handles for discovered
volume GUID paths in the fixed packaged `scripts/windows-volume-lock.ps1` helper.
It independently compares disk identity before locking; the helper accepts only
a disk number and expected identity and reads no commands from stdin. The raw
handle uses exclusive sharing, unbuffered I/O and write-through flags. Lock loss
terminates the write process. The lock helper releases retained handles on stdin
EOF. PowerShell/Get-Disk, native dependency support and elevation are prerequisites.
Unusual storage layouts or platforms where exclusive opens conflict with retained
volume handles/re-enumeration fail explicitly and require later qualification.

Linux resolves system, source and swap storage through sysfs partitions, LUKS/LVM,
RAID members and loop backing files; Btrfs subvolumes protect every physical member.
Unknown backing storage fails closed. Active holders on a disk or any partition
prevent selection. Mounts are matched by exact major/minor device numbers, including
aliases and bind mounts. Normal `/usr/bin/umount -- <mountpoint>` runs deepest first
with no lazy/forced fallback. Remaining mounts fail before writes, including a check
after O_EXCL/O_DIRECT/O_SYNC acquisition. The receipt reports `unmounted`, never
power-off or ejection; use the operating system safely-remove action.

macOS uses Disk Arbitration unmount, an O_EXLOCK descriptor with F_NOCACHE and
synchronous writes. USB identity enrichment requires IOUSBHostDevice registry
entries and an ioreg plist parsed with CFData support. IORegistry physical sector
properties correct drivelist's logical-only geometry; the retained descriptor must
still agree. APFS volumes/snapshots map to every physical store for system/source
exclusion. USB disks backing APFS containers are refused as in use. Missing serials,
unresolved virtual storage, missing registry support, or geometry disagreement fail
explicitly. Both POSIX raw paths reject symlinks, partitions and alternate aliases.

After successful readback/close, Windows and macOS request mountutils ejection
only after fresh identity matching. `ejected` additionally requires the target to
disappear from discovery. Failure yields a verified-write receipt with an honest
`failed` ejection outcome; the user must use the operating system safely-remove
action. Post-close device identity can race physical removal/replacement; OS
enumeration plus comparison reduces that race but is not a kernel identity token.
Flush/readback confirms the OS/device-reported result, not resilience to power
loss, a faulty controller, or later media corruption.

## Build and runtime staging

```powershell
npm run build
node scripts/stage-physical-runtime.cjs --output C:\absolute\new-runtime
```

Staging itself does not run a probe or test. `npm run stage:runtime -- --output
<absolute-new-directory>` first compiles TypeScript. The output contains Node,
`dist`, the installed dependency tree, fixed helper scripts, the npm lockfile,
this document and `runtime-manifest.json`. The manifest records every staged file
except itself with relative path, length and SHA-256, alongside Node version/ABI,
host platform/architecture, SDK version, package lock provenance and licenses,
and exact implementation model `gpt-6-astra`. The application must authenticate
this manifest against its own trusted compiled value and verify every file before
privileged execution; the manifest is not self-authenticating. Stage natively for
each supported target: copying a Windows native dependency tree to another host
does not establish ABI compatibility. No binaries are committed.

Etcher SDK and mountutils are Apache-2.0; drivelist is Apache-2.0;
@ronomon/direct-io is MIT. The full installed package tree retains upstream
license files and metadata, with resolved package licenses/integrities recorded
in the runtime manifest. Node's distribution license is required during staging,
using a license beside the executable (or the system Node package license on
Linux). `OMARCHY_NODE_LICENSE` can name the license for the exact Node distribution
when it lives elsewhere. The original file-only qualification record in
README.md is historical evidence only and does not qualify this physical provider.

The inspector includes the pinned `@balena/apple-plist` 0.0.3 and SAX dependency
closure. `node scripts/check-staged-physical-runtime.cjs` stages and loads the native
writer and an isolated inspector without enumerating or accessing disks. Linux
desktop resources resolve from portable, AppImage and installed Tauri layouts;
macOS uses app bundle Resources. Unix operation records live outside app resources.
