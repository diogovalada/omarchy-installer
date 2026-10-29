# Installing without USB from Linux and Intel Macs (design brainstorm, 2026-10-29)

Status: design only. Nothing here is implemented or qualified on hardware.

## The bar

The method should work on every laptop where a USB installation works,
including 4 GB RAM laptops, apart from needing enough disk space. That rules out
booting the downloaded ISO from a file and copying it into RAM (`copytoram`):
it needs roughly 8–16 GB RAM, and an ISO file inside a LUKS volume can't be read
at boot.

Parity has limits the USB route has too. The official installer doesn't shrink
anything: it installs into existing free space or takes a whole disk. USB users
make room with a live tool such as GParted, which shrinks ext4 offline but
can't shrink XFS in general (only a limited shrink at the end of the
filesystem). One gap is ours alone: a temporary installer on the same disk
needs space that survives the installation, so we can't erase the whole disk
the way a USB installation can.

## Shared shape

Keep the Windows design ([staged provider](../providers/staged-iso/README.md)):
make room while the host runs, create a temporary ESP and installer-source
partition outside the host's filesystem, boot them once, let the official
installer install into free space with `copytoram=n`, then remove the temporary
partitions and give the space back. Only small boot files need RAM; the 6 GB
payload stays on disk, unencrypted, so the installer never reads it through the
host's encryption.

Use the staged provider's ownership model on every host: disk identity,
partition GUIDs, exact extents and recorded boot artifacts, revalidated before
each change. Removal waits until Omarchy has booted on its own, and grows a
neighbouring partition only when the freed space is adjacent.

The installer source is either the ISO's files extracted to a filesystem the
installer's initramfs can mount (ext4 on Linux), or the unchanged ISO written
into the partition, which archiso can mount directly via `archisodevice`.
The raw ISO avoids a staging filesystem and can be checked byte for byte, but
Linux's ISO 9660 driver rejects logical sectors larger than 2048 bytes, so it
can't work on native 4K-sector disks. Prototype raw ISO on 512-byte-sector
disks and keep extracted files as the fallback. A FAT32 partition can't hold
the extracted files, since one file can exceed its 4 GiB limit.

## Linux

Describe support by storage capability, not by distro.

| Host disk | Handling |
| --- | --- |
| Unallocated space already present (or a disposable partition, such as swap) | Stage directly from the running system. One reboot. |
| Single-device btrfs, with or without LUKS (Fedora's default) | Shrink online: btrfs, then the open LUKS mapping, then the partition's GPT entry, then tell the kernel (allowed while the partition is in use, as long as its start doesn't move). Gate on tested kernel and tool versions; if anything fails, stop and use the maintenance boot. One reboot. |
| ext4, also inside LVM and/or LUKS (Ubuntu, Mint, Debian) | Maintenance boot (below). Two reboots. |
| TPM-bound LUKS | As above, but first require a working passphrase or recovery key: turning Secure Boot off changes what the TPM checks. |
| XFS, multi-device btrfs, RAID, thin pools, unusual stacks | Only existing free space. Don't shrink automatically. |
| Legacy BIOS, MBR, 32-bit UEFI | Out of scope; the same-disk installer requires UEFI and GPT. |

**Maintenance boot.** A small Linux environment shrinks what can't shrink while
mounted:

1. While the host runs: download and verify the ISO into the host filesystem,
   record the storage stack and the reviewed changes, and put the maintenance
   image on the ESP as a Unified Kernel Image. Boot it once with a temporary
   firmware entry and `BootNext`, without touching the host's bootloader or
   initramfs.
2. In the maintenance environment: unlock LUKS (the user types the passphrase),
   deactivate swap, check and shrink the filesystem, then each outer layer
   (LV, PV, LUKS, partition), in nesting order.
3. Mount the shrunk host filesystem read-only and copy the ISO onto the new
   temporary source partition, then verify the copy.
4. Reboot normally into the installer. `kexec` could save this reboot later, but
   it skips firmware initialisation right after disk changes, so it isn't the
   default.

Build the image from the ISO release's own kernel, modules and firmware, so it
supports the same storage hardware as booting that ISO from USB. Add our own
initramfs with cryptsetup, LVM and filesystem tools. The ISO's initramfs can't
be reused unchanged, since it only knows how to boot archiso. Record versions
and hashes. An upstream "maintenance" build would be welcome later but isn't a
prerequisite.

If the ESP is too small (the image is probably 100–300 MiB compressed; measure
it), fall back to a qualified GRUB route that loads the image from an
unencrypted `/boot` (which does change the host's boot configuration), then a
disposable partition, then decline. systemd-boot's one-shot entry can't reach
files outside the ESP or XBOOTLDR partition.

Offline ext4 shrinking can take a long time and isn't safe against power loss.
Show named phases and progress (`e2fsck -C`, `resize2fs -p`), require AC power
and a battery reserve, prevent suspend, and never kill a running resize.
`resize2fs -P` only estimates a minimum size; use it for sizing with headroom,
not as a dry run or a duration estimate. Keep operation records outside the
filesystem being resized.

## Intel Macs

- **App reach first.** The preview package requires macOS 15, which runs on few
  Intel Macs (mostly 2018+ T2 models). Deliver this feature through a small
  standalone launcher or helper targeting macOS 10.13.6, or 10.12 for older
  HFS+ systems, rather than lowering the whole app's floor blind. WKWebView
  comes from the OS, so test the real interface on old versions before
  promising them.
- **Making room** is easy online with Apple's tools: `diskutil apfs
  resizeContainer` (FileVault can stay on), `diskutil resizeVolume` for HFS+,
  and `coreStorage resizeStack` for legacy FileVault. Report the real blocker
  when local snapshots or filesystem errors prevent shrinking (snapshot
  guidance, Recovery First Aid).
- **Temporary partitions**: a FAT32 ESP with the loader at
  `EFI/BOOT/BOOTX64.EFI`, plus a separate source partition, right after the
  macOS container. Writing raw ISO bytes to a fresh internal partition with SIP
  on is a gating experiment. Unmount the partition and stop macOS from
  remounting it while writing (Disk Arbitration).
- **One-time boot**: Option key at startup is the supported route, on T2 Macs
  in particular. Use `bless --setBoot --nextonly` only on combinations that pass
  testing. SIP is known to block it in some setups.
- **T2 Macs**: Startup Security set to No Security in Recovery (don't ask users
  to disable SIP). The fork already contains T2 kernel integration; what's left
  is qualifying the artifacts, including keyboard input at the disk password
  prompt in the installed initramfs.
- **Better than USB**: while macOS is unlocked, the app can copy the Wi-Fi and
  Bluetooth firmware that T2 Macs need from macOS onto the temporary ESP,
  locally for that machine only, never bundled or uploaded. The installer needs
  a small import step for the live system and the installed one. Check Apple's
  licence terms before shipping.
- **Default boot after install**: keep macOS as the default. To make Omarchy
  the default, hold Option at startup, then hold Control while choosing it;
  to go back, pick macOS in Startup Disk. macOS upgrades can reset this. Apple
  firmware largely ignores `efibootmgr` entries, so Omarchy's ESP should also
  carry `EFI/BOOT/BOOTX64.EFI`, kept up to date by Omarchy.
- **Out of scope at first**: Fusion Drive, RAID, multiple physical stores,
  hybrid-MBR Boot Camp, and 32-bit EFI Macs.

## What upstream needs

1. A release containing omarchy-iso#187 (installing into free space while
   protecting the installer source and ESPs), qualified against the real ISO.
2. On Intel Macs: `EFI/BOOT/BOOTX64.EFI` on Omarchy's ESP, maintained across
   updates, and an import step for firmware copied from macOS.
3. Optional: a maintenance image built with each ISO release.

Shrinking, unlocking, staging, boot scheduling and cleanup stay in the app.

## Verify first, in order

1. Complete installations on 4 GB RAM machines, measuring peak memory during
   LUKS unlocking, filesystem checks, staging and the installer.
2. Interrupted operations: inject failures around shrinking, LVM changes,
   partition-table writes and staging; show recovery without blind replays.
3. Common layouts end to end: Ubuntu/Mint LUKS + LVM + ext4, Fedora LUKS +
   btrfs, encrypted APFS, including the return boot and cleanup.
4. Real firmware on several vendors and Mac models: maintenance image size,
   small ESPs, `BootNext`, Option and Control selection, macOS return.
5. The released ISO: raw ISO versus extracted sources, source protection,
   512-byte and 4K-sector disks, removal after Omarchy boots on its own.
