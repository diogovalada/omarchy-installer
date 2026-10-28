# Starting Windows after a staged install, with BitLocker on

On the user's laptop (ASUS Zenbook, Micron 512 GB NVMe, AMI-style firmware boot
menu), the testing build prepared a temporary installer. The official installer
then put Omarchy next to Windows, and Omarchy started on its own through Limine.
This is the first full real-hardware run of Install without USB.

## The assumption that doesn't hold

The plan was: add Windows to Limine's menu with `limine-scan` (what Omarchy's
dual-boot guide does), start Windows from Limine once, resume BitLocker there so
the TPM reseals to that route, and use Limine as the everyday menu.

With Secure Boot off, which the official ISO needs, BitLocker's TPM protector
uses the native-UEFI default PCRs 0, 2, 4 and 11. PCR 4 measures every boot
application that runs. `limine-scan` reads the firmware's entries with
`efibootmgr`, and for the one picked writes `protocol: efi` with a
`uuid(...)`/`guid(...)` path to `bootmgfw.efi`, a chainload
([`EfiScanner`](https://gitlab.com/Zesko/limine-entry-tool/-/blob/master/src/main/java/org/limine/entry/tool/processes/EfiScanner.java),
[`LimineManager.addEfi`](https://gitlab.com/Zesko/limine-entry-tool/-/blob/master/src/main/java/org/limine/entry/tool/processes/LimineManager.java)).
Limine's own binary then becomes part of Windows' measurements. Omarchy updates
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

  Limine resolves the entry, writes `BootNext` and does a warm reset. It
  doesn't chainload. A warm reset starts a fresh measurement sequence, so
  Windows sees the same PCRs as a direct firmware start, and Limine updates
  can't affect it. The cost is one extra restart. Sources:
  [Limine `efi_boot_entry`](https://github.com/Limine-Bootloader/Limine/blob/trunk/common/protos/efi_boot_entry.c),
  [Limine CONFIG.md](https://github.com/Limine-Bootloader/Limine/blob/trunk/CONFIG.md),
  and [BitLocker research](bitlocker-direct-boot-research-2026-09-06.md).

Both ways give Windows the same startup record, so either can serve as the
fallback for the other. Resume BitLocker only after starting Windows that way.

**Status (2026-10-27):** confirmed on the laptop. With BitLocker protection on
(TPM and recovery password protectors, resealed after a direct firmware start),
Windows started from the Limine entry above without asking for the recovery key.
A Limine update hasn't been observed with either kind of entry yet; by
construction it can't affect this one, since Limine doesn't run in that start.

## Choosing the kind of Windows entry

| | Chainload (`limine-scan` today) | `efi_boot_entry` | Pick per PC |
|---|---|---|---|
| BitLocker on, Secure Boot off | Recovery key after Limine updates | Works | Works |
| Firmware lost its entry | Works | Fails; boot menu key needed | Works |
| Speed | Direct | One extra restart, a few seconds | Depends |

From Limine's source, `efi_boot_entry` matches `entry` against the whole
description of a firmware entry, ignoring ASCII case. It only searches entries
in `BootOrder`, takes the first match, and stops with "efi_boot_entry: Failed
to find boot entry" when none matches. Firmware entries live in the motherboard,
so a firmware update, "load defaults" or moving the disk to another PC can
remove them while Windows is still installed. A chainload reads the file from
the disk and keeps working then.

"Pick per PC" therefore writes `efi_boot_entry` when Windows Boot Manager is in
`BootOrder` under a unique description, and chainloads otherwise. The decision
can't depend on BitLocker being on at that moment: Windows 11 can turn device
encryption on later. `limine-scan` lets you rename an entry; the `entry:` line
must keep the firmware's original description.

This is a change for whatever writes the entry, not for this app, which doesn't
edit Limine's menu. Omarchy chooses and wires these tools (Limine and
`limine-entry-tool` are separate projects), so the plan is an issue at
omacom/omarchy presenting the options (document the entry, have Omarchy add it
per PC, or ask `limine-entry-tool` for an option), and a comment on
omarchy#7867 (below) asking that fixes keep entries of every kind. An issue at
`limine-entry-tool` follows only if Omarchy prefers the tool to change. The app's last step recommends the `efi_boot_entry`
entry, with `limine-scan` when the PC has no Windows Boot Manager entry.

## Omarchy behaviour that affects the Windows entry

- **`omarchy-refresh-limine` drops every Windows entry**
  ([omarchy#7867](https://github.com/omacom/omarchy/issues/7867), confirmed on
  2026-10-08). It rebuilds `/boot/limine.conf` from a template without entries;
  `omarchy reinstall` runs it too. The entry has to be added again afterwards.
  The fix waits on a maintainer decision about which entries a rewrite keeps.
- **Omarchy 4.5 hides Limine's menu**
  ([omarchy#16367](https://github.com/omacom/omarchy/issues/16367), open). Its
  migration sets `timeout: 1` and `quiet: yes`, so the menu only appears when a
  key is pressed in the first second. Any Windows entry is unreachable without
  knowing that.

## The laptop after installing

Partitions in disk order: Windows' EFI partition (0.25 GB), Microsoft reserved,
C: (404 GB), the temporary EFI (0.5 GB) and source (6.78 GB) partitions,
**Omarchy's own EFI partition (2 GB)** and Omarchy's LUKS system (62 GB), then
Windows' recovery partitions. Omarchy's installer made its own EFI partition and
left Windows' alone. Limine is registered as `\EFI\limine\limine_x64.efi` on it.

Firmware boot order: Limine, Windows Boot Manager, then two "UEFI OS" entries.
The firmware creates those itself for EFI partitions with a loader at
`\EFI\BOOT\BOOTX64.EFI` and no named entry. The app's "Omarchy temporary
installer" entry, kept out of `BootOrder` by design, wasn't in the F-key menu,
and one "UEFI OS" pointed at the same partition and file. So this firmware lists
only `BootOrder` entries, plus the ones it discovers. After the temporary
installer was removed through the app, both "UEFI OS" entries were gone:
`BootOrder` was Limine, Windows Boot Manager and three generic device entries.

## The blue recovery screen

Once, choosing Windows Boot Manager from the F-key menu after restarting from
Omarchy opened Windows' recovery menu ("Choose an option") instead of Windows.
"Continue" restarted, and the restart went to Limine, the default. The next try
started Windows directly.

Nothing in Windows explains it. The previous Windows session had ended with a
clean restart by `shutdown.exe` (the app's "Restart into the installer"), and the
next Windows start succeeded. There was no unexpected shutdown (events 41 and
6008), Startup Repair never ran (no `SrtTrail.txt`), no one-time recovery start
was pending, and Windows Boot Manager has no `customactions`, so an OEM recovery
key (ASUS uses F9, which was being pressed to reach the menu) isn't mapped
there. Whether the firmware handles F9 itself is untested. It is recorded as an
unexplained one-off.

## The BitLocker reminder's "Open BitLocker settings"

On 25 October the reminder offered "Open BitLocker settings" instead of Resume
for a fully encrypted, paused C:, and Windows' BitLocker page only offered "Back
up your recovery key". Most likely the reminder dated from the first laptop run:

1. That run paused BitLocker and registered a reminder with its build's script.
2. C: stayed paused until `manage-bde -protectors -enable C:` on 25 October.
3. The next run found C: already paused, so it neither paused it nor registered
   a new reminder.
4. Scripts before 4f872ee showed "Open BitLocker settings" whenever an installer
   was waiting, and the app still listed one as prepared: its partitions had
   been deleted by hand.

The other explanation, a pause whose receipt was never written (`armed`), can't
be ruled out: the run after that registered a fresh reminder in the same place,
overwriting the record (`createdBootUtc` 2026-10-25T16:49Z, later `restored`).
PR #44 makes the reminder resume an `armed` pause either way, and every later run
leaves its status in `state.json`.

A reminder keeps the copy of the script it was registered with, so app updates
don't reach existing reminders. That is accepted, not fixed. It only matters
when a reminder is still active across an app update, an outdated reminder at
worst offers a less helpful button and retires once protection is back on, and
refreshing means touching an elevated sign-in script's integrity checks. Revisit
if a reminder fix is ever about safety.

## Other observations

- Disk Management and Settings don't offer Delete for the temporary source
  partition. It uses the Windows Recovery type on purpose, so that device
  encryption leaves it alone. Remove it through the app.

## Follow-ups

- Post the Omarchy issue and the omarchy#7867 comment.
- Revisit when Omarchy boots with Secure Boot on. BitLocker can then bind to
  PCR 7 (Secure Boot policy and signers) instead of binary hashes. Whether a
  third-party-signed loader in front of Windows keeps that binding needs
  checking at that point.
