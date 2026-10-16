# Install without USB: fixes from the first real-hardware run

Plan date: 2026-10-15, revised the same day after review. Based on `main` at
`0afd04e`.

## Summary

The first run on real hardware (an ASUS Zenbook with BitLocker, a Microsoft
account and a Windows PIN) worked, but felt slow, asked for administrator
access too often, and lost the user at the Secure Boot step. This plan fixes
that in four PRs:

1. **Secure Boot restart and reminder:** say how to turn off Secure Boot and
   what happens afterwards before restarting, warn about the PIN, reopen the
   app after sign-in with one **Restart into the installer** button, and fix
   the blurry, unhelpful BitLocker reminder.
2. **Visible progress:** a step list while preparing, with percentages where
   the work can be measured, per-step timings recorded for the test runs, and
   a Stop checking button that actually stops the check.
3. **One administrator prompt per app run:** a single elevated helper that
   serves every operation while the app is open.
4. **Disk check at launch, measured once:** check disks in the background as
   soon as the helper is approved, and stop re-measuring how far Windows can
   shrink when planning.

The staged flow stays limited to testing builds, as today. Provider safety
checks keep their current guarantees except where this plan says otherwise,
and each such change is listed under its PR.

## What happened on the laptop

| Moment | What the user saw | Cause |
| --- | --- | --- |
| Check available space | Administrator prompt, then "Reading disks…", then a long "Checking how much C: can shrink" | One elevated helper per operation; Windows' shrink measurement (`Get-PartitionSupportedSize`) is slow |
| Review changes | Another prompt, then a long "Planning the changes…" | New helper; `Get-StagingPlan` measures the shrink again |
| Preparing | A box saying "Keep this window open" with no progress until copying began | The provider rehashes the whole image, measures the shrink a third time (`Invoke-ConfirmedShrink`), shrinks, then creates and formats partitions; only the copy reports bytes, and its per-file readback doesn't |
| Restart to firmware | Another prompt; had to search for Secure Boot | The "How?" tip is folded away, and "then return to Windows" is easy to miss |
| After saving in firmware | Windows started instead of the installer | By design: the next restart can only be set once Secure Boot is off. The app never said so clearly |
| Signing in | Windows required a PIN reset through Microsoft account recovery | Turning off Secure Boot can invalidate the Windows Hello PIN. Unavoidable while Omarchy needs Secure Boot off; nothing warned about it |
| After sign-in | A blurry BitLocker reminder offering only BitLocker settings, "Remind me next time" and "Leave suspended" | The reminder is a WinForms dialog in PowerShell with no DPI awareness or visual styles, fixed pixel positions, and no hint to go back to the app |

Before installing, the user would have approved four administrator prompts
(check, review, firmware, next restart). After this plan: two, one per app run
(before and after the Secure Boot restart).

Two existing bugs found while planning are fixed too: **Stop checking**
doesn't stop the disk check (PR 2), and the installer instruction always says
"Free space install", even after choosing to replace Windows (PR 1).

## Decisions

| Decision | Choice | Reason |
| --- | --- | --- |
| How to reduce prompts | A long-lived elevated helper; the app itself stays unprivileged | Running the whole app elevated would give the webview and downloads administrator rights, and remove the boundary the helper protocol enforces today |
| When to ask | Right after the window opens, on Windows | Every Windows path (USB and without USB) needs it, and asking early lets the disk check run while the image downloads |
| If the user declines | The app works as today and asks when an action needs it | Declining must not block anything |
| Helper lifetime | Until the app closes | An idle timeout would bring prompts back mid-flow |
| After turning off Secure Boot | Windows starts; the app reopens and offers **Restart into the installer** | Setting the installer for the next start before the firmware trip is unreliable (see Not doing) |
| BitLocker reminder after the firmware restart | Shown as usual, pointing back to the app | Nothing proves the app reopened, so hiding the reminder could hide the only pointer back |
| Stopping a disk check | Stop before the next partition; never kill a measurement | Windows can't interrupt its own measurement, and killing PowerShell doesn't prove the storage service stopped |
| USB route and BitLocker | A note only | The official installer refuses BitLocker disks, even paused ones (see 1f) |

## Checks for every PR

From the repository root: `pnpm desktop:check` and `pnpm desktop:test`; in
`apps/desktop/src-tauri`: `cargo fmt --check`, `cargo test --locked`, and
`cargo test --lib --locked --features staged-iso-testing`;
`testkit/windows-direct/Test-StagedIso.ps1` and `Test-StoragePlan.ps1`, plus
`Test-StagedNative.ps1` and `Test-BitLockerFollowup.ps1` when those parts
change; typos. Each PR adds a `CHANGELOG.md` entry and a section to this plan when it
lands, and gets a review and a VM run before merging.

## PR 1: Secure Boot restart and reminder

### 1a. Instructions before the restart

The steps card spells out the Secure Boot step instead of folding it under
"How?", and the confirmation repeats the essentials.

Card, Secure Boot step (replaces the current line and the "How?" tip):

> Restart into your PC's firmware settings, then:
>
> 1. Find **Secure Boot**, usually under Security, Boot or Authentication.
> 2. Set it to **Disabled** and leave TPM on.
> 3. Save and exit, often with F10.
>
> If the firmware warns that BitLocker may ask for a recovery key, you can
> continue: this app paused BitLocker.
>
> Windows then starts again and this app reopens, so you can start the
> installer. Windows may ask you to set up your PIN again, so make sure you
> can sign in without it, with your password or your Microsoft account's
> recovery email or phone.

Below it, folded, because it only applies to some PCs:

> **Can't change it?** Some PCs only unlock Secure Boot after you set a
> firmware password, or call the setting "OS Type" (choose Other OS).

"Leave TPM on" matters because Omarchy's own manual says "Secure Boot and/or
TPM", and turning the TPM off is the surest way to lose the Windows PIN.

Firmware confirmation (`setup_helper.rs`, `staged_firmware`):

> Restart into firmware settings now? Save your work first. Turn off Secure
> Boot (usually under Security, Boot or Authentication), leave TPM on, then
> save and exit. Windows starts again and this app reopens. Windows may ask
> you to set up your PIN again.

### 1b. PIN warning before preparing

Review's last bullet, when Secure Boot is on, becomes:

> Next: turn off Secure Boot in firmware settings. Windows may then ask you to
> set up your PIN again.

No detection: Windows offers no reliable way to tell in advance whether the
PIN will be invalidated.

### 1c. Reopen after sign-in, then one button

**Reopening.** When the firmware confirmation arrives, and before showing
it, the desktop process (not the helper, so it lands in the signed-in user's
own registry) writes an `HKCU\Software\Microsoft\Windows\CurrentVersion\RunOnce`
value with the app's quoted absolute path. If the write fails, the
confirmation says so instead of promising it: "Open this app yourself after
Windows starts." A failed write doesn't block the restart.

The value is removed when the user declines, or on a definite no-restart
outcome, decided from structured data rather than error text: the result
reports Secure Boot already off, or the operation fails before the provider's
`restarting` stage, which it emits just before running `shutdown.exe`. After
that stage, or after a lost connection, the value stays, because the restart
is under way. Windows deletes RunOnce values as it runs them.

**One button.** The steps become: Turn off Secure Boot, **Start the
installer**, Install Omarchy, Clean up. Step 2's primary action is **Restart
into the installer**: one confirmation, then the helper selects the installer
for the next start (today's `arm`, including its file recheck) and restarts
Windows. A text action, **Set for next restart only**, keeps today's behavior
for people who want to restart later.

New action: `StagedAction::Restart` (arm, then `shutdown.exe /r /t 0`). The
restart happens only after arming returns `boot-scheduled`. If the restart
command fails, the result says the installer is set for the next restart,
which is still true.

**Retrying.** `ArmStagingBoot` refuses any pending one-time boot, including
its own, so after **Set for next restart only** or a failed restart, Restart
would fail. Arming accepts a pending `BootNext` that points at the operation's
own recorded entry (same index, same option bytes), still rechecks the files,
and still refuses any other pending entry.

**Scheduled but not yet restarted.** After **Set for next restart only**, or
a restart that failed, the card's current step is Install Omarchy with no
button today. It gets **Restart into the installer** as its primary action,
with "The installer opens at your next restart." above it, including after the
app is reopened.

**Installer instruction.** Today's instruction ("choose the disk marked
'installer disk, free space only', then 'Free space install'") shows for every
installer. `linuxBytes` 0 doesn't identify Replace Windows, because free-space
installers also reserve nothing. The staging summary gains `replaces`: the
name of the partition the installer must delete, set only for a shrink that
reserved no Omarchy space ("Windows" when the shrunk partition is the Windows
volume, otherwise its drive letter, or "Partition N" without one, as Review
names it). With `replaces` set,
the instruction adds "and delete Windows when it asks for more space" (or
"delete D:"). The installer's own flow is: Free space install, a "not enough
free space" screen, then its partition tool. The card and the restart
confirmation use the same instruction.

Restart confirmation (keep Windows):

> Restart into the Omarchy installer now? Save your work first. In the
> installer, choose the disk marked "installer disk, free space only", then
> "Free space install".

### 1d. Secure Boot state without administrator access

The home card only knows Secure Boot's state from an elevated check, so after
reopening it can't tell which step is current. Read
`HKLM\SYSTEM\CurrentControlSet\Control\SecureBoot\State\UEFISecureBootEnabled`
in the desktop process (readable by standard users; verify in the VM as a
standard user) and expose it in the setup snapshot. It is for display only: an
elevated result takes precedence when present, and arming still checks Secure
Boot itself. Missing value: unknown, as today.

### 1e. BitLocker reminder window

In `providers/windows-bitlocker/BitLockerSetup.ps1` (`Show-EncryptionFollowup`):

- **Sharp text:** call `SetProcessDPIAware` (user32, via `Add-Type`) and
  `[Windows.Forms.Application]::EnableVisualStyles()` before creating the form.
- **Layout that fits:** replace fixed `SetBounds` positions with a
  `TableLayoutPanel` (message, then a right-aligned `FlowLayoutPanel` of
  buttons), `AutoSize` on, Segoe UI 9 pt, a fixed message width and
  `AutoScaleMode` `Dpi`.
- **Waiting installer:** when an installer is waiting, the message points back
  to the app and the BitLocker settings button is hidden:

  > BitLocker on C: is paused for the Omarchy installer waiting on this PC.
  > Keep it paused until Omarchy is installed. To continue, open Omarchy
  > Installer.

  Buttons: **Remind me next time** (default) and **Stop reminders**.
- **Waiting includes a pending restart:** `Test-StagedInstallerWaiting` counts
  `staged` and `arming`, but not `boot-scheduled`. Someone who chose **Set for
  next restart only** and then signed out without restarting would be offered
  **Resume protection** while the installer is still pending. Count
  `boot-scheduled` as waiting while the recorded `scheduledBootUnixSeconds`
  still matches the current boot (the scheduled restart hasn't happened).

### 1f. USB route and BitLocker

Pausing BitLocker in the USB route doesn't work with the official installer:
it refuses a disk with any BitLocker volume, paused or not, and asks for full
decryption (`configurator` on omarchy-iso `quattro`, "Suspending BitLocker is
not enough"). Only the fork with omarchy-iso#187 accepts paused BitLocker.

So the USB route gets one line in its after-writing steps, next to the Secure
Boot line:

> Installing beside Windows on a disk with BitLocker? Turn BitLocker off in
> Windows and let it finish decrypting first.

Revisit a pause action if #187 lands upstream.

### 1g. README

Add two sentences to the "useful upstream support" paragraph: Secure Boot
support in Omarchy (upstream's `plans/consumer-secure-boot.md` in
omarchy-iso) would remove the Secure Boot step and the PIN reset it causes,
except on PCs that don't trust Microsoft's third-party certificate. BitLocker
handling would still be needed.

### Tests

- `StagedSteps.test.ts`: the Secure Boot step shows the instructions unfolded;
  step 2's primary action is Restart into the installer with the secondary
  text action; a scheduled, not yet restarted installer offers Restart; the
  installer instruction for free space, keep, replace Windows and replace D:.
- `stagedLayout.test.ts`: Review's Secure Boot bullet mentions the PIN.
- Rust: `Restart` validation mirrors `Arm`; the RunOnce value is written
  before the confirmation shows, removed when declined, after "already off" or
  after an error before the `restarting` stage, and kept after that stage or a
  lost connection; a failed write changes the confirmation's wording; the Secure Boot registry read
  maps present, absent and unreadable values. Registry tests use a scratch
  `HKCU` key, not the real RunOnce key.
- Provider: arming accepts its own pending `BootNext` and refuses a foreign
  one (`NativeStaged` behind the existing test seams); Restart after
  **Set for next restart only**; a failed restart still reports
  `boot-scheduled`.
- BitLocker provider tests: the waiting-state message and buttons;
  `boot-scheduled` counts as waiting on the same boot and not after it.
- VM: the reminder at 100% and 200% display scaling, before and after; the
  full return path (firmware restart, Secure Boot turned off from the host,
  the app reopening, Restart into the installer).

**Done when** the VM run reaches the installer from the reopened app with one
click, the reminder is sharp at 200%, and a replace-Windows installer shows
the delete-Windows instruction.

## PR 2: Visible progress

### Step list while preparing

The provider sends the step list once, at the start of the write, built from
the reviewed plan, so the app doesn't duplicate its logic:

| Step id | Label | Present when | Progress |
| --- | --- | --- | --- |
| `verify` | Verify the installer image | always | percentage (then mounting and listing its files) |
| `bitlocker` | Pause BitLocker on C: | an affected volume is protected | none |
| `shrink` | Shrink C: | the plan shrinks a partition | "This can take several minutes", moving bar |
| `partitions` | Create the installer partitions | always | none |
| `copy` | Copy and check the installer | always | percentage over writing and reading back |
| `startup` | Add the startup entry | always | none |

The preparing panel shows the list with ticks for finished steps, the current
step in bold with its bar, and pending steps dimmed. "Keep this window open."
and Stop stay where they are. The labels use the partition's real name (`C:`,
`D:`).

**Reaching the screen.** The app reads saved setup status, not live events,
and `run_elevated` keeps only selected event fields. The snapshot gains the
step list, the current step and the finished steps, reset at the start of each
operation, so the panel survives reopening or reloading the window.

Provider changes:

- `verify`: replace `Sha` (`Get-FileHash`) in `Invoke-StagingWrite` with a
  chunked SHA-256 that reports bytes, like `Copy-StagedFile` does.
- `shrink`: its own step, so its check and the resize are no longer hidden
  inside "Preparing the reviewed free space…".
- `partitions`: covers allocation and both formats.
- `copy`: `Copy-StagedFile` hashes each file again after writing it, with no
  progress; that readback reports bytes too, and the step's total counts both
  passes, so the bar doesn't sit at 100% while files are checked.

### Other waits

- **Planning:** the plan step emits "Checking how much C: can shrink…" while
  it measures, and the text beside Review shows the provider's message instead
  of the fixed "Planning the changes…". (PR 4 makes this short.)
- **Restart into the installer:** the arm recheck reports bytes while it
  rehashes the copied files, shown as "Checking the installer files" with a
  percentage, then "Restarting…".

### Stop checking that stops

An existing bug: the helper runs the staged inspection with `allow_abort` off,
and the provider's measuring loop never checks for cancellation, so **Stop
checking** sets a flag that nothing reads until Windows has measured every
partition.

The inspection gets a cancellation file, like the write already has. The
provider checks it before each disk and before each partition's measurement,
and stops with a cancelled result. A measurement already running finishes:
Windows can't interrupt it, and killing PowerShell wouldn't prove the storage
service stopped. While it finishes, the status says "Stopping after C:…".
Planning, preparing and maintenance keep their current cancellation rules.

### Timings

The provider records each step's start and end time in the staging record
(`timings`). The VM and laptop runs read them to see where the time goes; the
app doesn't show them. PR 4's optional item depends on these numbers.

### Tests

- Component: the panel renders steps from the snapshot; a step with bytes
  shows a percentage; a step without shows the moving bar; finished steps show
  ticks; the panel is restored after a reload; narrow windows wrap; the text
  beside Review shows the provider's message.
- Provider: the step list for free-space and shrink plans, with and without
  BitLocker; the chunked hash matches `Get-FileHash`; copy progress covers
  readback; timings are recorded; inspection stops at the next partition when
  the cancellation file appears.
- Rust: a cancelled inspection ends as cancelled with its partial result
  discarded; a cancel during planning or preparing behaves as today.

**Done when** every wait in the VM run shows either a percentage or a named
step with a moving bar, and Stop checking ends the check within one
partition's measurement.

## PR 3: One administrator prompt per app run

### Protocol

Today `elevation::launch` starts a helper per operation, and
`setup_helper::entry` reads one request, runs it and exits. Change both:

- **Desktop:** `Setup` owns at most one helper session (the pipe pair plus a
  reader thread). `run_elevated` sends each request over the existing session,
  starting one (with its prompt) only if there is none or it has died. A
  session launch already in progress is shared: the launch-time prompt and a
  button press during it produce one prompt.
- **Helper:** `entry` loops: read a request, run it, emit `result` or `error`,
  then wait for the next one. A failed or declined operation ends only that
  operation, not the session.
- **Frames belong to an operation.** Each request carries an operation ID,
  and cancel and confirmation frames carry the ID of the operation they're
  for. The ID is unique per request attempt (a repeated Arm or Restart gets a
  new one), separate from a staged installer's `operationId`. One reader
  thread dispatches them; each operation gets fresh cancellation and
  confirmation state. A frame for an operation that isn't
  running is ignored, so a late Stop or approval can't affect the next
  operation (including a delayed `source_lease` acknowledgement).
- **Malformed input or a failed write to the desktop** ends the session the
  same safe way as EOF below: stop dispatching, request cancellation, let
  uninterruptible work finish and reap it, then exit.
- **Pipe closed during an operation** (the app closed or crashed): keep
  today's behavior. EOF requests cancellation; cancellable work stops,
  uninterruptible provider work runs to completion, and the helper exits only
  after the operation has finished and its child has been reaped. Without an
  operation running, EOF ends the helper at once.
- **No replays.** The desktop never resends an operation. If the session
  breaks before a result arrives, that operation fails as "The result is
  unknown". Staged installers show their true state from the saved summary, as
  today. A USB write keeps today's conservative outcome: not presented as
  verified, and the user reconnects the drive and checks again; no receipt
  discovery is added. The next operation starts a new session, with a prompt.
- One operation at a time, as today (`Setup::begin`). Each operation keeps its
  own protected workspace.
- Identity checks stay as they are and run once per session: the helper checks
  the parent's executable at connect, and the desktop checks the helper's
  process ID on both pipes.
- macOS and Linux: the helper loop is shared code, but the desktop closes the
  session after each operation there, so they keep today's prompts until
  someone tests them on those systems.

### Asking at launch

After the main window first renders on Windows, the app starts the session,
so the prompt appears over a window the user can already see. A line in the
window says why while it's open: "Omarchy Installer needs administrator
access to check your disks and prepare installers."

If the user declines, the line goes away. The Install without USB screen then
shows "Administrator access lets the installer check your disks in the
background." with an **Allow** button, and any action that needs the helper
asks as it does today.

### What this changes for security

Today every privileged operation needs its own approval on the secure
desktop. After the session is approved, any process that can drive the
unprivileged app (it runs as the same user) can reach the helper's operations
without another prompt. That is an accepted increase in exposure, traded for
the prompts the user asked to remove. It stays bounded: the helper accepts only
typed protocol requests, never paths or commands from the webview; it
validates each request natively as today; every disk-changing operation
still needs its confirmation (Review, the firmware, restart and removal
dialogs, the USB confirmations); and the session ends with the app.

### Tests

- Rust: two operations over one connection; a cancel or approval tagged for an
  ended operation doesn't touch the next one; a malformed frame ends the
  session; EOF during an uninterruptible step lets it finish before the helper
  exits; EOF while idle exits at once; a broken session fails its operation as
  unknown and is replaced on the next request; an overlapping launch request
  produces one launch.
- Real app: one prompt at launch, then check, review, prepare and the firmware
  restart without another prompt; declining at launch still lets each action
  ask.

**Done when** the VM run goes from launch to the firmware restart with one
prompt, and from the reopened app to the installer with one more.

## PR 4: Disk check at launch, measured once

### Background check

- **When:** as soon as the session is up and the image's identity (SHA-256
  and length) is known: at once for an official release, whose manifest gives
  both before downloading; after selection for a local test image.
  `staged_iso('inspect')` stops requiring a finished download; it never reads
  the image.
- **What:** every internal disk and every NTFS partition that could fit the
  installer, as today. New: the disk holding Windows is measured first (today
  the order follows `Get-Disk`).
- **Its own state.** `Setup` keeps the check's status, results and errors in
  their own slot, with a generation number. A result from an older generation
  (the image was replaced, the user refreshed or cancelled) is dropped, and
  the check never writes the main snapshot, so a late result can't overwrite a
  foreground operation. The home screen and the USB flow aren't shown as busy.
- **Handing off.** An operation the user starts asks the check to stop (PR 2's
  cancellation file) and waits for the current measurement to finish, showing
  "Finishing the disk check…", then runs. Partitions already measured keep
  their results. The check restarts when the user opens Install without USB
  if it didn't finish.
- **The screen.** The disk map can be browsed before the image is downloaded;
  **Review changes** needs the image, and says so. A partition that is already
  measured can go to Review while others are still being checked; Review stops
  the check as above. Refresh stays, and planning recomputes the limit from
  current free space anyway.

### Measure once

Windows' shrink minimum (`SizeMin`) is the slow part; free space is cheap to
read.

- The inspection result for each measured partition adds Windows' raw
  minimum (`windowsMinimumBytes`) and when it was measured. Today it exports
  only derived limits.
- The helper keeps these from its own inspections (not values sent by the
  app), keyed by the disk's unique ID, partition GUID, offset and size, for the
  life of the session. If the user declined at launch, there is no cache until
  a later action's prompt starts a session; that session then caches as
  usual.
- When planning, the helper passes the cached minimum to the plan provider if
  the partition's offset and size are unchanged and the measurement is less
  than 30 minutes old. `Get-ShrinkCandidate` then skips
  `Get-PartitionSupportedSize` and recomputes the limit from current free
  space and the reserve.
- This is an estimate: files can move without the layout changing. The
  measurement right before shrinking stays and is the real gate. It is shared
  with the direct-x86 provider.
- **The final check runs before BitLocker is paused.** Today
  `Invoke-StagingWrite` pauses BitLocker and then `Invoke-ConfirmedShrink`
  measures, so a stale limit fails after BitLocker is already paused. The
  staged write now takes a fresh measurement of its own (never the planning
  cache) before pausing BitLocker, and passes it to `Invoke-ConfirmedShrink`.
  After the pause, that function still runs every check it runs today except
  the slow measurement: eligibility, volume identity, the minimum size with
  the reserve recomputed from current free space, partition identity, the GPT
  layout and the post-resize readback. It rejects a passed measurement for a
  different volume or partition. The direct-x86 caller passes nothing and
  keeps today's behavior.
- **A stale limit leaves no installer behind.** Staging writes its ownership
  record (`preparing`) before hashing, so a failure at this check would
  otherwise show a "Preparing the installer didn't finish" card asking for
  removal. A failure while the record is still `preparing` (before
  `allocation-starting`, so before BitLocker or the disk changed) removes the
  record, and the app returns to the choose screen: "Windows can no longer
  shrink C: that far. Nothing was changed. Check again." Records stay once
  BitLocker or storage changes have started.
- **Steps follow the new order.** PR 2's list gains a `preflight` step, "Check
  that C: can still shrink", before `bitlocker`; `shrink` then covers only
  the resize.

### Optional: skip the third image hash

The helper already holds the verified-image lease for the whole operation
(`HeldIso`, reused through `source_lease` without rehashing), so the
provider's whole-image hash at write time is a third check. If PR 2's timings
show it is significant, the helper tells the provider the image is held and
verified, and the provider skips the hash. Otherwise keep it.

### Tests

- Rust: inspection without a finished download; a result from an older
  generation is dropped; a user operation waits for the check to stop and then
  runs; the check doesn't touch the main snapshot; the cache is keyed by the
  helper's own results and rejected when the partition moved, resized or aged
  out.
- Provider: `Get-ShrinkCandidate` with a supplied minimum matches the measured
  result for the same free space; Windows' disk is measured first when it is
  Disk 1; a stale limit fails before BitLocker is paused and removes the
  `preparing` record; `Invoke-ConfirmedShrink` rejects a passed measurement for
  another volume or partition, and its direct-x86 caller behaves as before.
- Component: arriving at the screen during and after the background check;
  browsing before the download, with Review explaining it needs the image;
  Review on a measured partition while another disk is still checking.
- Real app: timings for check, planning and preparing, compared with PR 2's.

**Done when** opening Install without USB after launch shows a finished or
nearly finished check, and planning no longer runs Windows' measurement.

## PR 1 notes (2026-10-15)

Implemented as planned, with these details:

- The reminder window's focus starts on **Remind me next time**, so Enter
  doesn't stop reminders.
- The USB note is folded ("Installing beside Windows with BitLocker?"), like
  the boot-menu tip beside it.
- Review's Secure Boot-off bullet now reads "Next: restart into the installer."

## PR 2 notes (2026-10-15)

Implemented as planned, with these details:

- Stop checking was hidden for most of the check: every provider event set
  `cancelAvailable` to false. Inspection events now keep it while a cancel
  file exists, so Stop stays available.
- A stopped check ends as cancelled with "Disk check stopped." and no error;
  partitions not yet measured read "wasn't checked" with Refresh.
- The text beside Review shows only the planning step's own message
  ("Checking how much C: can shrink…"); the helper's technical stages keep
  the plain "Planning the changes…".

## PR 3 notes (2026-10-16)

Implemented as planned, with these details:

- The helper's reader registers each operation as its request arrives, so a
  Stop sent right after a request can't fall in a gap before it starts.
- The desktop prepares everything that may show a dialog (the records
  folder) before taking the session, as before sessions, so the first
  operation's dialogs still come before its prompt.
- A result or error tagged for another operation ends the exchange with an
  error and drops the session.
- "Check available space" drops "Windows will ask for administrator
  access" once the session is ready.

## Not doing

- **Running the whole app as administrator:** see Decisions.
- **Setting the installer for the next start before the firmware trip:**
  firmware often clears the one-time choice when its settings open, and if
  Secure Boot stays on, some PCs show a "Secure Boot violation" screen.
- **Avoiding the PIN reset:** it needs Omarchy to boot with Secure Boot on,
  which is upstream's plan, not ours (1g).
- **Dropping the arm recheck:** it checks the copied files before the PC starts
  from them; PR 2 gives it a percentage instead.
- **A new recovery mechanism for lost results:** the staged records and the
  status read already show what happened.
- **Hiding the reminder after the firmware restart:** see Decisions.

## Testing on real hardware

The VM covers everything except firmware menus, the PIN and real-disk timings.
After PR 4, repeat the laptop run with timings, at the laptop's normal display
scaling:

- prompts counted from launch to the installer;
- the reminder's look;
- the app reopening after sign-in, and Restart into the installer;
- the time for each wait, compared with this run.

## Risks

| Risk | Mitigation |
| --- | --- |
| RunOnce doesn't run (policy, a different user signs in) | The home card still shows the waiting installer when the app is opened by hand, and the reminder points to it |
| A long-lived elevated helper is a larger target | Typed protocol, native validation, the same confirmations, exits with the app; see PR 3 |
| The background check delays a user action | It stops at the next partition; the wait is at most one measurement and says so |
| The cached minimum is stale | 30-minute limit, layout match, and the final measurement before anything changes |
| DPI changes break the reminder's layout | Layout panels instead of pixel positions; screenshots at 100% and 200% |
