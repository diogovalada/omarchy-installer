# Staged official ISO provider (Windows x64)

Normal builds keep no-USB installation disabled, and `qualified-releases.json`
deliberately contains no releases. Explicit local testing builds enable real
staging, boot selection and cleanup without claiming upstream qualification.
This is not a boot-qualified distribution. No patched Omarchy ISO is shipped.

## Flow

The authenticated desktop helper owns plan confirmation and invokes the packaged
PowerShell provider from a protected operation directory. The provider implements
inspection, planning, staging, one-time boot selection, status and cleanup.
There is no desktop runtime switch or user-supplied release entry that enables
testing mode. The Rust `staged-iso-testing` feature is required at compile time.

## Local testing build

On Windows, build with:

```powershell
$env:OMARCHY_DISTRIBUTION = 'usb-preview'
./scripts/build-windows-portable.ps1 -UnsignedPreview -DebugBuild -StagedIsoTesting
```

This produces a distinctly named `*-testing.exe`. Omitting `-StagedIsoTesting`
keeps normal builds closed, including ordinary debug builds. The packager checks
the executable's compiled mode with its read-only `--build-info` command; release
collection rejects testing executables. Release CI refuses the testing switch.

Testing mode still authenticates the official ISO in the privileged helper and
holds its source open. Staging rechecks its hash and file inventory. An unqualified
ISO must contain one unambiguous kernel/initramfs pair; boot paths are discovered
from those original files before disk allocation. All disk ownership, encryption,
free-space and confirmation checks remain. The UI and preparation confirmation
warn that booting and same-disk installation may fail in the official ISO.

1. Select existing free space or explicitly review an NTFS shrink. Existing
   Windows sizing/reserve checks are reused; no existing partition is deleted.
   The target must be a basic GPT disk with 512-byte logical sectors, native
   x64 UEFI and Secure Boot off. Inspection does not require BitLocker suspension.
   Planning accepts unlocked, fully encrypted or decrypted volumes; incomplete
   conversion and unknown status block preparation.
2. Allocate only a 512 MiB FAT32 EFI partition and an NTFS source partition
   sized to the ISO plus 1 GiB. Windows does not reserve or create Omarchy's
   destination. Inspection shows the largest unallocated region that will remain
   after staging. If it is below the official installer's 32 GiB minimum, the
   user sees a warning and can make room in the booted installer by deleting an
   unneeded partition, once upstream same-disk editor support is available.
3. Hold the source against replacement, hash it against the qualified release,
   mount it read-only and inventory its files. Reject unsafe paths, missing boot
   files and an extraction that exceeds capacity before allocating partitions.
   After confirmation and source checks, suspend active protection on the reviewed
   Windows/target volumes, without decrypting. Register a per-volume sign-in
   reminder first; existing suspensions are not adopted. OS suspension is
   indefinite (DisableCount=0); data volumes omit DisableCount. The user explicitly
   resumes through the reminder after Windows boots through the final boot path.
   Copy the original ISO files to NTFS with file-level hash readback; no live
   filesystem or installer code is patched. NTFS supports the large squashfs file.
4. Copy the existing packaged GRUB loader to the new FAT32 partition. Its config
   locates an operation-specific source marker and supplies
   `archisodevice=/dev/disk/by-partuuid/<source GUID>` and `copytoram=n`.
   The live source is a directly mounted partition, not a loopback ISO.
5. Register a temporary `Boot####` entry without changing `BootOrder`. A separate
   user confirmation revalidates the files, owned partitions, encryption and firmware
   target, then sets `BootNext`. The app does not reboot automatically. Restart
   Windows to enter the official interactive installer, which owns Linux setup,
   its encryption and its permanent boot menu.
6. On returning to Windows, an existing temporary installer appears automatically.
   Use **Remove temporary installer**, or **Prepare again** in a testing build
   with a verified download. Preparation again performs reviewed cleanup first,
   then opens fresh space selection; it never repeats an old allocation silently.
   Cleanup remains accessible without the ISO download and requires explicit
   confirmation after independently booting installed Omarchy, or abandoning
   installation. It does not infer Linux success from a Windows boot.

No Docker, WSL or prepared-filesystem construction is used by this provider.
The prepared-filesystem implementation remains separately retained and disabled.

## Ownership and interruptions

Before allocation, the protected record at
`%ProgramData%\OmarchyStagedInstaller\<operation UUID>\state.json` records disk
identity, geometry, fresh partition GUIDs, exact extents, the source hash and
Windows boot evidence. A shared deployment mutex serializes our own operations.
States distinguish preparation, allocation, copy, firmware registration, staged
files, boot scheduling and cleanup. Failed staging is not blindly resumed: its
record supports inspection and reviewed cleanup, followed by a fresh operation.
Unreadable or missing records are reported individually; they do not hide other
valid operations. They cannot authorize cleanup or boot selection.
After each saved state, an administrator-written `summary.json` grants users read
access to minimal display information. The desktop reads this bounded, non-authoritative
index without elevation. Every action still uses the protected full ownership
record. No empty checker or automatic elevation appears on the home screen.

EFI artifacts are written and verified in an operation-specific scratch directory
outside `EFI`, then renamed to their final paths on the same volume without
overwriting existing files. Interrupted initial writes therefore remain eligible
for cleanup without relaxing the checks on published boot files.

Cleanup resolves the disk by identity rather than its old number, validates both
owned partitions before changing either, and refuses changed geometry, types,
running Windows partitions or the recorded Windows ESP. Additional/replaced EFI
files block cleanup because that partition might now boot an installed OS.
Native deletion locks and checks the volume extent, removes only the exact
recorded GPT entry, and verifies that the other entries were preserved. Missing
owned GUIDs can be skipped during an interrupted cleanup; a replacement at the
same offset or partition number is never adopted. Completed cleanup is idempotent.

Firmware cleanup checks the exact saved load option, refuses a reused slot or
the current boot source, and only clears our own BootNext/order references.
It preserves unrelated boot entries, including the new permanent Linux entry.
Owned storage is deleted only after firmware cleanup succeeds. Recovery records
remain; reclaimed space is unallocated and Windows is not automatically expanded.

Windows' Storage `Get-Volume` association omits ESPs on the inspected host.
The provider therefore resolves the partition's volume GUID using `Win32_Volume`,
formats only the fresh RAW volumes, and uses verified volume mount-point APIs.
`IsSystem` alone cannot exclude our own ESP; the recorded Windows ESP, current
OS partition, exact ownership and EFI-file checks provide the relevant bounds.

## Qualification required before enabling

A maintainer must inspect a released official ISO containing the protection in
upstream PR #187, and test its actual initramfs mounting NTFS by PARTUUID. A merged
PR or a higher version number alone is insufficient. Each catalog entry needs:

```json
{
  "version": "<official version>",
  "sha256": "<full official ISO SHA-256>",
  "sizeBytes": 0,
  "kernelPath": "arch/boot/x86_64/<qualified kernel>",
  "initrdPath": "arch/boot/x86_64/<qualified initramfs>",
  "minimumLinuxBytes": 34359738368,
  "sourceProtection": "direct-gpt-partition",
  "ntfsSource": true,
  "bootQualified": true
}
```

This is a schema illustration, not a usable qualification entry. Record evidence
for allocation/shrink, ESP formatting/mounting, NTFS live-source boot, source
preservation, installation, independent Linux boot, Windows return, cleanup,
interrupted steps and retry behavior. Test on disposable disks/VMs and then
representative firmware. Include BitLocker restoration after the final boot path
is established; no claim of universally keyless Windows startup is made.
Then change the explicit desktop/helper release gate and keep the catalog
matching the inspected artifact. Do not loosen the gate just to make tests pass.

Current validation covers native compilation, synthetic GPT/EFI structures,
policy tests, actual disposable file copying, mocked cleanup, packaging closure
and frontend behavior. It does not demonstrate actual Windows partition writes,
firmware mutation or a complete installed-system boot. A read-only host query
was used to check ESP enumeration; the host's disks, encryption and boot settings
were not changed.

## References

- [UEFI Boot Manager and BootNext](https://uefi.org/specs/UEFI/2.11/03_Boot_Manager.html)
- [ArchISO source mounting](https://github.com/archlinux/mkinitcpio-archiso/blob/master/hooks/archiso)
- [Windows partition identities and access paths](https://learn.microsoft.com/en-us/windows-hardware/drivers/storage/msft-partition)
- [Win32_Volume formatting](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/vdswmi/format-method-in-class-win32-volume)
