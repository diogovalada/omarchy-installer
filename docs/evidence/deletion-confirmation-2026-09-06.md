# Partition replacement and typed confirmation

Date: 2026-09-06. Implementation model: `gpt-6-astra`.

The user approved deleting eligible unused OS/data partitions to make room for
Omarchy, and requested a clear warning plus a typed, easy partition identifier.
This implementation adds that path to Windows direct installation. The native
Apple flow continues to manage macOS storage through its existing upstream engine.

## User interaction

- Replacement choices are behind **Replace an existing partition…**.
- A modal names the disk, partition, filesystem and entire partition size. It
  states that all files and any OS there will be lost, even when the requested
  Omarchy allocation is smaller than the old partition.
- The user types **Disk N Partition M**. Capitalization is ignored; a different
  identifier cannot continue. Keep partition receives initial focus; Escape
  cancels. The native dialog provides focus containment.
- Confirmation queues replacement and starts preparation. The existing final
  native installation confirmation repeats the exact destructive action before
  any storage mutation. Refreshing the inventory invalidates old choice IDs.

## Backend boundary

The selected disk ID, partition GUID, number, offset and size pass through the
typed Rust protocol, elevated helper and protected plan. The backend independently
requires the canonical typed identifier. Deletion follows image authentication,
final approval, fresh layout checks and protected BitLocker recovery preparation.

The provider excludes current OS/system/EFI/recovery/reserved storage, required or
unknown GPT attributes, hidden/offline/read-only/shadow partitions, paging/dump
and installer/source volumes. Path identities resolve through native handles;
ambiguous role, recovery or filesystem information prevents deletion. Windows
encrypted volumes, LUKS/unknown containers, LVM/RAID and multi-device Btrfs are
excluded. Plain supported Windows data and standalone ext/Btrfs partitions can
qualify. These checks do not enable cross-platform resizing.

The provider locks a Windows volume, or takes exclusive access to an unmounted
Linux partition, before calling `Remove-Partition`. It does not override Windows
system restrictions or force a dismount. GPT readback verifies the selected
partition disappeared and other entries retain their identity, extent, number,
name and attributes. Intent and completion are journaled. A deleted unencrypted
data volume is explicitly excluded from the final retained-volume BitLocker
comparison. Deletion is not automatically rolled back after a later failure.

## Validation boundary

Svelte/TypeScript checking passed with zero errors and four pre-existing warnings.
Rust library checking and the complete Windows native desktop build passed.
Windows PowerShell parsed every direct-provider module and compiled the C# storage
code without executing provider actions. All 10 direct-provider files match their
source, staging and packaged hashes; the bundle contains 6,144 authenticated files.
See the [build record](deletion-confirmation-build-2026-09-06.json).

Native executable SHA-256:
`333dd1fea6c2cd010db51a7b07d7aacf9617fb10e4db5bf5e6134d3607d269fd`.

Execution tests remain deferred as requested: no new disk probe, locking, deletion, installation,
BitLocker change, scheduled recovery task, VM or reboot is run for this change.
The warning modal's runtime interaction and native deletion/locking need separate
qualification before a supported release.

Relevant API contracts: [Windows deletion restrictions](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/delete-partition),
[volume locking](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_lock_volume),
and [partition roles](https://learn.microsoft.com/en-us/windows-hardware/drivers/storage/msft-partition).
