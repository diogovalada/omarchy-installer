# Starting Windows after a staged install, with BitLocker on

On the user's laptop (Micron 512 GB NVMe, AMI-style firmware boot menu), the
testing build prepared a temporary installer. The official installer then put
Omarchy next to Windows, and Omarchy started on its own through Limine. This is
the first full real-hardware run of Install without USB.

## The assumption that doesn't hold

The plan was: add Windows to Limine's menu with `limine-scan` (what Omarchy's
dual-boot guide does), start Windows from Limine once, resume BitLocker there so
the TPM reseals to that route, and use Limine as the everyday menu.

With Secure Boot off, which the official ISO needs, BitLocker's TPM protector
uses the native-UEFI default PCRs 0, 2, 4 and 11. PCR 4 measures every boot
application that runs. `limine-scan`'s entry chainloads `bootmgfw.efi` by path,
so Limine's own binary becomes part of Windows' measurements. Omarchy updates
Limine, so after any Limine update the next protected Windows start asks for the
recovery key. The direct-install brainstorm already listed "Omarchy currently
updates its Limine binaries" as a risk, but the staged route's guidance didn't
apply it.

## What works instead

Start Windows Boot Manager from the firmware, so Limine never runs before it:

- the firmware boot menu key, choosing **Windows Boot Manager**; or
- a Limine entry that hands over to that firmware entry:

  ```
  /Windows
      protocol: efi_boot_entry
      entry: Windows Boot Manager
  ```

  Limine v12.6.0 resolves the entry, writes `BootNext` and does a warm reset.
  It doesn't chainload. A warm reset starts a fresh measurement sequence, so
  Windows sees the same PCRs as a direct firmware start, and Limine updates
  can't affect it. The cost is one extra restart. Sources:
  [Limine `efi_boot_entry`](https://github.com/Limine-Bootloader/Limine/blob/v12.6.0/common/protos/efi_boot_entry.c),
  [Limine CONFIG.md](https://github.com/Limine-Bootloader/Limine/blob/trunk/CONFIG.md),
  and [BitLocker research](bitlocker-direct-boot-research-2026-09-06.md).

Both ways give Windows the same startup record, so either can serve as the
fallback for the other. Resume BitLocker only after starting Windows that way.

**Status:** inferred from the sources above. Not yet tested on the laptop. The
next check is a protected Windows start through the Limine entry, then another
after a Limine update.

## Other observations from this run

- After installing, the firmware menu listed Windows Boot Manager, Limine and
  two "UEFI OS" entries. The firmware creates those for any EFI partition with
  a loader at `\EFI\BOOT\BOOTX64.EFI` and no named entry. One is most likely the
  temporary installer's EFI partition, and the other Limine's fallback copy;
  `efibootmgr -v` output would confirm it. The app's own "Omarchy temporary
  installer" entry is kept out of `BootOrder` and wasn't listed by name, so this
  firmware seems to list only `BootOrder` entries plus the ones it discovers.
- The BitLocker reminder offered "Open BitLocker settings" instead of Resume
  for a fully encrypted, paused C:. Windows' BitLocker page there only offered
  "Back up your recovery key". There are two explanations, and the record's
  `createdBootUtc` and `scriptSha256` tell them apart:
  - The record was still `armed`: the pause happened, but its receipt wasn't
    written. PR #44 makes the reminder resume such a pause.
  - The reminder dated from the earlier laptop run. Each reminder runs the copy
    of the script made when it was registered. A run that finds C: already
    paused registers nothing new, so the old reminder stays. Scripts before
    4f872ee showed "Open BitLocker settings" whenever an installer was waiting,
    because the label only changed to Resume when resuming was allowed.
  The second explanation points to a gap: updating the app never updates an
  existing reminder.
- Disk Management and Settings don't offer Delete for the temporary source
  partition. It uses the Windows Recovery type on purpose, so that device
  encryption leaves it alone. Remove it through the app.

## Follow-ups

- The app's last step (`StagedSteps.svelte`) tells people to run `limine-scan`.
  Once the `efi_boot_entry` route is confirmed on the laptop, recommend that
  route instead for anyone keeping BitLocker on.
- Consider refreshing an existing reminder's script when the app finds its
  pause still in place, so fixes reach reminders from older builds.
- Revisit when Omarchy boots with Secure Boot on. BitLocker can then bind to
  PCR 7 (Secure Boot policy and signers) instead of binary hashes. Whether a
  third-party-signed loader in front of Windows keeps that binding needs
  checking at that point.
