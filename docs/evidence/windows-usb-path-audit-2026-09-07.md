# USB path audit, 2026-09-07

Reviewer and implementation: **gpt-6-astra**. User requested a proactive review
after several Windows USB-path failures.

## Findings addressed

1. **Blank USBs failed before writing.** `Get-Partition -DiskNumber` raises
   `CmdletizationQuery_NotFound_DiskNumber` when the query finds no partitions.
   The lock helper treated that as an operation failure even for a valid empty
   disk. It now queries `MSFT_Partition` with a disk-number filter, which returns
   an empty set without hiding actual query errors. The count must also match
   `Get-Disk.NumberOfPartitions`, so incomplete enumeration cannot skip locks.
   The old helper fails the new empty-disk regression; the fixed helper passes.

2. **The desktop completion gate did not validate the USB receipt.** It checked
   child exit and protocol completion but accepted the returned body without
   requiring the approved target, image digest, write/readback spans, padding,
   flush, and verification results. The USB branch now checks these before
   recording completion. A verified image whose ejection failed remains a
   successful write with the existing manual-ejection message.

## Coverage added

`providers/media-etcher/test/physical-engine.test.cjs` runs the production
`runPhysicalWrite` orchestration and the actual Etcher `BlockDevice` stream and
verifier. Native disk discovery, geometry queries, locks, and ejection are
intercepted. Reads and writes use an exclusively opened, unbuffered temporary
regular file. Unexpected raw paths or child commands fail immediately.

The scenarios cover successful multi-chunk writes, Windows' deferred first
buffer, 512/4096-byte sector alignment, a sub-sector image, bounded zero padding,
untouched trailing capacity, the unsupported-alignment fallback, changed
geometry/identity before writes, disconnection, corruption, flush failures,
short final readback, cleanup order, and ejection failure.

`windows-volume-lock.test.cjs` executes the real PowerShell helper with managed
mock volume handles and mock Storage queries. It covers blank disks, real query
failure, incomplete partition enumeration, a protected target, and failure on a
later volume. Every acquired mock handle must be disposed. Its native P/Invoke
body is never loaded.

`physical-cli.test.cjs` exercises the real CLI using a slow file-only engine.
Both explicit cancellation and parent-pipe closure must produce cancellation,
terminate the process, and release the open file. No success record is accepted.

The desktop receipt tests reject missing verification fields, false checks,
wrong target/digest, wrong byte counts, and unknown ejection status. They accept
the current complete receipt and the explicit manual-ejection outcome.

Validation on this Windows host: **44/44 provider tests** and **25 desktop native
tests** passed. The desktop suite retains three opt-in environment-dependent
tests outside its default run; the source-handoff fixture also **passed** when
run explicitly with the built media adapter, for **26 native tests passed** in
total. Physical hardware and boot tests are not implied
by these counts.

## Inspection and limits

Reviewed discovery and exclusions, source retention, volume locking, the raw
handle and geometry guards, SDK streaming and verification, flush/readback,
cancellation, ejection, the privileged completion path, and the existing
keep-files implementation. Existing physical-device reidentification and
source-image authentication boundaries remain enforced.

Read-only Windows queries confirmed that the filtered CIM query returns an
empty result for no matching partitions and returns the attached USB's current
partition inventory. No physical USB write, dismount, lock, ejection, elevation,
or boot test was performed during this audit. The tests establish software
behavior; device-driver and boot qualification still require hardware testing.

Windows API references:
[Get-Partition](https://learn.microsoft.com/en-us/powershell/module/storage/get-partition)
and [FSCTL_LOCK_VOLUME](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_lock_volume).
