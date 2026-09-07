# Windows x64 alongside installation provider

`Invoke-DirectX86.ps1` implements a Windows PowerShell 5.1-compatible NDJSON
provider. `NativeDisk.cs` supplies bounded storage/firmware primitives;
`EncryptedImage.cs` authenticates encrypted artifacts and streams plaintext
directly to new partitions. The authenticated elevated desktop helper owns
source verification, protected work products, the memory-only key and final
confirmation of the exact plan.

On Windows the helper reads the original ISO in place and retains file and
ancestor handles against writes, deletion and renaming until its consumers stop.
The small detached signature stays in the protected operation directory.
Construction mounts those two files separately and read-only under their
canonical names in `/input`; it does not expose the ISO's parent directory.

This revision implements selectable allocation, confirmed native NTFS shrink,
explicit partition replacement with typed confirmation,
conditional BitLocker suspension with verified restoration, and LUKS2 root
images. The [completion pass](../../docs/evidence/windows-direct-readiness-2026-09-06.md)
adds protector/PCR and measured-boot eligibility, firmware preparation, runtime
import and bounded cleanup. **Validation includes pure tests, static parsing,
compilation and local runtime-archive import.
No encrypted build, NTFS resize, BitLocker lifecycle, privileged host run,
physical deployment, independent boot or owner setup was performed.** An earlier
non-elevated inventory probe does not qualify these additions. The separate
credential-bearing image-proof runner's output is rejected by this provider.

## Protocol

```powershell
powershell.exe -NoProfile -NonInteractive -File Invoke-DirectX86.ps1 `
  -Action probe -RequestPath C:\absolute\request.json
```

Actions are `probe`, `firmware-info`, `prepare-firmware`, `prepare-runtime`,
`build`, `plan`, or `deploy`. Request JSON is
bounded to 1 MiB; unknown fields are rejected. Stdout uses NDJSON protocol 1.
Each action ends with `type: "result"` and exit 0, or `type: "error"` with
`code`, `message`, `mutationStarted`, `recovery` and exit 1. Payloads are in
`result`; artifact paths/hashes also appear at event top level. Build diagnostic
stderr stays beside the protected output directory.

| Action | Request fields |
| --- | --- |
| `probe` | `{}` or optional `operationId` UUID and `protectedPaths` array identifying original source/installer files |
| `firmware-info` | `{}`; administrator inspection of the separate firmware-preparation state |
| `prepare-firmware` | `operationId`, `expectedSha256`; fresh state must match the native reviewed firmware-info digest |
| `prepare-runtime` | `{}`; import only the packaged archive matching the pinned runtime |
| `build` | `operationId`, `sourceIsoPath`, `sourceSignaturePath`, `outputDirectory` |
| `plan` | `operationId`, `diskNumber`, `diskUniqueId`, `targetKind`, `allocationBytes`, `manifestPath`, `manifestSha256`, `encryption: "luks2"`, `protectedPaths`; `startOffsetBytes` for `free`, `shrinkPartitionNumber` and `shrinkPartitionGuid` for `shrink`, or the deletion fields below for `delete` |
| `deploy` | `planPath`, `planSha256`, `manifestPath`, `manifestSha256`; optional `operationId` |

For build/plan/deploy the same helper sends a bounded private stdin line with
exactly `protocolVersion: 2` and `stagingKey` (base64 of 32 random bytes).
It is never an on-disk request, command-line argument, environment variable,
manifest or log entry. Build forwards it to private container stdin. Losing
the helper key requires rebuilding; retained ciphertext cannot independently
resume. Manifests/plans/receipts use schema 2.

Sources must be `omarchy-4.0.2.iso` and its adjacent `.sig`. The client checks
the signature before staging; the builder independently checks full digest,
length, signature and pinned upstream key. Other releases fail.

Probe reports firmware, Secure Boot, prerequisites, disk identities/geometry,
free extents, NTFS shrink and deletion candidates, fresh BitLocker state and blockers.
Candidates include Windows limits, free-space reserve and maximum allocation.
Unavailable privileged checks are repeated elevated by the helper. Windows
supported-size analysis can start Optimize Drives, so this new probe is excluded
from static qualification. [Microsoft sizing API](https://learn.microsoft.com/en-us/powershell/module/storage/get-partitionsupportedsize?view=windowsserver2025-ps)

Build returns the encrypted manifest and path/hash. Plan writes `plan.json`
with exact existing/new extents, NTFS before/after sizes, affected BitLocker
state and boot policy. Deployment succeeds only after written-image readback,
firmware readback and verification of original Windows protection state.

## Allocation and prerequisites

- Native Windows x64, UEFI/GPT, Secure Boot confirmed disabled, basic local
  NVMe/SATA/ATA/SCSI with 512-byte logical sectors. 4096-byte physical sectors
  with 512-byte logical sectors are supported by alignment. RAID, Storage
  Spaces, USB, virtual/dynamic disks, offline/read-only disks and unknown
  encryption states are rejected.
- The Windows disk is selectable. Allocation uses a contiguous free extent or
  space released from one eligible NTFS partition. Only writable, healthy,
  operational basic NTFS data/OS partitions with MiB-aligned starts qualify.
  The after-size meets fresh Windows limits and retains the greater of 20 GiB
  or 10% of volume size as free space. Windows performs `Resize-Partition`.
  [Microsoft resize API](https://learn.microsoft.com/en-us/powershell/module/storage/resize-partition?view=windowsserver2025-ps)
- Starts/sizes are MiB-aligned. ESP is 2,147,483,648 bytes; root image is
  40,800,092,160 bytes. Minimum total is **42,947,575,808 bytes** (40 GiB minus
  2 MiB). Larger selected totals assign the remainder after ESP to root.
  First boot grows the LUKS mapping and Btrfs.
- An NTFS partition with an unaligned original end also releases the trailing
  fraction of a MiB. Recorded `shrinkBytes` equals allocation plus that exact
  remainder. The plan includes both before/after sizes and allocation.
- Construction needs an already running Linux Docker Desktop engine and exact
  local image ID from `image-builder-x86/runtime-lock.json`. The separate
  runtime preparation action imports the verified release-packaged archive.
  Construction never pulls
  images, installs Docker/WSL/QEMU, changes Windows virtualization, elevates
  itself or reboots. It requires 10 GiB currently free RAM and 85 GiB free
  construction storage: implementation bounds, not measured minimums.

## Explicit partition replacement

The Windows frontend exposes replacement separately from normal free-space and
shrink choices. Its modal shows disk, partition, name, filesystem and the entire
partition size. Users must type `Disk N Partition M` (case-insensitive in the UI)
before preparation; the native command validates and normalizes that identifier.
The final helper-owned installation dialog repeats the irreversible deletion.
Selecting a smaller Omarchy allocation still deletes the entire old partition.

A `delete` plan requires `deletePartitionNumber`, `deletePartitionGuid`,
`deleteOffsetBytes`, `deleteSizeBytes` and `deleteConfirmation`. These are bound
to the inspected disk identity, original partition extent and plan digest. The
provider independently requires the canonical identifier. `protectedPaths`
preserves the original ISO, running helper and staging/runtime storage.

Eligible types are basic Windows data and ordinary Linux filesystem GPT
partitions. Current OS/system, EFI, recovery, reserved, required/bootable GPT
attributes, hidden/offline/read-only/shadow partitions, paging/dump storage and
installer volumes are excluded. Recovery configuration and path/volume mapping
must be known. Encrypted/unknown containers, LVM/RAID and multi-device Btrfs are
not supported. Ordinary single-device ext4/Btrfs can be replaced from Windows
without filesystem resizing; unrecognized Linux layouts remain unavailable.

Deployment repeats role, extent, filesystem and label checks, obtains a volume
lock (or exclusive access to an unmounted Linux partition), journals deletion
intent, and calls Windows `Remove-Partition` without an override. Readback must
show that exactly the selected partition disappeared and every other GPT entry
is unchanged. The freed allocation is checked again before creating Omarchy's
partitions. Windows locking/storage-provider interaction and foreign-filesystem
inspection are implemented but **not execution-tested**.

After deletion, BitLocker verification expects only the explicitly removed,
previously unencrypted data volume to disappear. All retained volumes must keep
their original protection state. There is no automatic restoration of a deleted
partition; a failure can leave deletion complete and installation incomplete.

## Windows encryption lifecycle

TPM-backed Windows OS protection requires the supported `0,2,4,11` PCR profile
with Secure Boot already off, the retained Windows firmware entry as
`BootCurrent`, and current measured-boot evidence for the matching Windows ESP.
Unknown/custom profiles and other measured EFI application routes are blocked.
These conservative checks do not prove recovery-free behavior on hardware.

When Secure Boot is on, a separate user-reviewed firmware-preparation action
can suspend originally active Windows protection and restart into firmware
settings. Leave TPM enabled, disable Secure Boot, return to Windows and inspect
again. Its recovery worker waits for a new Windows boot or a five-minute
cancelled-restart deadline. This transition is implemented but not machine-tested.

BitLocker/device-encrypted volumes qualify only when fully encrypted, unlocked
and in a known protection state. Incomplete/paused conversion, unknown state,
missing persistent protectors and ambiguous identity block planning. Inspection
covers the target disk plus OS/boot volumes on other disks because firmware
changes affect the Windows boot chain.

Only originally active OS protection is suspended with `DisableKeyProtectors`
and `DisableCount=1`. Existing suspension is preserved; data-volume protection
is not suspended. Unsupported encrypted non-OS boot layouts are blocked.
Suspension retains ciphertext and temporarily exposes a clear key; it does
not decrypt the volume. No Windows decrypt or protector add/delete method is
called. [Microsoft suspension API](https://learn.microsoft.com/en-us/windows/win32/secprov/disablekeyprotectors-win32-encryptablevolume)

Before suspension, durable admin/SYSTEM-protected state, a recovery script and
a fixed SYSTEM task are created under
`%ProgramData%\OmarchyDirectRecovery\<operation UUID>`. Startup and one-minute
triggers restore protection when the original helper exits or its two-hour
deadline passes. A protected mutex prevents suspension racing restoration.
Durable resume intent precedes suspension; the one-reboot limit is a backstop.

Success and failure restore in `finally`. Recovery reidentifies each volume,
verifies encryption, calls `EnableKeyProtectors` only for operation-owned
suspension and verifies protection. The final snapshot must match original
affected volumes and protector IDs for success. External protector rotation
does not prevent recovery restoring protection, but the helper reports the
configuration change. Failed recovery retains its retry task; verified recovery
removes that task and keeps state evidence. [Microsoft restoration API](https://learn.microsoft.com/en-us/windows/win32/secprov/enablekeyprotectors-win32-encryptablevolume)

Protection is restored before returning and before any reboot. This verifies
current Windows state, not the absence of a TPM/PCR recovery prompt on the next
Windows boot. That subsequent boot remains unqualified for each supported
firmware/storage configuration.

## Linux encryption and artifact boundary

Root is LUKS2 with a unique per-install identity and the upstream per-install
bootstrap mechanism. The manifest records `protectionState: "owner-setup-required"`.
Owner setup must replace bootstrap passphrase slots to complete personal
encryption setup. It changes slots, not the underlying LUKS volume key.
Deployment alone does not establish owner confidentiality or volume-key rotation.

Guest backing disk and both exports are encrypted at rest using the ephemeral
staging key. Exports use fixed OpenSSL AES-256-CBC/PBKDF2-SHA256 plus independent
HMAC-SHA256. The provider checks stored/plaintext lengths, ciphertext digest,
HMAC, plaintext digest and actual LUKS2 UUID. ESP plaintext is written to its
intended unencrypted boot partition. No plaintext partition image is staged
on the Windows filesystem.

The operation is owned by Administrators/SYSTEM, disables inheritance and
restricts modification to those identities. The helper also protects ancestors,
inputs and copied recipe. A supplied hash alone cannot authorize arbitrary
deployment. Docker is trusted: pins/ACLs do not attest an engine controlled by
a principal with equivalent Docker rights. Packaged Docker mount/control
permissions still need execution qualification.

Every recipe digest, release/runtime pin, fixed filename/length, unique build
GUID, deferred owner state, portable initramfs and physical first-boot contract
is checked. Manifest commands, arbitrary executable/bootloader paths, container
physical devices and extra Docker/QEMU arguments are rejected, as are reparse
points, UNC paths, alternate streams and comma-containing paths.

## Storage, firmware and interruption

After full image authentication and again after suspension, deployment checks
fresh physical identity, geometry and full native GPT hash. Shrink revalidates
volume identity/limits immediately before resizing; native readback permits
only the approved length change and transient rewrite flags. Existing GUIDs,
offsets, types, attributes and names remain. Allocation checks usable bounds
and occupied extents before adding planned GUIDs with drive letters suppressed.

The writer reidentifies and locks/dismounts only each new partition,
authenticates its immutable ciphertext handle, decrypts the exact base image
directly into the partition, flushes and hashes the entire written range.
Unwritten extra selected root capacity is outside image readback and grows
on first boot.

After both writes verify, permanent UEFI `Boot####` points to
`\EFI\limine\limine_x64.efi`. The OS menu is prepended to `BootOrder`, preserving
the relative order and contents of every existing entry. Both writes are read
back, and the protected journal records the prior order. On a failed order write,
rollback is attempted only if the current order still equals the value we wrote.
The provider does not set `BootNext` and never reboots. Firmware lacks atomic
compare-and-set: concurrent changes cause refusal, and failures can leave an
unused entry for inspection.

`bootMenu` is required in build/plan requests and bound into the constructed
image, its identity, manifest and final plan: `defaultOs` is `omarchy` or `windows`,
and `timeoutSeconds` is 5, 10, 15 or 30. The image's persistent Limine post-hook
preserves these settings after upstream kernel/snapshot updates. Choosing Windows
uses Limine's `efi_boot_entry` protocol, which schedules the uniquely identified
Windows Boot Manager entry and briefly restarts through firmware. Existing
one-time firmware boots, ambiguous/renamed Windows entries, mismatched system EFI
partitions and an overfull boot order are refused before construction.
See [platform behavior](../../docs/platform-behavior.md) for user/team guidance.

The helper can cancel construction using the fixed empty
`<outputDirectory>/cancel.requested` marker while keeping the provider alive
until its terminal event. Build checks at boundaries and during guest/export
work. Deployment honors cancellation before Windows changes; cancellation
ends at `preparing-recovery`, before suspension/shrink/deletion. Approved changes then
finish or enter recovery without a partial-write cancellation action.

Journals record deployment start, exact shrink/deletion start and completion, created
partitions, verified images and receipt. Started operations cannot replay
blindly. Failures may leave the confirmed shrink or deletion applied, incomplete new
partitions or firmware setup. There is no automatic deletion for cleanup or
filesystem rollback: inspect protected operation/recovery records. Physical
boot and owner setup remain unverified.

Implementation model provenance: `gpt-6-astra`.
