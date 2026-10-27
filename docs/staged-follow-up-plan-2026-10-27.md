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
  while the app shows a temporary installer. The app leaves a named event for
  that; if the app didn't open, the reminder shows as before.
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
     isn't opted out, keep the RunOnce entry and the named event; otherwise
     remove both. The firmware restart keeps its own registration.
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
