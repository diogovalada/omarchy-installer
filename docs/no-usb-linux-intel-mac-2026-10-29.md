# Installing without USB from Linux and Intel Macs (design brainstorm, 2026-10-29)

Status: design only. Nothing here is implemented or qualified on hardware.

## The bar

The method should work on every laptop where a USB installation works,
including 4 GB RAM laptops, apart from needing enough disk space. That rules out
booting the downloaded ISO from a file and copying it into RAM (`copytoram`):
it needs roughly 8–16 GB RAM, and an ISO file inside a LUKS volume can't be read
at boot. Reading the ISO in place without RAM would also block replacing the
existing system, since the installer would live inside the partition being
replaced.

Parity has limits the USB route has too. The official installer doesn't shrink
anything: it installs into existing free space or takes a whole disk. USB users
make room with a live tool such as GParted, which shrinks ext4 offline but
can't shrink XFS in general (only a limited shrink at the end of the
filesystem). One gap is ours alone: a temporary installer on the same disk
needs space that survives the installation, so we can't erase the whole disk
the way a USB installation can.

The first version below has explicit exclusions. "Every laptop where USB
works" is the coverage roadmap, not a claim for the first release.

## Shared shape

Keep the Windows design ([staged provider](../providers/staged-iso/README.md)):
make room, create a temporary ESP and installer-source partition outside the
host's filesystem, boot them once, let the official installer install into free
space with `copytoram=n`, then remove the temporary partitions and give the
space back. Only small boot files need RAM; the 6 GB payload stays on disk,
unencrypted, so the installer never reads it through the host's encryption.

One staging method on every host: extract the ISO's files to a filesystem the
host writes natively and the installer's initramfs can mount, hashing each file
while copying and comparing a readback against the ISO's manifest. NTFS on
Windows (unchanged), ext4 on Linux, exFAT on macOS. FAT32 can't hold the
5.89 GB squashfs. The released initramfs includes `exfat` (the `filesystems`
hook without `autodetect`), and the packaged GRUB loader embeds it; qualify both
against each released ISO.

Writing the unchanged ISO into the partition was considered and dropped: it
would be a second method to qualify, and Linux can't mount ISO 9660 on disks
with logical sectors larger than 2048 bytes. T2 Macs' Apple SSDs report
4096-byte sectors, so it would fail on exactly those machines.

Use the staged provider's ownership model on every host: disk identity,
partition GUIDs, exact extents and recorded boot artifacts, revalidated before
each change. Removal waits until Omarchy has booted on its own, and grows a
neighbouring partition only when the freed space is adjacent.

## Linux

Describe support by storage capability, not by distro.

| Host disk | First version |
| --- | --- |
| Unallocated space already present (or a disposable partition, such as swap) | Stage directly from the running system. One reboot. |
| ext4 or single-device btrfs, also inside LVM and/or LUKS (Ubuntu, Mint, Debian, Fedora) | Maintenance boot (below). Two reboots. |
| TPM-bound LUKS | As above, but first require a working passphrase or recovery key: turning Secure Boot off changes what the TPM checks. |
| XFS, multi-device btrfs, RAID, thin pools, unusual stacks | Only existing free space. Don't shrink automatically. |
| Legacy BIOS, MBR, 32-bit UEFI | Out of scope; the same-disk installer requires UEFI and GPT. |

**Maintenance boot.** A small Linux environment does all the shrinking:

1. While the host runs: download and verify the ISO into the host filesystem,
   record the storage stack and the reviewed changes, and put the maintenance
   image on the ESP as a Unified Kernel Image. Boot it once with a temporary
   firmware entry and `BootNext`, without touching the host's bootloader or
   initramfs.
2. In the maintenance environment: unlock LUKS (the user types the passphrase),
   deactivate swap, shrink the filesystem (ext4 unmounted; btrfs mounted
   temporarily, then unmounted), then each outer layer (LV, PV, LUKS,
   partition), in nesting order.
3. Mount the shrunk host filesystem read-only, copy the ISO's files onto the new
   temporary source partition, and verify them.
4. Reboot normally into the installer.

Build the image from the ISO release's own kernel, modules and firmware, so it
supports the same storage hardware as booting that ISO from USB. Add our own
initramfs with cryptsetup, LVM and filesystem tools. The ISO's initramfs can't
be reused unchanged, since it only knows how to boot archiso. Record versions
and hashes.

**Space on the ESP.** Default ESPs on Ubuntu, Fedora and Mint are 512 MiB to
1 GiB; the image is probably 100–300 MiB compressed. Build and measure it
first. If it fits representative machines, the first version needs room on the
existing ESP or declines with a clear message. If common machines don't have
room, one qualified fallback belongs in the first version: GRUB loading the
image from an unencrypted `/boot` (which does change the host's boot
configuration). systemd-boot's one-shot entry can't reach files outside the ESP
or XBOOTLDR partition.

**Shrinking safely.** Offline shrinking can take a long time and isn't safe
against power loss. Show named phases and progress (`e2fsck -C`,
`resize2fs -p`), require AC power and a battery reserve, prevent suspend, and
never kill a running resize. `resize2fs -P` only estimates a minimum size; use
it for sizing with headroom, not as a dry run or a duration estimate. Keep
operation records outside the filesystem being resized.

**Replacing Linux.** Put the temporary partitions after the part of the old
Linux partition that's kept. Once they're verified, the installer deletes the
old Linux and installs into the space before them; afterwards the app, running
in Omarchy, removes them and grows Omarchy's partition, then its LUKS mapping,
then btrfs, all online. This needs about 7 GB for the temporary partitions,
plus room for the downloaded ISO inside Linux and some headroom. Omarchy's
partition must end right before the temporary ones to reclaim them, and the
fork currently picks the largest free region, so check the adjacency. Keep
cleanup records outside the deleted Linux. Replacing XFS, or a disk with no
shrinkable space, remains a gap.

## Intel Macs

- **The main app covers them.** The Intel package's minimum drops from macOS 15
  to 10.13.6 (system WebKit from Safari 13.1.2), which reaches Intel Macs from
  about 2010. Set explicit Vite JavaScript and CSS targets for Safari 13.1 (the
  current configuration sets neither), audit generated code for runtime APIs
  that need polyfills, replace flexbox `gap` (Safari 14.1) and the `:has()` rule
  (Safari 15.4), check every packaged binary's deployment target, and test the
  packaged app on real 10.13.
- **Making room** is easy online with Apple's tools: `diskutil apfs
  resizeContainer` (FileVault can stay on), `diskutil resizeVolume` for HFS+,
  and `coreStorage resizeStack` for legacy FileVault. Report the real blocker
  when local snapshots or filesystem errors prevent shrinking (snapshot
  guidance, Recovery First Aid).
- **Temporary partitions**: a FAT32 ESP with the loader at
  `EFI/BOOT/BOOTX64.EFI`, plus an exFAT source partition, right after the macOS
  container. Mount the source privately (`nobrowse`), turn Spotlight off for it
  (`mdutil -i off`), create `.fseventsd/no_log`, and copy without extended
  attributes so macOS doesn't add `._*` files. Verification checks every
  expected file's size and hash, ignores only narrowly recognised macOS metadata,
  and rejects anything else.
- **One-time boot**: hold Option at startup and pick the installer.
- **T2 Macs**: Startup Security set to No Security in Recovery (don't ask users
  to disable SIP). The fork already contains T2 kernel integration; qualify its
  artifacts, including keyboard input at the disk password prompt in the
  installed initramfs.
- **Default boot after install**: keep macOS as the default. To make Omarchy
  the default, hold Option at startup, then hold Control while choosing it;
  to go back, pick macOS in Startup Disk. macOS upgrades can reset this. Apple
  firmware largely ignores `efibootmgr` entries, so Omarchy's ESP should also
  carry `EFI/BOOT/BOOTX64.EFI`, kept up to date by Omarchy.
- **Out of scope at first**: Fusion Drive, RAID, multiple physical stores,
  hybrid-MBR Boot Camp, 32-bit EFI Macs, and macOS older than 10.13.6.

## Later

- Linux: shrinking single-device btrfs online, saving one reboot.
- Linux: `kexec` from the maintenance environment into the installer, saving
  the second reboot. It skips firmware initialisation right after disk changes,
  so it needs its own qualification.
- Linux: an upstream-built maintenance image with each ISO release.
- Macs: automatic one-time boot with `bless --setBoot --nextonly`, only on
  combinations that pass testing (SIP blocks it in some setups).
- Macs: copying the Wi-Fi and Bluetooth firmware that T2 Macs need from macOS
  onto the temporary ESP, locally for that machine only, never bundled or
  uploaded, with an import step in the installer. Check Apple's licence terms
  first.

## What upstream needs

1. A release containing omarchy-iso#187 (installing into free space while
   protecting the installer source and ESPs), qualified against the real ISO.
2. On Intel Macs: `EFI/BOOT/BOOTX64.EFI` on Omarchy's ESP, maintained across
   updates.

Shrinking, unlocking, staging, boot scheduling and cleanup stay in the app.

## Verify first, in order

1. Complete installations on 4 GB RAM machines, measuring peak memory during
   LUKS unlocking, filesystem checks, staging and the installer.
2. The maintenance image's size, against the ESPs of representative machines.
3. Interrupted operations: inject failures around shrinking, LVM changes,
   partition-table writes and staging; show recovery without blind replays.
4. Common layouts end to end: Ubuntu/Mint LUKS + LVM + ext4, Fedora LUKS +
   btrfs, encrypted APFS, including the return boot, replacement and cleanup.
5. Real firmware on several vendors and Mac models: `BootNext`, Option and
   Control selection, macOS return, the app on macOS 10.13.
6. The released ISO: ext4 and exFAT sources, source protection, 512-byte and
   4K-sector disks, removal after Omarchy boots on its own.
