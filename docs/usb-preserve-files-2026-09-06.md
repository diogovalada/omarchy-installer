> **Current scope, 2026-09-07:** Keep-files preparation and verified bootloader backup are active again. Only automatic boot restoration remains deferred. The design below includes historical and future proposals; see [implemented scope](usb-preserve-implementation-2026-09-07.md).


# USB creation with existing files

Date: 2026-09-06. User requirement: offer a choice between replacing USB contents
and adding the bootable Omarchy installer while preserving existing files when
the drive's layout and boot configuration are compatible.

Status, updated September 7: the Windows preview implements separate keep/erase
actions and boot backup/restoration for a narrow GPT + NTFS data + FAT32 EFI
profile. The official ISO has booted through this path in isolated virtual media.
See the [implementation record](usb-preserve-implementation-2026-09-07.md).
Ventoy integration, generic conversion and physical qualification remain pending.

The [September 7 product proposal](usb-product-direction-2026-09-07.md) develops
this requirement into a recommended flow, compatibility decisions, recovery
behavior, and the broader delivery sequence beyond this first profile.

## Intended choices

1. **Erase USB and create installer.** The existing verified raw-image path
   replaces the drive's visible layout/content. This is not a secure-erasure
   claim: the provider writes only the approved image span and bounded padding.
2. **Keep files and add installer.** Enable only after detecting a supported
   layout and boot method. Preserve existing user files and existing supported
   boot choices; never silently switch to erasing, overwrite an unknown loader,
   or treat copying an ISO alone as proof of bootability. Explain a concrete
   incompatibility when unavailable.

An existing bootable system is not automatically a conflict. A recognized
multiboot loader can offer multiple installers. Ordinary documents are not a
boot conflict; loader ownership, filesystem limits and available space matter.

## Confirmed constraints

- UEFI removable-media startup uses an architecture-specific fallback loader;
  for x64 it is `\EFI\BOOT\BOOTX64.EFI`. Replacing it can replace another
  system's startup path. Legacy BIOS uses boot code in disk/partition sectors,
  so inspecting filenames alone is insufficient. Sources:
  [UEFI boot specification](https://uefi.org/specs/UEFI/2.10/03_Boot_Manager.html#removable-media-boot-behavior),
  [GRUB BIOS installation](https://www.gnu.org/software/grub/manual/grub/html_node/BIOS-installation.html).
- Read-only inspection of the previously verified local Omarchy 4.0.2 ISO
  found `arch/x86_64/airootfs.sfs` at **5,915,328,512 bytes**. Neither that
  file nor the complete 6,227,752,960-byte ISO fits within FAT32's single-file
  limit. Having enough total free space does not fix this.
  [Microsoft filesystem limits](https://learn.microsoft.com/en-us/windows/win32/fileio/filesystem-functionality-comparison#limits).
- The same ISO's normal GRUB configuration passes
  `archisosearchuuid=2026-08-31-03-24-58-00`. Blind extraction onto an existing
  filesystem does not preserve that ISO identity. Its `boot/grub/loopback.cfg`
  instead supports `iso_path`, `img_dev=UUID=...` and `img_loop=...`, making
  a stored-ISO boot method a concrete candidate. Presence of this configuration
  is source evidence, not proof of booting through our proposed integration.

## Implementation direction

The first candidate is adding the verified ISO to an **existing, recognized
Ventoy drive**, preserving both ordinary files and other ISOs. Ventoy explicitly
supports storing multiple bootable images and selecting them from a menu.
[Ventoy overview](https://www.ventoy.net/en/index.html).
Pin and qualify the loader/version/layout/configuration plus the exact Omarchy
ISO before marking this combination supported. A label or folder named Ventoy
alone is not sufficient identity or compatibility evidence.

Ordinary data USBs also belong in the desired feature scope. Adding a boot
partition/loader while retaining their filesystem is a separate conversion path
requiring its own inspected plan. Ventoy documents experimental non-destructive
installation with specific partition/start-offset/filesystem/free-space rules;
it overwrites the MBR and first 1 MiB and must not replace an existing MBR
bootloader. Do not equate this with simply adding a file, or infer that every
NTFS/exFAT USB is supported.
[Ventoy conversion requirements](https://www.ventoy.net/en/doc_non_destructive.html).

The preserving backend must bind the exact USB and filesystem identity, known
boot configuration, verified image, free space and new file paths into its plan.
It must refuse conflicting existing files, verify copied data after flushing,
retain unrelated files and boot entries, and clean only files created by that
operation after interruption. Any required shrinking or boot metadata change
must appear in the reviewed plan. It must not reuse the raw-image write command.

Qualification must cover ordinary data and multiple-ISO coexistence, unknown or
conflicting loaders, FAT32/NTFS/exFAT limits, insufficient space, duplicate image
names, interrupted copies, device replacement and the actual supported firmware
boot paths. BIOS/UEFI files in an ISO alone do not establish a supported Omarchy
installation on every firmware type. Expose the preserving action only once its
backend and tested compatibility profiles exist.
