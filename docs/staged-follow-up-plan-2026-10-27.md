# Install without USB: the follow-up after installing

After the first full laptop run (see
[the run's notes](evidence/windows-after-staged-install-2026-10-27.md)), two jobs
were left after installing, in two places: removing the temporary installer
(only in the app, which you had to open yourself) and resuming BitLocker (the
sign-in reminder). This plan puts both in the app, which reopens until they're
done, and makes removal give the space back.

## Decisions

- **Remove temporary installer extends C:** over the free space next to it,
  whether or not Omarchy was installed. Someone who removes the installer is
  done with it. **Prepare again** keeps the space free, because the new
  installer reuses it at once; extending first would mean measuring and
  shrinking again. No option or question: each button does one thing.
- **Only the partition the installer took space from grows**, and only when
  it ends where the temporary installer starts. When the space came from free
  space instead, it stays unallocated and the message says so.
- **The new size comes from the layout:** up to the next partition, or the
  disk's end. `Get-PartitionSupportedSize` is the slow call behind the
  several-minute shrink measurement, so it isn't used.
- **The app reopens at every sign-in while a temporary installer exists,**
  from the end of preparing until it is removed, unless you choose **Don't open
  at sign-in** on the card. BitLocker alone stays the reminder's job: without
  administrator access, the app can't read the pause's state.
- **A reopened start doesn't ask for administrator access;** an action does,
  when clicked. A prompt at every sign-in looks alarming and trains people to
  accept it.
- **No two windows:** the reminder waits a minute after sign-in and stays quiet
  while the app shows a temporary installer that paused BitLocker, so the card
  can offer what the reminder would. The app leaves a named event for that; if
  the app didn't open, or its installer paused nothing (Prepare again after a
  pause), the reminder shows as before.
- **Resume BitLocker from the app,** on the card once the installer has
  started, and after removal. Same rules as the reminder (only a pause this
  installer made, never while an installer waits) through the same function.
- **When removal is the main action:** unchanged. Once the installer has
  started, the card asks "Does your PC now start Omarchy by itself?" and offers
  removal first. Detecting Omarchy isn't needed: removal always extends, and a
  half-finished install would fool the detection.

## Parts

1. **Removal extends C:**
   - Provider: `Get-StagingReclaim` finds the recorded shrunk partition by GUID
     on the identified disk, checks its offset and that nothing sits between
     it and the temporary EFI partition, and returns its current and new size.
     `Invoke-StagingCleanup` takes `reclaim` and, after deleting both
     partitions, resizes it with `Resize-Partition` and verifies the size. A
     failed resize keeps the removal and says the space stays unallocated.
   - Helper: `again` is its own action, mapped to cleanup without `reclaim`.
     Removal's confirmation says how far the partition grows, from a read-only
     `reclaim` query; Prepare again's says the space stays free for the new
     installer.
2. **Resume BitLocker from the app:**
   - Provider: `resume-bitlocker` loads the operation's paused volumes and runs
     `Resume-OwnedProtection` for each, with the waiting check; the record
     notes the result, and the summary lists volumes still paused.
   - Card: **Resume BitLocker** once the installer has started, and after
     removal while the result lists a paused volume. Its confirmation says to
     start Windows the usual way first, and to turn Secure Boot back on first
     if you will.
3. **Reopen at sign-in:**
   - Desktop: while the recovery summary lists an unremoved installer that
     isn't opted out, keep a Run entry (written once per change, never from
     the run it starts); while one that paused BitLocker is shown, hold the
     named event. The firmware restart adds its one-time RunOnce entry only
     when the Run entry isn't in place, so the app never starts twice. The card
     offers the opt-out only when the entry is really registered.
   - A `--reopened` start skips the launch-time administrator request.
   - Reminder: the logon trigger gets a one-minute delay, and the reminder
     returns without showing while the event exists.

## Tests

PowerShell tests for the reclaim calculation (shrunk partition next to the
installer, Omarchy after it or not, free-space target, a partition in between),
the cleanup's resize order and failure, and resuming per operation. Rust tests
for the new actions and summaries. Frontend tests for the card's buttons and
the reopened start. Then one VM run of the stack on `OmarchyStagedTest`
(BitLocker on): prepare, start the installer, remove, check C: and BitLocker.

## Not doing

- Detecting whether Omarchy was installed.
- Refreshing old reminders' scripts (see the run's notes).
- Editing Limine's menu from the app; boot management and removing Omarchy stay
  in future work.

## VM run (2026-10-28)

On `OmarchyStagedTest` (BitLocker on), built from `main` @ 003570a, every check
passed. The run's report is in `staged-vm-handoff/vm/follow-up-run/`.

- **Source partition:** 6178 MiB, the ISO's 5921.9 MiB plus 256 MiB.
- **Reopening:** both reopened starts asked for no administrator access at
  launch; the first action did. The firmware restart added no RunOnce entry
  while the Run entry was in place, and only one copy of the app ever ran.
- **The reminder:** it ran about a minute after each sign-in and stayed quiet
  while the app's card covered C:. After **Don't open at sign-in** it showed,
  offering Resume protection.
- **Removal:** the confirmation came after about 20 seconds and said "C: grows
  from 56.1 to 126.7 GiB". C: came back to exactly its original size, still
  encrypted and paused, and the Run entry and firmware entry were gone.
- **Resume BitLocker:** protection came back on, the reminder retired, and
  nothing appeared at the next sign-in.
- **Prepare again:** C: stayed 56.1 GiB, and the new check offered the freed
  70.5 GiB.
- Four prompts in five app runs, each at a normal launch or a reopened run's
  first action.

Fixed afterwards: the Clean up text lost the space before "Resume BitLocker",
since Svelte drops a block's leading space.

Left as they are:
- the disk map rounds the installer up (6.6 GiB) where Review rounds to the
  nearest (6.5 GiB), on purpose: the map never understates what's needed;
- while Resume's confirmation is open the removal result is hidden behind it;
  cancelling brings it back;
- a reopened start shows the official release instead of a local test ISO
  picked earlier, which doesn't affect the card;
- the reminder's folder stays after it retires, as a record.

