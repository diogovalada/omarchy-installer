# Native Apple bridge protocol v1

Start one directly spawned `omarchy-apple-bridge --stdio` process per app session.
Stdin/stdout are private inherited pipes. Stdout is NDJSON exclusively. Stderr is
diagnostic output and must be drained separately. No listening socket, general
shell, arbitrary executable/path/URL, disk identifier or caller-supplied trust
root is accepted. Authoritative byte counts are **decimal strings** to preserve
UInt64 values across JavaScript. Optional `storageChoices.allocation.recommendedBytes`
is a display-only number and is emitted only within JavaScript's safe integer range.
The process shares the upstream per-user instance lease.

Requests have a maximum of 65,536 bytes plus LF and have this shape:

```json
{"version":1,"id":"request-1","command":"probe","params":{}}
```

IDs contain 1–128 ASCII letters, digits, hyphens or underscores and cannot repeat.
A session admits at most 4,096 requests. Unknown commands and fields are refused.
Malformed or oversized framing may close input; it does not cancel submitted work.

```json
{"version":1,"id":"request-1","type":"result","data":{}}
{"version":1,"id":"request-1","type":"error","error":{"code":"busy","message":"Another native operation is still running"}}
{"version":1,"type":"state","data":{"sessionId":"...","phase":"idle"}}
```

Each admitted request gets one result/error. Inspection, preparation and execution
reply when the upstream operation settles. While waiting, changed state snapshots
are emitted at most four times per second; `state` returns one immediately. A
result means the command finished processing, **not** that installation succeeded:
always inspect `data.phase`, `failure`, and Recovery handoff. Upstream can report
unsupported/failed states or reopen credentials after rejection without throwing
a transport error.

| Command | Parameters | Preconditions and behavior |
| --- | --- | --- |
| `probe` | none | Reports architecture, pins, catalog models/expiry, missing bundle/helper identities and resources. Does not touch disks or imply host eligibility. |
| `inspect` | none | Requires native arm64. Runs read-only Apple host and pinned engine inspection. Refuses models absent from the pinned catalog. Clears previous storage choices. Creates no plan/approval. |
| `prepare_plan` | optional `allocationBytes` string | From `welcome`, prepares/stages/verifies the release and plan; the initial custom allocation is applied by upstream replan. From `plan_prepared`/`plan_review`, requires a new allocation and invalidates approval/acknowledgement. May download approximately 3.6 GB plus engine/metadata. |
| `choose_storage` | `choiceId`, optional `allocationBytes` or `confirmation` | Only in `existing_install_choice`. Accepts a current opaque choice ID. Alongside accepts an optional allocation; replacement requires the displayed identifier typed with case-insensitive matching and rejects allocation overrides. Calls the retained native session choice API, then clears acknowledgement. No disk mutation occurs. |
| `review_plan` | none | Moves `plan_prepared` to `plan_review`. |
| `acknowledge` | `value` boolean | Records acknowledgement only for the visible retained review. |
| `approve` | `bindingDigest` | Requires acknowledged `plan_review` and exact matching binding. Approval is built by upstream from retained trust objects, never JSON display values. |
| `refresh_helper` | none | Refreshes helper presence and signed parent admission; it never registers a daemon. |
| `execute` | `bindingDigest`, `username`, `password` | Requires the retained approved plan, eligible host, signed parent/companion, enabled helper, explicit user confirmation in Tauri and machine-owner credentials. Uses the original InstallerSession one-shot latch, coordinator and authenticated XPC. |
| `retry_recovery` | `bindingDigest`, `username`, `password` | Only after upstream reports an eligible trusted Recovery-authorization retry. Does not repeat an arbitrary install. |
| `cancel` | none | Discards pre-execution state only while idle. Active inspection/download/preparation/helper execution returns `cancel_unavailable`; cancellation is not rollback. |
| `state` | none | Read-only snapshot, also accepted while another command runs. |

Render `plan.facts`, allocation, artifact rows and binding digest before confirming.
After preparing without a custom allocation, call `review_plan`. A custom-size
replan can already return `plan_review`. Never manufacture a digest, target,
approval, engine package, helper request or completion from a display snapshot.

Snapshots include `phase`, `busy`, `activeCommand`, `canExecute`,
`canRetryRecovery`, `cancelAvailable`, `hasExecutionStarted`, `helper`, and
optional `host`, `layout`, `storageChoices`, `plan`, `progress`, `handoff`, `failure`, `credentials`,
`blockingReason`, `admissionBlock`. The `layout` inventory is explicitly
`displayOnly` and cannot select or authorize an extent. It is a read-only snapshot
obtained before upstream's own retained inspection, and may be older than the
prepared plan. The plan's facts/digest and the helper's reinspection govern writes.

`storageChoices` is rendered by the same frontend controls as Windows. At welcome
it offers native automatic placement. Signed preparation can return retained
existing-install options; each gets a session-local opaque ID and an easy typed
identifier such as `Installation 1`. IDs are invalidated on inspect/cancel and
cannot name arbitrary sources. A pending custom allocation survives the existing
install choice screen. Replacement always uses the native candidate's full size
and cannot be resized with `prepare_plan`; discard preparation to choose again.
`plan.deletion` identifies the selected replacement and its prepared allocation.
Render this destructive warning in both frontend review and native confirmation.

Phases are `idle`, `inspecting`, `unsupported`, `welcome`,
`existing_install_choice`, `preparing_plan`, `plan_prepared`, `plan_review`,
`awaiting_install`, `installing`, `awaiting_recovery`, `done`, `failed`.
`awaiting_recovery` is not a successful independently booted OS. Show the retained
human steps exactly; shutdown/boot policy operations are not exposed over IPC.

Send credentials once over stdin; never use argv, environment variables, a file,
logs, telemetry or a retained frontend state store. The bridge never echoes them.
Credentials necessarily exist transiently in process memory and Swift/Foundation
copies; the adapter does not promise complete memory zeroization. Clear UI and
Rust request buffers as soon as the request is sent. A `credentials.rejected`
snapshot means upstream proved no execution began and reopened the credential
sheet. The user may deliberately resubmit; never implement automatic retries.

Do not kill the companion as cancellation. On EOF it waits for an active call
to settle. A bridge/desktop crash or transport timeout after submission has an
unknown outcome: preserve logs/journal, show that uncertainty, and follow upstream
Recovery handling. No stateless restart/resume endpoint is claimed. The process
cannot reconstruct an approved plan from a persisted JSON snapshot.
