# Tauri client integration

`apps/desktop/src-tauri/src/apple_setup.rs` owns the persistent companion process.
Register `AppleSetupService::default()` as managed state and the two commands
`apple_setup_status` and `apple_setup_action`. The client resolves only the fixed
resource path below and verifies its code signature against the desktop's actual
Apple Team ID before launching it directly:

```text
Contents/Resources/direct-apple/Omarchy Apple Bridge.app/Contents/MacOS/omarchy-apple-bridge
```

The default staged daemon plist uses that same location. Keep the complete nested
app there when bundling Tauri resources; do not flatten its `Contents` tree.
Signing/build requirements in [README.md](README.md) still apply. This client does
not install/register a helper, elevate itself, run the old GUI, or create a release
trust root. Non-macOS and Intel/Rosetta builds return explicit unavailable state.

## Frontend contract

Get the initial snapshot with `invoke('apple_setup_status')`; listen to
`apple-setup-state` before dispatching actions. Each action supplies the exact
current snapshot revision:

```json
{
  "action": {
    "expected_revision": 12,
    "intent": {"kind": "prepare_plan", "allocation_bytes": "128000000000"}
  }
}
```

Allowed intent kinds are `connect`, `inspect`, `prepare_plan`, `choose_storage`, `review_plan`,
`approve_plan`, `execute`, `retry_recovery`, `cancel`, and `refresh`.
`allocation_bytes` is optional for `prepare_plan` and an alongside `choose_storage`.
`choose_storage` requires the current opaque `choice_id`; a replacement instead
requires `confirmation` matching the displayed installation identifier and accepts
no allocation override. Rust validates these against the current native choices
and revision; Swift resolves the ID to its retained session option and repeats
the typed confirmation check. No intent
accepts credentials, plan digests, disk identifiers, executable paths, arguments,
URLs or signing requirements from the webview.

Actions reserve the service immediately and return a busy snapshot. The actual
native workflow runs on a worker; subsequent snapshots arrive through the event.
Only one Apple action can run at a time. The command also refuses to begin while
the desktop's USB/other installation service is active. Status reads never spawn
a process. `connect` verifies the signed companion, opens the pipes and reads its
probe; it does not inspect or change disks. `inspect` is a separate read-only step.

The serialized `AppleSetupSnapshot` uses these snake-case fields:

| Field | Meaning |
| --- | --- |
| `revision` | Increasing parent state revision; required for the next intent. |
| `status` | `unavailable`, `disconnected`, `connecting`, `ready`, `busy`, `failed` or `unknown`. |
| `available` | Authenticated companion connection; inspect the native/catalog gates separately. |
| `host_os`, `host_architecture` | Actual parent build host platform. |
| `busy`, `pending_intent` | Parent workflow activity, including native confirmations and credentials. |
| `probe` | Optional unchanged Swift probe object, using its documented camelCase fields. |
| `native` | Optional unchanged Swift state object, using its documented camelCase fields. |
| `plan` | Retained display-only native plan; survives into execution/Recovery screens. |
| `error` | Optional `{code,message}` object. Render `error.message`, not the object. |
| `outcome_unknown` | Missing trusted response after submission or an unsettled prior app run. |

Use the Swift `native.phase`, `native.canExecute`, `native.canRetryRecovery`,
`native.cancelAvailable`, `native.failure`, and `native.handoff` to render the
actual installation state. A parent `status: ready` means the last action finished
processing; it does not override an unsupported/failed native phase or prove an
independently booted installation. `probe.packagingReady` likewise does not
establish this Mac's model or disk eligibility.

## Approval, credentials and lifecycle

The Windows and Mac panels use `StorageControls.svelte` and the same typed deletion
modal. The Mac adapter exposes automatic free/resize placement and native detected
Omarchy replacement; it does not turn the advisory layout into arbitrary targets.

`approve_plan` redisplays the retained plan facts/digest and any permanent-deletion warning in a native confirmation
dialog before sending `acknowledge` and `approve` to Swift. `execute` and
`retry_recovery` display a further operation confirmation. Machine-owner account
and password entry then happens in fixed native macOS dialogs, using Apple's
documented [display dialog command and hidden-answer parameter](https://developer.apple.com/library/archive/documentation/AppleScript/Conceptual/AppleScriptLangGuide/reference/ASLR_cmds.html).
The dialog script contains no interpolated plan or credential data. Credentials
travel from the dialog's stdout to Rust memory to the companion's stdin; they are
never accepted from or emitted to the webview. Owned Rust credential and serialized
request buffers are cleared after use; complete OS/allocator memory zeroization
is not claimed. Cancelled native prompts send no authorization request.

The parent retains stdin for the session, drains stdout and stderr continuously,
and never kills the companion as cancellation. Root disk execution is never run
through the generic USB elevation transport. The app's close handler can consult
`AppleSetupService::active_operation()`: it is true for parent/native active work,
but not merely for the one-shot execution latch, an awaiting-Recovery screen or
an uncertain outcome after work stops.

Before submitting an execution/retry request, the parent creates a private
`apple-installation-in-flight.json` record in its application data directory. It
contains the operation, binding digest, time, source pin and an ownership token,
without credentials. Only a trusted response (or a proven rejection before
submission) clears it. A transport failure retains it and sets `outcome_unknown`.
Its presence on a new app run blocks another automatic native installation until
the upstream journal and Recovery state have been reviewed. This file is an
uncertainty marker, never a reusable approval or a resume endpoint. No stateless
resume/reapproval is synthesized from a saved display plan.

Source validation: Rust formatting parsed the complete module; the Windows
desktop library passed `cargo check --lib`. The macOS-only Rust branch, Swift
companion, signed bundle, native prompts and physical workflow have not been
compiled/executed together. No test suite or installation operation was run.
