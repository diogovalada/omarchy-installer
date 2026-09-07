# USB preservation implementation

The Windows community preview now offers **Keep files and add installer** and
**Erase USB and create installer** after selecting and inspecting a USB. Each has
its own native confirmation; erase has a warning and an explicitly destructive
button. The app does not fall back from preserving files to erasing.

## Supported first profile

- Windows x64 prepares an existing GPT USB with exactly two partitions: an
  unencrypted NTFS basic-data volume and a FAT32 EFI System Partition.
- The exact authenticated Omarchy 4.0.2 ISO, 6,227,752,960 bytes, SHA-256
  `2ef8e624aa1bec7e277e28056b8535a6c9373ba48d7ede3f1a01cb6d2373cfb8`.
- Data free space must cover the ISO plus 64 MiB; EFI needs 16 MiB free.
- Boot target: x64 UEFI with Secure Boot disabled. Creator firmware settings
  do not establish compatibility of a different destination computer.
- Existing data files and the partition table are retained. BIOS sectors and
  other EFI loaders/configurations are not modified. A pre-existing fallback
  file is preserved byte for byte and restored to its original location.

A normal single-partition USB, FAT32 data, exFAT, compressed or encrypted storage, an
unresolved layout, or insufficient free space leaves the preserving action
disabled with an explanation. The raw erase action has independent capacity
and device-safety checks. Ventoy integration and partition conversion are not
implemented in this profile.

## Boot and recovery

The backend creates a uniquely named ISO under the NTFS root, writes it first
as `.iso.partial`, flushes it, and performs SHA-256 readback through an uncached
Windows handle. It publishes the ISO without overwriting another file and keeps
file handles and checked directory guards through boot activation.

GNU GRUB is assembled from pinned Ubuntu `2.12-1ubuntu7.3` modules. Its EFI
prefix resolves configuration on the volume from which firmware loaded it.
The generated configuration uses the official ISO's `boot/grub/loopback.cfg`.
The exact GRUB source archives, Ubuntu patches, build recipe, notices and
license text are included in the provider bundle.

Before replacing `EFI/BOOT/BOOTX64.EFI`, the backend saves and verifies the old
file under `Omarchy-USB-Recovery/previous-BOOTX64.EFI` on the NTFS volume. A
durable `record.json` contains the USB fingerprint, partition GUIDs, operation
ID and boot/configuration/source hashes. The only other new EFI file is
`EFI/Omarchy/grub.cfg`. The user is told that the previous UEFI boot option is
unavailable until restored.

**Restore previous boot setup** works without downloading the ISO. It validates
the USB and backup, refuses to overwrite a subsequently changed loader, restores
the original fallback file, verifies it and removes only unchanged app boot
files. If there was no original fallback loader, it removes only the matching
app loader. Completed recovery records and original loaders are archived with
the operation UUID. The ISO, ordinary files and archive remain on the USB.
Interrupted copies may leave an explicitly named `.iso.partial`; restoring
retains it rather than deleting a file whose partial contents lack a final hash.

Device discovery remains separate from deeper selected-volume inspection.
Compatibility queries have a timeout; access restrictions offer a separate
administrator inspection. Source and system device exclusions are rechecked by
the elevated provider before confirmation and execution. No USB is preselected.

## Validation and limits

The exact production loader and configuration booted the official ISO from an
isolated GPT virtual USB through the Omarchy **Press Return to Start Install**
screen. A diagnostic boot also mounted the NTFS backing volume, ISO loop device
and live root and reached an Omarchy serial login. QEMU had no host devices or
network access. See [production screenshot](../providers/usb-preserve/qualification/production-boot.png)
and [qualification result](../providers/usb-preserve/qualification/production-result.json).

Native tests cover copying/readback, cancellation, preserving ordinary files,
backup/restore round trips, failed verification, changed bootloaders, corrupted
backups and held directory ancestry. UI tests cover both operation modes,
insufficient space, warnings and restoration without an ISO.

The final UI checks passed at 1180×790 and the minimum 820×620 window size,
including disabled preservation for insufficient space and restoration without
an ISO. The loader also reproduced byte for byte in a second build.

This remains a preview: physical USB controllers and firmware have not been
qualified for this new profile. Filesystem corruption or sudden power loss is
not an atomic rollback guarantee. The boot archive shares the USB's failure
domain and is not a backup against USB failure or an explicit later erase.
