# Windows encryption preparation and explicit follow-up

## September 19 implementation update

The historical full-decryption design below is superseded. Staged preparation
now suspends protection only after explicit plan confirmation and source checks,
immediately before disk allocation/writes. Windows and affected target volumes
stay encrypted. An indefinite OS suspension survives installer restarts; data
volumes use the API without its OS-only reboot-count parameter.

Only newly app-suspended volumes acquire reminders. Existing suspension is left
alone. The task is durable before the mutation, and a pending receipt is written
only after suspension verifies. An interrupted receipt is shown as uncertain,
never silently adopted. At Windows sign-in, Resume protection requires explicit
confirmation and revalidates volume identity/state. Closing defers; an explicit
Leave suspended ends reminders. A verified manual resumption also retires it.
Legacy decryption/manual opt-in records remain supported, but the GUI no longer
offers early decryption or manual reminder registration.

Validation: mocked PowerShell suspension/order/identity/ownership/resumption
checks, storage regressions, 541 measured-boot parser cases, 51 frontend tests,
and 52 native tests passed (six environment-specific native tests ignored).
Three normal-build gate tests also passed. The frontend type check had zero
errors and four existing warnings. No host encryption, scheduled tasks,
partition writes, or actual reboot lifecycle were exercised by these tests.

The independent `gpt-6-astra` / `max` review found one registration retry defect:
scheduler initialization could fail after saving an armed record but before
registering its task. The fix includes scheduler initialization in failure
handling. Retry also reconciles an interrupted record against the same account,
volume identity and current protection state; already-protected volumes can
start a fresh confirmed attempt, while unresolved suspensions are preserved.
Regression tests run the real registration code with disposable files and a
mock scheduler: construction/connection/folder/registration failures, successful
retry, a crash leaving an armed record without a task, and ownership refusals.

## Historical design (superseded)


The September 16 product decision selects staged official-ISO installation for
the future no-USB route. Keep the existing prepared-filesystem builder/deployer
source, but do not expose it as an alternative or fallback. The desktop UI remains
disabled for no-USB installation, and the privileged helper now also rejects the
retained build/deploy destinations. A merged PR is not a released ISO: enabling
the route requires an inspected official ISO containing the needed same-disk
support and qualification of the complete staged installation. No patched ISO
will be shipped as an implicit substitute.

## Implemented independent Windows preparation

After an official image is downloaded, Windows x64 users can explicitly review
turning off BitLocker for use with the official installer, including USB installs.
The authenticated elevated helper inspects the Windows OS volume, obtains a
separate confirmation bound to that inspection, registers a follow-up, then
requests full decryption. It does not suspend protection or install Linux.

Ownership rules:

- Only a fully encrypted, actively protected OS volume can enter app-owned
  decryption. Already-off, already-decrypting and already-suspended volumes do
  not acquire an app-owned restoration obligation.
- A separate manual-reminder action is an explicit opt-in for a decrypted or
  decrypting OS volume. Merely observing encryption off creates no task.
- Match the volume, persistent volume ID, GPT partition GUID and disk identity;
  a reused drive letter is not sufficient.
- Register the task and durable intent before requesting decryption. Confirmed
  completion of the call is recorded afterward. An interruption in between is
  presented as uncertain preparation, not proof the app changed encryption.

The follow-up is an interactive scheduled task for the approving Windows account,
at the next sign-in. Its script/state live in an administrator/SYSTEM-only
directory under ProgramData, independent of the portable executable and temporary
operation workspace. It runs once per sign-in, not on a repeated timer; Fast
Startup and signing out/in do not suppress it based on an unchanged boot time. A file
lock and the task's IgnoreNew policy serialize access.
Preparation rejects elevation under a different Windows account, so it cannot
silently schedule the reminder for someone other than the desktop user.

Closing the prompt or selecting **Remind me next time** leaves it pending.
**Open BitLocker settings** delegates activation, recovery-key backup and consent
to Windows; opening settings is not recorded as success. **Keep BitLocker off**
requires an explicit confirmation and retires the reminder without modifying
encryption. Interrupted preparation also offers **Stop reminders**, without
claiming whether decryption happened. Fully encrypted AND protected status also retires it, including when
the user restores protection manually. In-progress/paused encryption, unknown
status and failures are not reported as restoration. State records remain as
evidence after the task is retired.

The current bounded automatic recovery for *temporary suspension* is unchanged.
That is distinct from this full-decryption follow-up. The new feature never
automatically enables encryption, resumes suspension, calls protector-deletion APIs or
changes the TPM/boot configuration. Before activation, the prompt directs users
to boot through their final Omarchy menu; it does not claim to verify that route
or guarantee future BitLocker unlocks. No restoration can run while Windows is
not running. Existing firmware preparation and staged-install qualification are
separate from this reminder.

## Release gate and remaining work

The no-USB button remains disabled. The subsequent September 17 implementation
adds staged ISO allocation, extraction, boot handoff and cleanup behind that gate;
see the [provider implementation and qualification requirements](../../providers/staged-iso/README.md).
The encryption preparation/follow-up enforces separate explicit consent. The
retained construction provider remains accessible to development tests, not the
consumer desktop protocol. Before enabling staged installs, pin the official ISO,
prove its source-partition protection, and test installation, Windows return and
cleanup on disposable disks and representative firmware. See the
[staged route plan](../staged-iso-first-plan.md).

Validation uses pure policy, mocked Windows boundary and frontend tests plus
native compilation. No BitLocker query/mutation, scheduled-task registration,
physical disk operation or restart is performed on the development machine.
Native task execution, Windows encryption UI activation and actual protected
boots still need machine qualification.

Task registration uses `TASK_DONT_ADD_PRINCIPAL_ACE` to preserve its explicit
administrator/SYSTEM DACL; otherwise the scheduler adds an account ACE that
conflicts with verification. See Microsoft's
[RegisterTask documentation](https://learn.microsoft.com/en-us/windows/win32/api/taskschd/nf-taskschd-itaskfolder-registertask).
