# What users should expect on each platform

Last updated: 2026-09-06. This is the shared reference for product copy, support,
onboarding, training and the wider team. It describes implemented development
behavior, not a hardware support or release qualification claim.

The [current priority](direct-install-priority-2026-09-06.md) is local construction
and native deployment with first-boot owner setup. Windows behavior below
describes that existing development path. Staged-ISO installation remains an
unimplemented secondary option and does not automatically inherit this behavior.

## Startup and OS choice

| Experience | Windows x64 PC, direct install | Apple Silicon Mac, direct install | Linux host / Intel Mac |
| --- | --- | --- | --- |
| OS chooser | Omarchy / Windows menu at each normal startup | Apple's startup picker, opened by shutting down and holding the power button | Direct installation is not implemented |
| Default | Omarchy, unless the user selects Windows in our installer | The startup disk configured through Apple's controls | Not applicable |
| Countdown | Five seconds initially; installer offers 5, 10, 15 or 30 seconds | No PC-style automatic menu/countdown is configured | Not applicable |
| Choose the other OS once | Stop the countdown with the keyboard and select an OS; the saved default stays unchanged | Select the other OS in Apple's startup picker | Not applicable |
| Starting Windows | Selecting Windows causes a brief additional restart, then boots its existing firmware entry | Not part of this Apple Silicon flow | Not applicable |
| First Omarchy start | Hardware setup, storage growth, account setup and replacement of the temporary encryption key | Follow the native Finish Installation / Recovery authorization instructions | Not applicable |
| Later settings changes | Edit the installed startup settings and regenerate the menu; see below | Use Apple's Startup Disk / startup picker controls | Not applicable |

On Windows PCs, the menu becomes the first firmware boot entry. Existing entries,
including Windows Boot Manager, remain available in the firmware's own menu.
Our app does not reboot the computer automatically after installation.

“Every startup” means the normal boot route. A firmware-selected device, a
one-time boot selection, Windows maintenance/update boots, or firmware that resets
its boot order can bypass it. A scheduled one-time firmware boot at installation
time blocks this flow until that startup has completed. Do not promise that the
installer overrides these platform behaviors.

The Windows menu uses Limine's `efi_boot_entry` mechanism: Windows starts through
its original firmware entry after a warm restart. This avoids adding a direct
Linux-to-Windows chainload to the Windows boot path. It is not a guarantee that
every TPM/BitLocker/firmware combination will avoid a recovery-key prompt; those
combinations remain part of qualification. Keep the Windows recovery key available
when changing boot configuration.

## Storage, encryption and removal

| Operation | Windows direct flow | Apple Silicon direct flow |
| --- | --- | --- |
| Make room | Select unallocated space, shrink an eligible NTFS partition, or explicitly delete an eligible unused partition | Native backend selects suitable free space or a supported macOS resize |
| Choose size | Within native inspected limits | Alongside size request is checked by the native backend; replacement uses the entire detected installation |
| Replace existing storage | Typed `Disk N Partition M` confirmation; entire selected partition is deleted | Typed `Installation N` confirmation; replaces a native-detected Omarchy installation |
| Delete the running host OS | Unavailable: running Windows and system/recovery storage are protected | Unavailable: macOS and essential Apple boot/Recovery storage are retained |
| Foreign Linux partitions | Eligible unused ext4/Btrfs partitions may be deleted; resize them in Linux first if retaining their contents | Arbitrary foreign-partition deletion and NTFS/ext4/Btrfs resizing are not exposed |
| Encryption | New Omarchy uses per-install LUKS2; first boot establishes the owner's password. Existing Windows BitLocker data stays encrypted, with bounded protection suspension/restoration where required | The pinned direct-install package deploys an unencrypted Btrfs root. First-boot account setup does not encrypt it; see the [source and artifact inspection](evidence/apple-direct-encryption-2026-09-06.md). |
| Recovery | Preserve the operation journal and Windows recovery information after a failed or interrupted install | Preserve and follow native Recovery instructions and journals |

Deleting Windows completely needs a different boot environment. A USB-free
second-stage “Remove Windows after Omarchy boots successfully” feature is not
implemented. Likewise, Mac “replace” means replace an existing Omarchy install;
it does not mean erase macOS. Asahi recommends keeping internal macOS for
installation, bootloader maintenance and resizing.

Creating an x86 installation USB is a separate workflow. These direct-install
boot menu defaults do not change the USB image or configure the USB's destination
machine. An x86 USB made on a Mac does not install Omarchy on Apple Silicon.

## Changing the PC menu after installation

The installed `/etc/omarchy-boot-menu.json` stores only these preferences:

```json
{"defaultOs":"omarchy","timeoutSeconds":5}
```

As an administrator inside the installed Omarchy system, edit that file to use
`omarchy` or `windows` and a timeout of `5`, `10`, `15` or `30`, then run
`sudo limine-update`. The installed post-hook regenerates and verifies the menu.
This is a configuration-file workflow; an Omarchy settings GUI is not implemented.

The upstream kernel/snapshot tree is retained under **Advanced Omarchy options**.
The primary Omarchy entry follows the first normal upstream kernel in that tree.
Do not rename that managed group or change its `TARGET_OS_NAME` binding when only
adjusting the startup default. The hook runs before config enrollment, preserves
upstream kernel command lines and hashes, and keeps `remember_last_entry` disabled.
An identity pre-hook binds that group to the current machine ID before upstream
updates run, including the first hardware boot and a factory reset. Both hooks
verify the mounted EFI partition against the installed partition/filesystem IDs.

## Communication and release checklist

- Before Windows preparation: show the default OS, timeout and the extra restart
  when selecting Windows. Repeat the chosen behavior in native plan approval.
- Before Mac preparation: explain the power-button startup picker, retained macOS
  and required Recovery completion. Do not show Windows boot-menu controls.
- At completion: explain the next actual boot step, not just “installation done.”
- In support and team training: distinguish host OS, image architecture, creation
  of USB media, installed OS, and startup picker. “Cross-platform frontend” does
  not imply identical firmware, encryption or storage operations.
- Before release: qualify Windows and Omarchy defaults, each timeout, keyboard
  cancellation, subsequent starts after selecting Windows, kernel/snapshot updates,
  first-boot owner rekey, BitLocker combinations, firmware-order recovery and Mac
  Recovery/startup behavior. Check the visible copy against this document.

## References and present limitations

The implementation uses the official ISO's Limine **12.6.0-1** and
limine-mkinitcpio-hook **1.37.1-1** packages. Their hashes are added to the inspected
source lock. New boot execution tests remain deferred at the user's request.
macOS compilation/signing and hardware qualification remain pending.

- [Apple startup disk and startup picker](https://support.apple.com/en-euro/guide/mac-help/mchlp1034/mac)
- [Asahi installation and maintenance requirements](https://asahilinux.org/docs/project/faq/)
- [Limine configuration](https://github.com/Limine-Bootloader/Limine/blob/v12.x/CONFIG.md)
- [Limine firmware-entry boot implementation](https://github.com/Limine-Bootloader/Limine/blob/v12.x/common/protos/efi_boot_entry.c)

The packaged documentation was inspected directly from the pinned ISO; moving
upstream documentation links are explanatory, not the build's trust anchors.
