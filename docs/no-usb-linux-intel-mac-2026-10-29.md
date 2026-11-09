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

**Maintenance boot.** A small Linux environment does all the shrinking (see
[the alternatives to evaluate first](#to-evaluate-before-building-the-maintenance-image)):

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

## To evaluate before building the maintenance image

Not decided. These are notes from looking at how
[nixos-anywhere](https://github.com/nix-community/nixos-anywhere) installs NixOS
over SSH (2026-11-04). The question they raise is how to get off the system
disk so it can be shrunk. There are three ways:

| | Image on the ESP and a firmware restart (current plan) | `kexec` into an image in RAM | Shrinking during shutdown |
| --- | --- | --- | --- |
| Own kernel to maintain | Yes | Yes | No |
| Space on the ESP | 100–300 MiB | None | None |
| Firmware entries | One temporary | None | None |
| Screen and keyboard | Normal boot | Riskiest: firmware doesn't set the hardware up again | Expected to keep working: same kernel, still running |
| LUKS password | Typed at a text prompt | Typed at a text prompt | Expected to stay unlocked; if needed, asked in the app first and kept in RAM only |
| Needs systemd | No | No | Yes |

**What nixos-anywhere does.** It uses `kexec` to jump from the running Linux
into a NixOS installer in RAM, with no firmware restart. Then `disko` destroys
and recreates the filesystems, and it installs. It needs
[1–1.5 GB RAM](https://github.com/nix-community/nixos-anywhere/blob/main/docs/requirements.md)
(its docs disagree) and
[Secure Boot off](https://github.com/nix-community/nixos-images). What's
relevant here:

- **`kexec`** needs nothing on the ESP and no firmware entry, which would remove
  the small-ESP exclusion. But nixos-anywhere drives machines over SSH and
  doesn't support Wi-Fi, so nothing depends on the screen, while a laptop has to
  show progress and may need a password. Its docs don't cover hangs. Nothing
  has been written to the disk at that point, so powering off should bring the
  old system back.
- **Carrying data into the RAM environment:** it carries SSH keys and network
  settings over from the old system. Ours would carry the reviewed plan.
- **Wiping from RAM, then downloading the system:** a possible way to close the
  gap of replacing a Linux that can't shrink. A failed download would leave no
  OS, and Wi-Fi settings would have to come along.
- **`disko`** isn't useful here: it creates and formats, nothing we read says it
  shrinks, and Omarchy's installer does its own partitioning.

**Shrinking during shutdown.** If `/run/initramfs/shutdown` is executable,
[systemd](https://systemd.io/INITRD_INTERFACE/) switches to that RAM environment
at shutdown, with the old root still mounted at `/oldroot`, and
[runs it](https://github.com/systemd/systemd/blob/main/src/shutdown/shutdown.c).
The hook exists to detach complex storage. Ours would unmount `/oldroot`, check
and shrink each layer with bundled static tools, and restart. Linux then starts
normally with free space, and the app continues as it does when free space
already exists, so the RAM environment only shrinks and preparing the installer
stays in the app. It's still two restarts. If the old root can't be unmounted,
nothing has changed and it just restarts.

What could break it:

1. We found no precedent for shrinking through this hook, only
   [older](https://www.ivarch.com/blogs/oss/2007/01/resize-a-live-root-fs-a-howto.shtml)
   [guides](https://prosauce.org/blog/2018/7/23/online-filesystem-or-disk-changes-on-embedded-hardware)
   that move the system into RAM by hand.
2. systemd leaves the hardware watchdog armed at this stage (10 minutes by
   default, to confirm). A long shrink has to turn it off, or the machine could
   reset in the middle of a resize.
3. Fedora's dracut and Arch's mkinitcpio use the same path (to confirm), so it
   has to coexist with them.
4. Text may not reliably show at that stage (Plymouth, NVIDIA). A blank screen
   during a long shrink would look like a hang.
5. Processes can keep the old root busy, and a separate encrypted `/home` is
   already closed by then.
6. Distros without systemd would still need the image on the ESP, or decline.

`systemctl soft-reboot` with `/run/nextroot/` is a similar mechanism that also
keeps the kernel running, but it needs a new root with a service manager.

First test: a VM with Ubuntu on LUKS, LVM and ext4, shrinking during shutdown
and checking that it boots back. Then an independent review before changing the
design.

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
   updates. Limine's `ENABLE_LIMINE_FALLBACK` setting does this, but the fork
   turns it off for free-space installs (`enable_fallback` in
   `phases_impl.py`). A
   [report from a 2015 MacBook Pro](https://github.com/omacom/omarchy/issues/7867#issuecomment-5397153472)
   confirms the Option-key picker follows that file, not `BootOrder`.

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
