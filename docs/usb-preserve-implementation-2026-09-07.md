# USB preservation implementation

The Windows preview offers **Keep files and add installer** and **Erase USB and
create installer** after selecting a USB. Each has its own in-app review before requesting administrator access.
Unsupported preservation is disabled with reasons, never converted into erasing.
Only **Restore previous boot setup** is deferred. Its implementation and tests
remain in the [historical archive](../archive/usb-preserve-2026-09-07/README.md).

## Supported profile

- Windows x64; existing GPT USB with exactly two partitions: an unencrypted,
  uncompressed NTFS basic-data volume and a FAT32 EFI System Partition.
- Authenticated Omarchy 4.0.2 ISO: 6,227,752,960 bytes, SHA-256
  `2ef8e624aa1bec7e277e28056b8535a6c9373ba48d7ede3f1a01cb6d2373cfb8`.
- NTFS free space must cover the ISO plus 64 MiB; EFI needs 16 MiB free.
- Destination computer boots x64 UEFI with Secure Boot disabled.
- Existing files and partitions are retained. A previous fallback EFI loader is
  saved and verified before replacement. Its previous boot option is replaced;
  the UI and confirmation state that automatic restoration is unavailable.

Ordinary single-partition FAT32 USBs, exFAT, encrypted storage and other layouts
remain unsupported. Ventoy integration and partition conversion are not
implemented. Available free space is reported even when a layout is unsupported.
The connected 7.46 GiB USB was inspected read-only on 2026-09-07: a FAT32 volume
with 720,408,576 bytes free. It fails both layout and available-space requirements.

## Confirmation flow

Selecting an action opens a review inside the primary app, with the USB name,
readable capacity, installer name and the effect on existing files. Cancel
dismisses the review without elevation. Confirm consumes a single-use native
review token bound to the source image and destination; reviews expire after
five minutes. Changed inputs require another review.

Only then is the helper elevated. Its final write challenge must exactly match
the approved plan and can be accepted only once; USB operations no longer show
a second native confirmation dialog. Device identity and write safety checks
still run in the elevated helper. Read-only compatibility inspection remains a
separate explicitly labelled action where administrator access is needed.

Blocked preservation uses a short red lead-in with a normal-color summary.
Technical reasons are available under Details.

## Writes and guards

The helper revalidates source/system exclusions, USB identity and layout before
confirmation and again before writing. The source is authenticated in protected
staging. A uniquely named ISO is written to a partial file, flushed and read back
through an uncached SHA-256 verification handle before publication. Existing
paths cannot be overwritten through links or junctions; checked directory and
file handles remain held through boot activation.

Before replacing `EFI/BOOT/BOOTX64.EFI`, the helper saves and verifies its previous
bytes at `Omarchy-USB-Recovery/previous-BOOTX64.EFI` on NTFS. A durable
`record.json` binds the backup and operation to the USB fingerprint, partition
GUIDs and content hashes. Configuration is added at `EFI/Omarchy/grub.cfg`.
Existing preparation records block another addition. Interrupted operations may
retain preparation files and the archive; automatic cleanup/restoration is deferred.

GNU GRUB is assembled from pinned Ubuntu `2.12-1ubuntu7.3` modules and loads the
ISO's upstream loopback configuration. Exact source archives, patches, licenses
and build recipe are included in the provider bundle.

## Evidence and limits

The unchanged production loader and configuration previously booted the official
ISO from a GPT virtual USB through the Omarchy installer start screen under
QEMU/OVMF. See [boot image](../providers/usb-preserve/qualification/production-boot.png)
and [result](../providers/usb-preserve/qualification/production-result.json).
This is virtual evidence; physical USB controllers and firmware are not qualified.

Current native tests cover verified copying, existing file protection, original
boot backup, failed verification, cancellation and held directory ancestry. UI
tests cover both choices, disabled preservation with its reason, and the absence
of the deferred restoration button. The portable launcher retains its verified
payload cache and foreground behavior.

Sudden power loss is not an atomic rollback guarantee. The boot archive shares
the USB's failure domain and does not survive an explicit later erase.
