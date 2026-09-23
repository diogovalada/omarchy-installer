# Opus review of upstream PR #187 — 2026-09-21

Status: review with subsequent user decisions recorded below. No installer code or commits were changed during the review. The user subsequently requested a short future-work note in the PR description about bulk deletion. Other recommendations remain proposals.

## User decision: preserve upstream editor access

Preserve the pre-PR behaviour for when the partition editor is offered: it remains available through the insufficient-space flow. Do not add an always-available Manage partitions option or restore the removed Tools shortcut in this PR. Keep the guarded editor and installer-partition protections for the new same-disk path.

We are not convinced this is the best user experience: access to partition management seems useful even when enough free space already exists, for example when a user still wants to remove Windows. The user's preference is to keep the default flow simple without removing access: the installer need not automatically offer the editor when space is sufficient, but users should still be able to open it deliberately through an optional menu or advanced action. Record that concern for a separate upstream discussion or proposal, rather than expanding this PR. This decision supersedes the earlier recommendation to fix H1 by adding a new menu entry in this PR.

## User decision: keep this PR only

Keep this work in existing upstream PR #187. Do not split source-resolution, BitLocker suspension, or same-disk support into additional PRs. Organise changes into focused commits within this PR. The reviewer's earlier splitting proposal is superseded by this explicit user decision.

## User decision: defer bulk deletion

Keep individual deletion in this PR. A bulk action to delete all other eligible partitions from the protected editor is future work. Its motivation is to offer a same-disk counterpart to Full disk install, which is already available when the destination is a different disk from the installer, while preserving the installer and EFI partitions. It would reuse the installer/EFI protections and need a concrete preview, confirmation, per-partition checks, and honest handling of partial failure. The PR description now includes this motivation at the user's request.

## Provenance and scope

- User requested a fresh Opus review at maximum effort with one-million-token context, covering the actual PR diff, the community installer, and concerns about excessive scope.
- Backend: Claude Code 2.1.274, OAuth, fresh session `b05061e6-954b-4d37-9e23-daa014064e1d`.
- Requested model: `opus[1m]`; resolved reviewer: **`claude-opus-5[1m]`**, canonical `claude-opus-5`; effort `max`.
- CLI completion telemetry confirmed a **1,000,000-token context window**. This is capacity, not a claim that the prompt contained a million tokens.
- Tools: Read, Glob, Grep, WebSearch, WebFetch only; safe mode and `dontAsk`; no shell, file-editing, or GitHub mutation tools. The CLI also reported `claude-haiku-4-5-20251001` for built-in web-tool processing; the reviewer itself was Opus.
- Reviewed [PR #187](https://github.com/omacom/omarchy-iso/pull/187), draft at head `c5cb70b55d2fec1c6e8678c8462a38eb259ed52e`, base `7cfb7111a06873d61c45d37034577d4ba08d3f4f` (`quattro`).
- The reviewer read the full 1,604-line diff, current and base code, relevant orchestration, community staging code, and upstream BitLocker policy context. It received a constructed brief containing the user's goals, implementation facts, prior observations, and explicitly provisional recommendations, as requested by the user.
- No tests were executed during this review. Earlier reported tests are not new validation. Full staged installation, Windows/TPM return, and physical-device qualification remain outstanding.

## Main conclusions and primary-agent assessment

The primary agent challenged the first report and received a clarification in the same Opus/max/1M session. Opus **withdrew** the unsafe unknown-source fallback, reclassified automatic BitLocker resumption as an unvalidated policy proposal, accepted the broader cleanup-reporting gap, and acknowledged that separate commits in one PR are reasonable. The clarification is preserved after the first report. The original report must not be read as the final agreed recommendation on those points.

The core same-disk safeguards are justified. Keep dedicated staging, protection of the installer source and all ESPs on that disk, guarded deletion, full-disk-wipe restrictions on that disk, overlap/identity checks, and verification of kernel partition geometry before formatting. Do not add cross-firmware EFI discovery, filesystem resizing, or same-disk deferred provisioning to this PR now.

The call graph confirms an inherited usability limitation: `open_partition_tool` is called only inside `not_enough_space`. If enough space already exists, a user cannot reach the guarded editor to remove Windows. The user has decided to preserve that access pattern in this PR and retain the concern for separate consideration. BitLocker checks that prevent reaching the editor are a separate review finding; any proposed remedy must respect this decision about when the editor is offered.

The missing active-holder check and incomplete post-write cleanup reporting deserve focused fixes. A partition can have no mounted filesystem and still be used by an active device-mapper, RAID, or LVM stack. A failed bookkeeping operation after partition creation must not be reported as complete cleanup.

### Assessment of the first report

Finding identifiers below refer to the verbatim report appended after this assessment.

| Finding | Primary-agent assessment |
| --- | --- |
| H1: editor inaccessible when free space is sufficient | **Confirmed inherited limitation; preserve by user decision.** Do not add a new menu entry in this PR. Keep broader editor access as a separate UX concern. |
| H2: BitLocker failure prevents reaching deletion | **Usability issue retained for scoped evaluation.** Any recovery or gate-ordering change must preserve the existing insufficient-space access pattern and must not weaken checks on encrypted volumes being retained. No always-available management entry is authorized by this finding. |
| H3: unresolved source should warn and continue | **Rejected; reviewer withdrew this fallback.** Clearing `install_media_disk` makes every disk ordinary and can expose the unknown source to full wipe. Both agree to keep writes blocked unless the target is proved independent, or the live source is positively proved RAM/remote backed. Current recognized copy-to-RAM boots are explicitly supported; future layout uncertainty is not proof they already fail. Improve the explanation of unsupported sources. |
| M1: no active-holder check before deletion | **Agree.** Check both the displayed candidate and the final destructive operation; fail safely if usage cannot be established. |
| M2 / earlier B: successful creation can remain untracked | **Agree.** Tracking before UUID lookup closes that particular gap, but earlier post-write discovery failures also need honest incomplete-cleanup reporting. Never guess an identity to delete. |
| M3: unreadable BitLocker signature exits the installer | **Agree on recoverable UX, not ignoring the failure.** Name the affected partition and return to selection; no writes based on an assumed absence of encryption. |
| M4: default one-Windows-boot resumption should replace indefinite suspension | **Agreed after clarification: a policy proposal needing end-to-end validation, not an established bug.** See the BitLocker qualification below. Preserve the user's chosen explicit-resume/reminder behavior until a different policy is deliberately selected and qualified. |
| M5: rewrite of the existing Secure Boot plan rationale | **Agree.** Drop that editorial change or state the new capability neutrally; do not speculate about why upstream made its policy. |
| L1 / earlier A: deletion ignores UUID-only replacement | **Agree with the behavior and a small identity check.** Lower priority than active-holder protection; the reproduction does not establish a likely physical-device race. |
| L2 / earlier C: multi-parent resolution | **Unproven on real topology.** Reject unsupported/ambiguous backing without offering possible source disks for destruction. Do not build a universal storage graph, and do not dismiss the risk using an untested assertion about `lsblk -d`. |
| L3: empty source row passes the line-count check | **Agree.** Reject an empty/missing snapshot where it is captured. Existing later checks fail closed. |
| L4: parted warnings appear in the TUI | **Defer cosmetic suppression.** Retain useful diagnostic information; this is not a reason to expand the PR. |
| L5: old free-space tip disagrees with actual minimum | **Agree.** Correct the touched UI/documentation consistently: current configurator minimum is 32 GiB total, including its dedicated 2 GiB ESP. |
| L6: BitLocker clear-key regex rejects trailing whitespace | **Reasonable small robustness improvement.** Keep exact semantic matching and read-only unlock verification. |
| L7: stronger rollback applies beyond same-disk installs | **Agree.** Preserve it and accurately describe its shared-path impact. |
| L8: autoinstall can bypass configurator checks | **Document the boundary.** Do not claim these guards cover all automated provisioning; expanding autoinstall is outside the interactive feature unless a supported entry path actually invokes it. |

### Scope recommendations and unresolved choices

- **EFI protection:** agree with Opus over the earlier primary-agent proposal: keep source plus every ESP protected for now. Precise active/retry-loader discovery adds firmware-dependent complexity. Retained partitions can be addressed after the replacement system boots.
- **Bulk deletion:** **User decision: defer and mention as future work.** Keep individual deletion in this PR. A later bulk action would reuse the guarded editor's checks, preview the concrete affected partitions, and stop/report partial failure; the review's small line-count estimate is not an implementation budget or proof of simplicity.
- **Other BitLocker target disks:** defer expanding their editor. Upstream previously rejected the encrypted free-space path entirely, so the new restriction is not a loss of an existing usable encrypted path. This does not negate the same-disk editor requirement.
- **Data-only encrypted volumes:** retain verified suspension of target-disk BitLocker volumes for this scope. Reliable Linux-side classification and permission to retain active protection are a separate policy refinement; the existing scope is inherited, not newly introduced by this PR.
- **PR splitting:** **User decision: keep this PR only.** Use focused commits within #187; do not create separate PRs for parts of this work. Opus's earlier splitting proposal is retained in the verbatim review as history, not as an active recommendation.
- **Validation:** first test the supported end-to-end staging/install path and ordinary ISO/copy-to-RAM/loopback regressions in disposable virtual disks. Then test Windows with virtual TPM, including cancel/retry and subsequent boots. Tests of isolated shell helpers do not establish boot compatibility.

### Follow-up: upstream deliberately removed general tools access

Further history inspection after the user's question found that the editor was once directly available: commit `7e79355c7139f8f9a61e51c9ebbe801cb5ea1d5e` added an Open partition tool installation-mode choice. The later [orchestration change](https://github.com/omacom/omarchy-iso/commit/c50d0bc29948d86d73c7f9f9d393313c3a58a17b) moved general access into a Ctrl-T Tools menu while retaining the insufficient-space entry point. DHH subsequently [removed that Tools menu and shortcut](https://github.com/omacom/omarchy-iso/commit/c77a7b555bce92e49368b5e1f84af540edfd2560), with the message "Drop the tool offering during this initial install."

The reviewed base has no remaining general menu/shortcut to the editor; its only wizard entry point is the insufficient-space flow. The cfdisk executable remains callable from a shell. The removal commit does not explain the deeper UX rationale; simplification is an inference. This history qualifies H1 as an inherited restriction. The user subsequently chose to preserve that behaviour in this PR while recording doubt about its usability, as stated in the decision above.

### BitLocker qualification

Microsoft documents that the default reboot count resumes OS-volume protection on the next Windows boot; zero leaves it suspended until explicitly resumed. Microsoft also documents resealing to changed measurements when protection resumes. This supports Opus's suggestion as a candidate behavior, but does not qualify our installation sequence, interim Windows boots, or all managed-device policies. The FAQ describes conditions under which Entra policy/key-backup requirements can prevent automatic resumption. [manage-bde protectors](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/manage-bde-protectors), [Suspend-BitLocker](https://learn.microsoft.com/en-us/powershell/module/bitlocker/suspend-bitlocker), [BitLocker FAQ](https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/faq).

The current app's explicit resume reminder was an intentional user-approved choice. Replacing it with automatic resumption is a separate behavior decision requiring a Windows/TPM round trip; this review did not make that change.

Similarly, a current Arch package version is not evidence of what is inside a particular released or rebuilt ISO. Verify the actual artifact's cryptsetup support before qualification.

## Follow-up: pre-existing partial-write cleanup gap

At the user's request, the original Claude reviewer was resumed to decide whether the remaining partial-write cleanup issue belongs in PR #187. Reviewed head: `bd4f83fd316a19503b8ee44dc2f59a166bbebcff`. Session: `b05061e6-954b-4d37-9e23-daa014064e1d`; resolved reviewer `claude-opus-5[1m]` (canonical `claude-opus-5`), maximum effort, telemetry-confirmed 1,000,000 context capacity. Read-only consultation; no code or GitHub changes.

Evidence from the primary agent: on a disposable loop-backed GPT image with a deliberately stale busy kernel partition node, real `parted` wrote a new entry and then returned nonzero because it could not inform the kernel. `create_partition` returned before setting `created_partition_write_uncertain`; no new number was tracked, rollback returned success, and the new entry remained. This failure case already existed at the upstream base. The reproduction establishes correctness of the finding, not frequency or reports of affected users.

**Agreed recommendation: fix narrowly in this PR as optional hardening, not a merge blocker.** The reason is that #187 already changes this exact partition-creation and cleanup-reporting path, and the reproduced write escapes its new incomplete-cleanup warning. This does not establish a new data-loss regression or a failure of the installer-partition protections. Deferring with explicit documentation of the existing limitation remains defensible; maintainer preference is unknown. No implementation was authorized by this consultation.

If implemented, distinguish a proven unchanged table from an uncertain or changed table, preserve the original command failure, and never delete using guessed ownership. Keep tests on disposable images. Do not introduce a transaction engine or unrelated storage changes.

The primary agent challenged two parts of the initial answer, and Claude explicitly withdrew them: the claim that the issue likely already affected users (no occurrence evidence), and its approximately six-line implementation sketch/test expectation. Matching partition numbers alone does not prove an unchanged table or a successful read; a safely identified new partition could be removed by rollback, so rollback need not always return failure. The final recommendation stands without those claims. Raw first response and clarification are retained in the gitignored review directory; the reasoning above is the durable outcome.

### Implementation authorized and delegated

The user subsequently authorized planning and delegation of the fix to the existing Sol agent (`/root/pr187_agreed_fixes`, `gpt-5.6-sol`, high effort). Baseline head was `bd4f83f`; implementation remains limited to the existing PR.

The chosen plan compares successful, nonempty full `sfdisk --dump` snapshots before and after a failed `mkpart`. Mark uncertainty before attempting the write. A failed baseline read blocks the attempt; a failed command with a proven unchanged table retains ordinary cleanup of earlier known creations; a changed or unreadable table retains uncertainty and refuses cleanup without guessing ownership. The original operation still fails. Prior unresolved uncertainty must never be cleared accidentally. Successful creation retains the existing number/identity checks. No automatic adoption of entries after failed commands, retries, transaction system, or unrelated changes.

Required disposable-image regressions cover a real write followed by an injected nonzero exit, an ordinary rejected overlap with unchanged table and earlier owned creations, failed before/after snapshot reads, and changed identity despite unchanged partition numbers. Keep existing GPT/MBR, preservation and rollback tests. The agent is authorized to make one focused commit, push to #187, keep it draft, and report actual validation without claiming end-to-end qualification.

## External review, verbatim

The report below is the reviewer's original output, preserved as evidence. It is **not blanket endorsement**: use the user decision and assessment above, plus the follow-up clarification below, when deciding changes. In particular, its recommendation to add a Manage partitions menu entry has been superseded by the user's decision to preserve upstream behaviour.

I read the brief, the full diff, the current PR-head sources in `omarchy-iso/same-disk-free-space`, the two base files, the surrounding orchestrator, the community app's `Staging.ps1`, and upstream PR #105. Everything below is my own reading of the code; where I'm relying on the primary agent's reported test runs rather than my own observation, I say so.

---

# Verdict

**Both — but not in the places the brief assumes.**

You are **over-doing it on the shared code path**: the PR adds several new *fatal* aborts to the ordinary, pre-existing install flow (unidentifiable live medium, btrfs-backed loopback, an unreadable partition signature, a non-GPT BitLocker disk). Upstream's behaviour in those cases was "degrade quietly and let the user pick a disk." Turning them into "the installer exits" is the change most likely to get this PR rejected, and none of it is required for same-disk installs.

You are **under-doing it on the feature itself**: the restricted deletion menu — the thing that answers "I want to remove Windows" — is **reachable only when free space is already insufficient**. If the user shrank C: generously and there's 60 GiB free after staging, there is no menu entry anywhere that opens it, and full-disk install is (correctly) removed on that disk. The user's correction ("I never asked you to disable the editor, simply to protect the installation partition") is therefore still only half-applied.

And you are **under-doing it on one real safety check**: deletion looks at mountpoints but not at holders, so an LVM PV or MD member with no mounted filesystem can be deleted out of the partition table while the mapping stays live.

Everything else — the protected-table snapshot, explicit overlap check, GPT-UUID rollback identity, kernel-vs-table geometry check before `mkfs`, blanket ESP protection, refusing full-disk install on the source disk, and the clear-key probe design — is right-sized. I would not strip any of it.

**Smallest coherent PR I would advocate: three PRs, not one.**

1. **"Don't offer the disk backing a loopback live medium as an install target."** `get_install_media_disk` + its tests, ~60 lines, no policy change. This is a genuine upstream data-loss bug: boot with `img_dev=/dev/sda2 img_loop=...` (archiso's `archiso_loop_mnt` hook, which this ISO ships — `configs/airootfs/etc/mkinitcpio.conf.d/archiso.conf:2`), and base `get_root_disk /dev/loop0` returns nothing, so upstream happily offers to wipe the disk holding the ISO it is reading from. Pure win, lands fast, buys credibility.
2. **"Accept suspended BitLocker via a verified clear-key probe."** `bitlocker_is_suspended`, `check_bitlocker`, README, the two BitLocker tests. Independently useful to every existing dual-boot free-space install, and it's the one change a maintainer will want to think about on its own.
3. **Same-disk free-space install.** Protection snapshot, restricted editor, identity/geometry verification, full-disk blocks.

Splitting does not undermine the user's needs — nothing ships to end users until all three land anyway, and the community app's USB-less path is already gated off in published builds. What it buys is three reviewable units instead of one 1,229-line draft from an outside contributor touching disk-destruction policy.

---

# 1. Correctness findings

## High — functional gap that blocks the stated goal

**H1. The deletion menu is unreachable whenever free space is already sufficient.**
`open_partition_tool` is called from exactly two places, both inside `not_enough_space()` (`configurator:620`, `configurator:630`), which is only reached from `run_partition_decide` at `configurator:707,725,732` — i.e. only when the largest free region is too small. On the media disk `install_mode_form` builds `choices=()` then adds only "Free space install" and "Choose a different disk" (`configurator:1143-1156`), and full-disk install is removed. So a user with 60 GiB free and a Windows partition they want gone has **no path to the delete menu at all**.

*Remedy (small):* in `install_mode_form`, when `is_install_media_disk` (or `$protected_disk == $disk`), append a `"Manage partitions"` choice, and add the matching `case` arm in `select_installation` (`configurator:1221`) calling `open_partition_tool`. ~5 lines. This is the single highest-value change in this review.

**H2. BitLocker verification failure dead-ends the installer before the delete menu exists.**
`run_partition_decide:643` does `check_bitlocker || abort "..."`, and `abort` exits the process (`configurator:178-183`). On the media disk this means: an encrypted data volume the user intends to *delete* prevents them from reaching the menu that would delete it. Deleting an encrypted partition has no encryption-safety implications — it's destructive by definition and explicitly confirmed.

*Remedy:* on the media disk, replace the abort with `say` + `open_partition_tool` + `return 1`. Optionally skip the gate for volumes the user has already chosen to delete. ~4 lines.

**H3. New hard aborts on the shared path where base degraded gracefully.** (High for acceptance/regression risk; low for data harm.)
`disk_form:1012-1019` aborts before disk selection when the source can't be resolved. Concretely, upstream boots that work today and would now refuse to start:

- ISO loop-mounted from a **btrfs** filesystem — `get_install_media_disk:404` returns 1 → abort. Base offered all disks.
- Any boot where `/run/archiso/bootmnt` is absent and `ram_backed_live_root` (`configurator:378-389`) doesn't match. That function hard-codes archiso internals — `/run/archiso/copytoram` being tmpfs, the live image being a **loop** device mounted at `/run/archiso/airootfs`, and its backing file living under copytoram. If any of that differs in a future archiso, `copytoram=y` users get "Cannot identify the installer source; stopping." Base needed none of this: no bootmnt simply meant no disk to exclude.

*Remedy:* make unresolved source **non-fatal**. Print a warning, exclude what you can identify, and set `install_media_disk=""` so same-disk support is off. Delete `ram_backed_live_root` and the `configurator_dry_run` global (`configurator:427-428`, which also duplicates the `$1 == dry` convention `run_partition_execute` already uses). That removes ~25 lines, one test, and a whole class of "installer won't start" bugs. Keep an abort only for the loopback case where the backing is a local file you genuinely cannot map — that hazard is one the PR itself creates by dropping base's name-based exclusion.

## Medium

**M1. Deletion checks mountpoints but not holders.**
`delete_unprotected_partition:85-86` rejects only non-empty `MOUNTPOINTS`. An LVM PV, MD member, or closed-but-mapped dm device has no mountpoint. `parted --script rm` will write the new table and then fail only to inform the kernel (it typically still exits 0), so line 87's `|| return 1` won't catch it, line 90's `verify_existing_partitions` passes, and the function reports success. The stale mapping keeps pointing at those sectors, and a later `create_partition` can allocate the same extent and `mkfs` over live data.

*Remedy:* additionally require `/sys/class/block/${device##*/}/holders` to be empty. ~3 lines, no new dependency.

**M2. Finding B — CONFIRMED, materially.**
`create_partition` (`disk-partitioning.sh`, the `identity=$(created_partition_identity ...) || return 1` line) computes identity **before** `created_parts+=("$num")`. If `sfdisk --part-uuid` fails after a successful `mkpart`, the partition exists, `created_parts` is empty, `rollback_created_parts` returns 0 immediately, and `disk_abort_hook` reports the original failure while claiming nothing was left behind. Some pre-recording exposure existed in base (the number-is-new and size-tolerance checks), so the PR adds a third such point rather than inventing the class.

*Remedy:* move `created_parts+=("$num")` to immediately after the `grep -qx "$num" <<<"$before" && return 1` check; record the identity afterwards; on identity failure still `return 1`. Rollback's `verify_created_partition` then fails for that number, `rollback_created_parts` returns 1, and `disk_abort_hook` (`configurator:542`) already says "created partitions could not be safely removed. Inspect the disk before retrying." Add the numbers to that message. This keeps the existing "a refused creation stays out of the rollback list" checks green — both of those fail before `mkpart` succeeds.

**M3. `check_bitlocker` turns an unreadable partition into a fatal abort for every free-space install.**
`configurator:481-483`: `od` failing or returning fewer than 8 bytes → `return 1` → abort. Base's `dd` simply didn't match. Fail-closed is the right *direction* (unreadable ≠ "no BitLocker"), but the consequence shouldn't be "the installer exits." Name the partition and return to the disk/mode picker.

**M4. `-RebootCount 0` is the wrong default in the README.**
`README.md` tells users to run `Suspend-BitLocker -MountPoint C: -RebootCount 0`, which suspends **until explicitly resumed** — leaving the volume unprotected indefinitely if the user forgets, which the PR's own text then warns about. The Windows default (one reboot) is better here: the reboot counter is decremented by the Windows boot manager, so booting the ISO and installing Linux doesn't consume it; the clear key is still present for `cryptsetup` during install; and the first time the user boots Windows through the new menu, Windows resumes protection and re-seals TPM protectors to the *new* PCR values automatically. That is exactly the self-healing behaviour you want after changing the boot configuration. Cancelling also self-heals. I'd recommend the default and mention `-RebootCount 0` only for users who expect several Windows boots before finishing.

**M5. The `plans/consumer-secure-boot.md` edit re-characterises the maintainer's own policy.**
I read #105. Its description does ground the decrypt guidance in signature detection ("Since the installer detects BitLocker via the `-FVE-FS-` signature, a suspended volume remains detectable and will abort"), so the PR's claim isn't fabricated — but the review framing there treats aborting on suspended BitLocker as intentional policy, and an outside contributor rewriting a plan document to say the maintainer's stated reason was really just a detection artifact is the sort of thing that sours a review.

*Remedy:* drop the `plans/` change from the PR entirely, or reduce it to a neutral statement of the new capability: "Suspended volumes are now distinguished from protected ones by verifying a usable clear-key protector, so the requirement to fully decrypt no longer applies to free-space installs (#105)." No motive attribution.

## Low

**L1. Finding A — CONFIRMED as code behaviour, but low materiality.**
`delete_unprotected_partition:81-82` compares only the parted row (`num:start:end:size:fs:name:flags;`), which carries no GPT UUID, so a UUID-only replacement with identical geometry, name and flags passes. That is a real asymmetry with the rollback path's `created_partition_identity`. But the `expected` row is captured seconds earlier in the same menu, nothing else is running in the live environment, and any mistaken deletion would have to land on byte-identical geometry — so the practical risk is ~nil. I'd rate this a consistency nit, not a safety hole, and I'd fix it only because it's cheap: capture `sfdisk --part-uuid` alongside the row at listing time and re-check both. **Do not let this gate the PR** — M1 is the genuinely load-bearing gap in the same function.

**L2. Finding C — real in the helper, unproven input.**
`get_root_disk` (`configurator:364-368`) does walk `lsblk -dno PKNAME | tail -n1`, so a multi-parent line would resolve to one arbitrary parent. But the reproduction stubbed `lsblk`; with `-d` (nodeps) real lsblk does not emit a parent list for a multi-parent dm device. The supported path is already protected — `can_install_beside_media:436-441` requires `TYPE == part` and `PKNAME == disk basename`, which any dm/LVM/RAID source fails. The residual risk is only *incomplete exclusion*, i.e. the same class as the btrfs case in H3.
*Remedy:* allowlist source types in `get_install_media_disk` (`part`, `disk`, `loop`, `rom`) and route anything else into the H3 "warn and disable same-disk" branch. Two lines. **Do not build a storage graph** — that is exactly the over-engineering the user is worried about.

**L3.** `protect_install_media_partitions:45-46`: if the grep finds nothing, `protected_partition_table` is empty, and `printf '%s\n' "" | wc -l` is **1**, so the `-eq 1` guard passes. The empty first line later makes `verify_existing_partitions` fail (fail-closed), but the error surfaces in the wrong place. Add `|| return 1` to the grep.

**L4.** The new `parted -ms ... print` calls (lines 32, 41, 61, 81, 95, 127 of `disk-partitioning.sh`, plus `configurator:1065`) don't redirect stderr. `-s` prevents prompting, so there's no hang risk — but parted's warnings will land on the gum TUI. Cosmetic.

**L5.** `open_partition_tool:1109` still says "at least 32GB unallocated, plus 2GB more if no ESP already exists", while `run_partition_decide:717` uses a 32 GiB minimum that *includes* the 2 GiB ESP. Pre-existing inconsistency the new README text sits next to.

**L6.** `bitlocker_is_suspended:462` anchors the protection line with `clear key$`. Trailing whitespace from a future cryptsetup would fail closed and tell an already-suspended user to suspend. Use `clear key[[:space:]]*$`.

**L7.** Behaviour change worth calling out in the PR description: rollback went from best-effort (`parted rm ... || true`) to refuse-and-abort on any identity mismatch, **for all free-space installs**, not just same-disk. I think that's an improvement, but it's a silent change for existing users.

**L8.** The autoinstall/`cidata` path skips the configurator entirely, so none of these guards apply there. Pre-existing and out of scope, but the PR body claims it "guards alternate full-disk entry points" — worth one honest sentence.

## What I verified vs. what I'm taking on report

I read the code and traced the flows myself. I did **not** run any test: this session is on Windows, the storage tests need root and loop devices, and the BitLocker test needs cryptsetup plus the upstream fixtures. The reported 93-test/24-test runs are the primary agent's, not mine.

Two external facts I did check: cryptsetup gained **clear-key BitLocker open support in 2.8.2**, and Arch currently ships **2.8.8** — so the shipped ISO satisfies it, and anything older fails closed (the feature silently becomes useless rather than unsafe). Worth one line in the PR. The `--key-file /dev/null` trick relies on cryptsetup preferring the clear-key VMK over the supplied key; `test/bitlocker-suspension` against the upstream `bitlk-aes-xts-128-clearkey-only.img` fixture is the only thing pinning that behaviour, which is a good reason to keep that test prominent.

**What the clear-key probe proves:** a clear-key VMK exists and works right now, cryptsetup can derive the FVEK, and the volume is not in an unsupported conversion state (the partially-encrypted fixture is rejected). **What it does not prove:** that Windows will boot afterwards, that the user will resume protection, that the suspension will still be in place at the next Windows boot (see M4), or anything about other disks.

---

# 2. Keep / simplify / change now / defer

| Item | Call | Why |
|---|---|---|
| Blanket protection of source + **all** ESPs on the source disk | **Keep** | The temp ESP created by `Staging.ps1:238` carries the ESP type GUID, so type-based protection covers it; labels wouldn't. Precise boot-ESP detection (brief #3) is **rejected** — BootCurrent paths, optional `LoaderDevicePartUUID`, and retry requirements all differ by firmware, and the cost of keeping a stranded 100–300 MiB Windows ESP is trivial next to that complexity. |
| Protected-table snapshot, explicit overlap pre-check, `verify_*` before writes | **Keep** | Cheap, no-op on unprotected disks (`verify_existing_partitions:59` returns 0 early), and the loop test exercises the real path. |
| GPT-UUID / MBR disk-id rollback identity + kernel-vs-table geometry check | **Keep** | Directly prevents formatting a stale node. MBR branch exists only to avoid breaking the existing UEFI+MBR free-space path. |
| Restricted delete menu instead of `cfdisk` on the source disk | **Keep** | Right call, though the code comment overstates it — deleting a mounted partition's table entry doesn't destroy the data underneath, and the kernel won't drop an in-use partition. Simpler to reason about is justification enough. |
| Blocking full-disk install on the source disk (4 entry points) | **Keep** | I traced all four: `install_mode_form:1145`, `select_installation:1207`, `confirm_disk_overwrite:1159`, `configurator:1306`. Comprehensive. |
| **Reaching** the delete menu (H1) | **Change now** | Add a "Manage partitions" mode choice. |
| BitLocker gate ordering on the media disk (H2) | **Change now** | Don't dead-end before the menu the feature exists to provide. |
| Hard aborts on unresolved source; `ram_backed_live_root`; `configurator_dry_run` (H3) | **Simplify** | Warn and disable same-disk instead. Net −25 lines. |
| Holders check before delete (M1) | **Change now** | 3 lines, closes the only path I found where a "safe" delete can precede an overwrite of live data. |
| Tracking order in `create_partition` (M2) | **Change now** | Report leftovers honestly instead of claiming clean rollback. |
| **Bulk "delete all other partitions"** | **Change now** — *I disagree with deferring this* | The user asked for it twice ("allow full disk erasure except for our installer partition"). A Windows disk has 3–5 partitions and today each one costs a full round trip through the mode picker. It's ~12 lines reusing `delete_unprotected_partition` in a loop behind one confirmation that **lists every partition by name and size**. It's also the piece most likely to draw maintainer pushback, so keep it in the same PR as the restricted editor and make the confirmation explicit. |
| Per-target-disk BitLocker scope, with verified suspension replacing decryption (brief #6) | **Keep** | Distinguishing OS from data volumes from Linux isn't reliable, and absence of a TPM protector isn't proof. In the supported flow there's no friction anyway: `Get-AffectedBitLocker` (`providers/direct-x86/BitLocker.ps1:78`) already selects target-disk volumes plus the OS/boot volume and suspends them all. |
| Guarded deletion on **other** BitLocker disks (brief #5) | **Defer — and I reject the premise** | This is not a regression. Base *hard-aborted* the whole configurator on any BitLocker signature (`base-configurator.sh:523-528`), so the PR is strictly more permissive. And full-disk install is still offered on a non-media BitLocker disk in both base and PR — the user's "delete Windows" goal is already achievable there. Only the media disk needs the guarded editor. |
| Loopback backing-disk resolution | **Keep, and split out first** | Fixes a real upstream hazard. |
| Btrfs rejection | **Simplify** | Don't let it abort the installer (H3). Resolving all btrfs members is optional polish; warn-and-continue is enough. |
| copytoram handling | **Simplify to nothing** | Base already tolerated a missing bootmnt correctly. |
| Dedicated staging vs. copy-less Windows file (brief #1) | **Keep / defer the alternative** | Yes, this is the simplest dependency boundary: once staged, nothing in the install depends on the Windows filesystem, which is precisely what makes "delete Windows" safe. |
| Temp ESP retained until the new system boots; 512 MiB (brief #2, #9) | **Keep** | 512 MiB is one aligned allocation nobody will notice; shrinking it to fit an 0.85 MiB GRUB buys nothing and risks firmware/FAT edge cases. Keeping the retry boot path after Windows is gone is worth far more. |
| Filesystem-aware Linux resizing (brief #7) | **Defer** | Deletion-only is the right minimal addition; Windows already does the shrink natively. Say so plainly in the README. |
| Same-disk deferred provisioning / OEM (brief #8) | **Defer** | Already excluded via `can_install_beside_media:437`. |
| `plans/consumer-secure-boot.md` policy rewrite (M5) | **Drop** | Not yours to re-characterise. |

---

# 3. Recommended near-term patch scope

**Before marking anything ready for review** (all small, all inside existing structure):

1. H1 — add the "Manage partitions" mode choice.
2. H2 — media-disk BitLocker failure routes to the menu, not `abort`.
3. H3 — warn instead of abort on unresolved source; delete `ram_backed_live_root` + `configurator_dry_run`; add the source-type allowlist from L2.
4. M1 — holders check in `delete_unprotected_partition`.
5. M2 — track the created number before capturing identity; name leftovers in the abort message.
6. M3 — unreadable-signature failure returns to the picker.
7. M4 — README uses the Windows default reboot count.
8. M5 — drop the `plans/` edit.
9. Optional bulk delete (~12 lines) and L1/L3/L6 (~6 lines total).

Net effect: roughly −25 lines of over-reach, +30 lines of feature and safety. The PR gets *smaller* and does *more*.

**On splitting:** split. The BitLocker policy and same-disk support solve different problems, and only their combination is needed for your end-to-end flow — which is already disabled in published builds, so nothing user-facing waits on serialising them. Leading with the loopback fix costs you one cheap PR and changes the conversation from "here are 1,229 lines relaxing your BitLocker policy" to "I found a way your installer can offer to wipe the disk it's booting from; here's the fix — and here are two follow-ups."

**Acceptance forecast** (a reasoned guess, not knowledge): loopback fix, likely and quick. BitLocker policy, plausible — the technical case is strong (Microsoft's own guidance for firmware/boot changes is *suspend*, not decrypt; requiring full decryption of a 1 TB drive is a multi-hour barrier), and the cryptsetup fixture test is exactly the evidence a maintainer needs — but expect a conversation, and don't open it by reinterpreting #105. Same-disk support, genuinely uncertain: it's the largest change, its motivation is external, and it should be framed generically ("install from a partition-based live medium") rather than as plumbing for a third-party app.

---

# 4. Minimal meaningful validation

**Already covered** (per the included tests; reported passing by the primary agent, not observed by me): function-level dispatch, eligibility, protection and BitLocker gating against stubbed topology; real `parted` against disk images for numbering, overlap rejection, protected-row preservation, rollback refusal on a replaced UUID, ESP-by-type protection, MBR identity; real loop devices at 512 and 4096 logical sectors proving a mounted source survives creation, formatting and rollback and that a wrong kernel node is rejected; real `cryptsetup` against upstream BITLK fixtures for accept/reject, byte-immutability and mapping cleanup.

**Unproven, and all reproducible on disposable virtual disks — no physical hardware, no user data:**

1. **Regression sweep on the ordinary path.** Existing `bin/omarchy-iso-test` VM, three boots: plain ISO; `copytoram=y`; `img_dev`/`img_loop` from a second virtual disk. Assert the disk list and full-disk install are unchanged, and that the installer *starts* in every case. This is the direct check on H3 and is the evidence a maintainer will actually want.
2. **The same-disk flow end to end.** Build a QEMU image mirroring what `Staging.ps1` produces — GPT with `[ESP 512 MiB + GRUB][NTFS source with extracted ISO][fake "Windows" NTFS][free space]` — boot it, delete the fake Windows partition through the restricted menu, install, reboot, boot the installed system. `bin/omarchy-iso-test-windows-disk` is already most of this fixture. **This is the single highest-value missing test.**
3. **btrfs-hosted `img_loop`**, to pin whatever degrade behaviour you settle on.
4. **BitLocker round trip in a Windows VM with a virtual TPM** (QEMU swtpm or Hyper-V): suspend → stage → install → boot Windows through the new menu → confirm protection state → boot Windows again. Also test cancel-and-return. This is the only way to validate M4 and the "Windows still boots" claim, and it needs no physical disk.
5. **A 4Kn target**, since `verify_partition_device:134` hard-codes the 512-byte sysfs unit. The loop test covers 4096 *logical* sectors; a `qemu` disk with `physical_block_size=4096,logical_block_size=4096` closes the rest cheaply.

Physical hardware last, and only on a machine whose disk has been imaged first. Nothing in this review is a recommendation to test on a disk with data you care about.

---

# 5. For you, in plain terms

The PR is in better shape than the brief's worry suggests — the protections around your installer partition are well built and genuinely tested. Two things are off, and they pull in opposite directions.

**You're still under-delivering the thing you asked for.** The "delete a partition" menu only appears when there isn't enough free space. If you shrank Windows generously, there's no way to reach it, and full-disk install is gone on that disk — so you can't remove Windows at all. That's a five-line fix: add "Manage partitions" to the menu you already get. While you're there, add a "delete all the others" option; you asked for it twice, it reuses every check that's already written, and it's the difference between four screens per partition and one.

**You're over-delivering somewhere that isn't yours to change.** The PR makes the installer *refuse to start* in several situations where it currently just works — someone booting the ISO from a file on a btrfs disk, or with `copytoram`. None of that has anything to do with installing next to your media, and it's the kind of thing that gets a PR turned down. Make those cases print a warning and carry on.

Two small safety gaps worth closing: deleting a partition checks whether it's mounted but not whether something else is still using it, and if one bookkeeping step fails mid-way the installer can say it cleaned up when it didn't.

Finally, three presentation calls. Split this into three pull requests — the loopback fix first, since it's a real bug in their installer that has nothing to do with you and should land easily. Drop the edit to their Secure Boot plan document; stating what your change does is fine, re-explaining why they made their old decision is not. And change the README's BitLocker instruction to leave the reboot count at the Windows default — then Windows turns protection back on by itself the first time it boots, instead of relying on you remembering.

Sources: [cryptsetup 2.8.2 clear-key support](https://www.phoronix.com/news/cryptsetup-2.8.2), [cryptsetup-bitlkOpen(8)](https://man.archlinux.org/man/cryptsetup-bitlkOpen.8.en), [Arch cryptsetup package](https://archlinux.org/packages/core/x86_64/cryptsetup/), [omarchy-iso PR #105](https://github.com/omacom/omarchy-iso/pull/105)


## External clarification, verbatim

# Addendum

**1. H3 — revised.** You're right and I withdraw the fallback. Clearing `install_media_disk` makes the unidentified source disk an ordinary target at `disk_form:1035-1039`, re-enabling full-disk wipe and cfdisk on it. That is base behaviour, but "base did it" isn't a defence when the PR is already rewriting that code, and my own principle (don't drop safeguards protecting the live source) contradicted my recommendation. Your rule — destructive actions only on targets proven independent of the source, or after positive proof of RAM/remote backing — is the correct one. I also accept the copytoram correction: `ram_backed_live_root` matches current archiso layout, and I demonstrated no broken present-day boot, only a coupling I'd prefer looser. That part was a robustness preference, not a finding.

What I retain, narrowed: H3 drops from High to **Medium, scoped to btrfs-backed and stacked loopback sources**, and the remaining ask is about *how* it fails, not *whether*. Aborting at the top of `disk_form` ends the process before the user sees any actionable screen. Since an unprovable source means no local disk can be offered anyway, the outcome is nearly identical — so make it a clear explanatory screen ("the installer media is on storage this ISO can't pin down; boot from USB or use another machine") rather than a bare abort line. Optional improvement, not required: enumerate btrfs members via `btrfs filesystem show` and exclude those disks, which converts an outright refusal into a working install on genuinely unrelated disks.

**2. M4 — qualified as you propose.** I overstated. "Wrong default" was a claim about outcomes I hadn't tested; the mechanism is documented but the round trip isn't done, and the Entra-joined policy conditions in the FAQ are a real counterexample I didn't account for. Reclassify as: *candidate policy change, to be decided by the Windows/TPM round-trip test.* Explicit resume plus a reminder remains defensible today. The only piece I'd keep as-is: the README must not imply the ISO or app resumes protection for you — which it already states plainly.

**3. M2 — accepted, and my remedy was narrower than the class.** Reordering fixes the newly-added UUID-query gap and the size-tolerance case. It does **not** cover failures between a successful `mkpart` and discovering/verifying `num` (`create_partition`, the `before`/`after`/`comm` block), where there is no number to record. Those need what you describe: a flag set before the write and cleared only on a clean outcome, so rollback reports "a write was attempted and cleanup is incomplete" instead of success. Never guess a number to delete. So: ordering fix = closes B; explicit incomplete-cleanup state = closes the rest.

**4. Splitting and bulk delete — disagreement recorded, not forced.** I still think leading with a standalone source-resolution fix is worth it, and you agree it may make sense. On suspension: the user explicitly chose to include it so ordinary encrypted Windows machines are covered, which is a stronger argument than my acceptance forecast — separate commits in one PR is a reasonable resolution, and my three-PR prescription was a proposal, not a fact. Unresolved: I'd still split if a maintainer stalls. On bulk delete, "about 12 lines" was glib — it needs an explicit preview list, per-item revalidation through the guarded path, and an honest stop on first failure. Offer it that way or not at all.

**Two factual corrections for the record.** Arch shipping cryptsetup 2.8.8 is not proof of the version inside a given built ISO; that artifact still needs checking, and below 2.8.2 the policy fails closed. And I asserted that real `lsblk -d` doesn't emit multi-parent output — I didn't test that. Finding C stays **unproven input, risk not dismissed**.



