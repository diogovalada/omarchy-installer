# USB creation: product direction

Date: 2026-09-07. Status: the two-action flow and a bounded replace-and-restore
profile are implemented in the Windows community preview. See the
[implementation and qualification record](usb-preserve-implementation-2026-09-07.md)
for its exact scope. Remaining sections describe the broader product direction.
No physical storage operation is authorized by this document.

Builds on [the preservation requirement](usb-preserve-files-2026-09-06.md).

## Recommendation

Make keeping files the preferred outcome whenever a tested method exists. Let
the app determine the available method from the selected drive. The user should
choose what happens to their belongings without needing to choose UEFI, BIOS,
partition tables, a bootloader, or a filesystem.

Use the user's proposed two-option flow: **Add installer and keep files** and
**Erase USB and create installer**. Alongside requires both enough free space
and a qualified boot/layout method. Give each action its own confirmation dialog.
Never change the selected intention after a failed check. Unsupported preservation
should offer a visible reason and a practical next step.

“Alongside” means existing files stay on the drive. A future backup/recreate/
restore workflow belongs in a separate, explicitly named migration flow; it must
not be hidden behind this action. If a supported in-place boot preparation is
eventually included, its exact partition/boot changes belong in the review.

The first implemented profile retains an existing NTFS data partition and FAT32
EFI partition on GPT, adding the ISO and using a pinned GRUB UEFI loader. Its
previous fallback loader is backed up and can be restored in place. This avoids
partition conversion and BIOS sector writes while providing the requested
archive-and-restore behavior. Existing Ventoy integration remains a separate
future profile; it is not implemented by this loader replacement path. Ordinary
USBs containing documents remain in the wider product scope.

## Recommended flow

1. Check for USB drives and list candidates. Automatic discovery on opening the
   panel is a possible convenience; keep Check/Refresh available. React to
   connection/removal while the panel is open. Listing should work while the ISO
   downloads or verifies. A candidate is not yet authorized or ready for writing.
2. Select a drive identified by model, label, capacity and current volume name.
   No drive is preselected. Show used/free space when available, without walking
   every file or guessing a file count.
3. Inspect the selected drive's layout, filesystem, loader and relevant settings.
   Show a local Checking compatibility state. Do not delay the entire list or
   hash all user files for this step. Preserve independent action states:
   supported, checking, unsupported, or unable to determine.
4. Offer the two actions together. Prefer Add installer and keep files when
   supported; show free space, required space and the relevant result of the
   compatibility check. Disable it while checking, when space is insufficient,
   or when compatibility is unsupported/unknown, with a visible explanation.
   Never automatically select Erase USB when alongside becomes unavailable.
5. Both actions open a confirmation dialog. The alongside dialog names the USB,
   image/version, new space usage and any supported boot changes, and confirms
   that existing files are retained. Its final button is Add installer. The
   erase dialog names the physical drive, label, capacity and affected volumes,
   warns that all files and existing boot setups will be lost, and ends with
   Erase USB and create installer. Both have Cancel; only erase uses destructive
   emphasis. Avoid generic Are you sure, a warning for ordinary copying, and
   several successive confirmation dialogs. Host administrator permission is
   separate from confirmation of the storage operation.
6. Execute with named stages, real byte progress and cancellation behavior.
   Completion distinguishes verified on-drive files/layout from a boot actually
   observed on the destination computer. Offer safe eject and boot instructions.

Example for a supported existing boot USB: “Your files and existing boot menu
will be kept. Omarchy needs another 6.23 GB.” The exact storage number comes from
the resolved release, allocation rounding and any operation overhead.

Disabled alongside reasons should be specific and visible, for example:
“Needs 6.23 GB; 4.10 GB available,” “This filesystem cannot hold the installer
file,” or “This drive's existing boot setup is not supported for adding Omarchy.”
Enough space does not guarantee a bootable result. Erase eligibility uses total
usable device capacity rather than the current filesystem's free space, and
still requires a writable, eligible target. An unknown contents scan must not
describe the drive as empty.

## What each drive should offer

| Observed state | Recommended behavior | Important boundary |
| --- | --- | --- |
| Recognized, qualified Ventoy layout with a compatible filesystem/menu | Add installer, keeping files | Preserve loader and existing settings; no implicit loader upgrade |
| Same authenticated Omarchy release already on that drive | Verify existing installer and reuse it | Filename/size alone do not justify Already ready |
| Older Omarchy installer already present | Add the new version alongside it | Removing the old image is a separate explicit choice |
| Ordinary data USB with a qualified conversion method | Prepare USB and keep files | Explain boot/partition changes; this is not a copy-only operation |
| Ordinary data USB that cannot be converted in place | Offer a separate backup/recreate/restore workflow when supported, or another USB | Never silently format under an Add installer button |
| Unknown/custom bootloader or recovery media | Explain that keeping its boot setup is unsupported | Existing bootability is neither automatic conflict nor proof of compatibility |
| FAT32 data volume | Explain that the current ISO exceeds its per-file limit | Free capacity alone cannot enable the keep route |
| Insufficient free space, locked/encrypted volume, or unresolved layout | Explain the specific obstacle | Do not delete unrelated files or change to erase automatically |
| Empty USB or user explicitly chooses erase | Use the qualified standard image-write method initially | A reusable multiboot layout can be added after its own boot qualification |

The target's supported firmware/architecture matters. The computer creating
the USB may not be the computer that will boot it. Avoid making boot promises
from the creator's firmware settings or from the mere presence of EFI files.

## Existing boot manager integration

Use a narrowly supported Ventoy integration rather than maintaining a new
general-purpose bootloader. Ventoy is designed to hold ISO images and ordinary
files together ([official start guide](https://www.ventoy.net/en/doc_start.html)).
Treat “qualified” as a tested combination of exact Omarchy image, loader version,
disk layout, data filesystem, relevant configuration, host write implementation
and target boot mode. Identify real structures and loader files; labels are hints.

A copied ISO must actually appear in the boot menu. Existing search roots,
depth limits, ignore files, allowlists and other supported settings affect this.
Choose a compatible path under the existing policy; if a settings change is
necessary, show and approve the exact change, or declare that profile unsupported.
Do not replace a user's configuration with app defaults. Ventoy documents both
[search filtering](https://www.ventoy.net/en/doc_search_path.html) and
[explicit image lists](https://www.ventoy.net/en/plugin_imagelist.html).

Prefer a tidy app directory such as `Omarchy/` only when the existing boot menu
will discover it. Verify an occupied target before reuse; use a new planned name
or stop on a different file. Do not overwrite based on filename. Record ownership
of created files so removing our installer later cannot remove unrelated data.

For newly initialized reusable drives, choose a simple data/installer directory
convention and a menu that avoids unnecessary traversal of ordinary data. Keep
the raw-image method available for the compatibility profiles where it is needed.

## Ordinary USBs and conversion

### Proposed bootloader backup and replacement

The user proposed archiving an existing USB bootloader on the same drive and
informing the user. This can be part of a qualified in-place migration, but an
archive alone neither keeps the previous installer bootable nor establishes that
the new installer will boot.

For a recognized setup, back up every boot file/configuration item the operation
will change, with original paths, hashes and relevant layout/identity information.
Verify the backup before replacement and test the exact restoration procedure.
Do not assume the x64 EFI fallback executable is self-contained: GRUB, for example,
can locate its configuration and modules using an embedded path or filesystem
identity ([GRUB configuration](https://www.gnu.org/software/grub/manual/grub/html_node/Embedded-configuration.html)).
BIOS boot code can also occupy areas outside ordinary files
([GRUB BIOS installation](https://www.gnu.org/software/grub/manual/grub/html_node/BIOS-installation.html)).

Prefer keeping both boot choices available through a tested menu integration.
Loading the old loader from a new menu is possible in supported setups, but moving
its executable and adding a menu item is not sufficient proof that its dependent
paths still work ([chain-loading](https://www.gnu.org/software/grub/manual/grub/html_node/Chain_002dloading.html)).
Otherwise, a supported replace-and-restore profile must state before confirmation:
“Your files will stay. The existing USB boot setup will be replaced and backed
up. Its previous boot option will be unavailable until restored.” Show the backup
location and provide a qualified Restore previous USB boot setup action. Restore
must detect subsequent user changes instead of blindly overwriting newer state.

An on-USB archive is only suitable if its filesystem and storage are retained.
It cannot protect against formatting that drive; operations that threaten that
storage need a verified backup on another physical device. This proposal does
not make unknown boot layouts eligible for alongside mode or remove the new
installer's filesystem, capacity and boot-compatibility checks.

### Preparing ordinary data drives

Evaluate conversion separately from adding to an already prepared drive.
Ventoy's own non-destructive installation is explicitly experimental, changes
early boot sectors, and has layout/filesystem requirements. That makes it a
candidate for a bounded, tested adapter, not a default universal keep-files path.
([Official conversion documentation](https://www.ventoy.net/en/doc_non_destructive.html))

For broad ordinary-data support, prefer developing a visible backup/recreate/
restore workflow before promising generic in-place conversion. It is slower and
needs host storage, but provides a recovery copy before the original is changed.
It must meet these conditions before becoming available:

- Backup destination is on another physical device, has enough space, and is
  shown to the user. Verify the complete selected preservation scope before
  allowing any USB layout mutation; unreadable/unaccounted-for files stop it.
- The recreated USB has room for all restored data, the ISO and boot overhead.
  Detect filename, metadata, permissions, streams, link and filesystem-semantic
  incompatibilities up front. Preserve supported semantics or refuse the route;
  do not silently turn an NTFS volume into a lossy exFAT copy.
- Quiesce/revalidate the source to prevent changes after the verified backup.
  The reviewed action explicitly says that the USB will be erased and recreated.
- Keep a durable recovery journal and verified backup across cancellation,
  unplugging, reboot and application crash. Only erase after the backup checkpoint.
  Interrupted restore must remain recoverable; do not promise atomic rollback.
- Restore to the agreed paths, verify the result, and retain the host backup
  until the user explicitly removes it. Do not put it in auto-cleaned temp storage.

A file backup does not preserve a custom bootloader, encryption setup, recovery
partition or complete disk identity. Do not market this route as preserving those
structures. Unknown boot/recovery drives need a separate full-device migration
design or another USB, even if their ordinary files can be backed up.

## Performance and recovery

Separate quick device discovery, selected-drive compatibility checks and file
verification. One old drive with many files should not block choosing another.
Only a selected backup route needs a complete file inventory. Hash the new ISO
after copying/flushing; do not reread unrelated user files on a copy-only route.

Copy into an operation-owned temporary filename on the target volume, verify,
then publish without replacing an existing file. Respect the actual filesystem's
durability/rename guarantees; a rename alone is not a power-loss guarantee.
After interruption, identify and resume or clean only this operation's partial
file. Revalidate hardware and volume identity, source, free space and relevant
boot settings immediately before mutation, including after replugging.

Show Copying installer, Verifying USB and Ejecting, with byte progress when
available. Keep 100% copying distinct from success. Existing boot entries and
ordinary files remain untouched on the add-only path. Tests must simulate full
media, stale plans, device replacement, changed configuration and unplugging.

## Delivery and evidence

1. Add selected-drive compatibility inspection and distinct action capabilities
   to the native contract. Keep shared system/source-device exclusions. The
   current single `eligible` flag represents raw-write eligibility and cannot
   stand in for file-preserving eligibility.
2. Qualify the current Omarchy ISO through a pinned Ventoy profile in isolated
   virtual media, then sacrificial physical USBs on the advertised target firmware.
   Test through installer startup and source access, not just the Ventoy menu.
   Test coexistence with ordinary files and another ISO, including configured
   menus and interruption. No automatic loader/firmware security-policy changes.
3. Ship Add installer and existing-image reuse for those profiles, with the
   honest unsupported explanations and the existing explicit erase route.
4. Add a qualified reusable layout for newly prepared USBs, then the recoverable
   backup/recreate/restore route for ordinary-data drives. Investigate bounded
   in-place conversion as a later speed improvement; release it by supported
   profile, not an override checkbox.

There is a first-hand report of Omarchy 3.8.4 booting with Ventoy 1.1.16 on one
UEFI/GPT machine ([report](https://github.com/ventoy/Ventoy/issues/3697)), and an
older report where Omarchy 3.0.1 failed through Ventoy while direct writing worked
([report](https://github.com/omacom/omarchy/issues/1814)). These motivate exact
qualification; neither establishes support for our current 4.0.2 ISO. Ventoy's
own Secure Boot support also does not qualify the entire Omarchy boot chain
([Ventoy documentation](https://www.ventoy.net/en/doc_secure.html)).

Success criteria: existing data remains usable, existing supported boot choices
remain available, the Omarchy installer reaches its working environment, retry
does not duplicate/corrupt content, and the user can tell exactly what will change.
