# Install without USB: disk map redesign plan

Plan dates: drafted 2026-10-06, decisions settled 2026-10-07.
Based on `f751b32` on `staged-vm-test-fixes`, which is not yet merged into
`main`. Stack this work on that branch until it merges.

## Summary

Rebuild the Windows **Install without USB** screens around a graphical disk map
and cut the text to what each step needs:

1. **Choose space:** a to-scale disk map where clicking a partition chooses the
   space, one line about the selected partition, and a zoomed bar with a
   draggable divider for Omarchy's size. This replaces the partition list, the
   "Use space from" dropdown and, on single-disk PCs, the disk card.
2. **Review:** a screen of its own with Now and After bars drawn from the actual
   plan, a few short bullets and one test-build warning.
3. **After preparing:** a step-by-step guide that shows only the current step.

The work is split into five PRs (0–4). Provider safety checks, plan math and
staging behavior do not change.

**In scope:** `apps/desktop/src/lib/StagedInstaller.svelte` and new components
beside it, two small backend additions, one bug fix, tests, fixtures and docs.

**Out of scope:** the Docker-based direct path (`SetupPanel` with
`StorageControls`), which is not reachable from the current UI, and the
USB flow.

## Decisions

All settled on 2026-10-07:

| Decision | Choice | Reason |
| --- | --- | --- |
| Default target when free space and a shrink both fit | Free space | Changes no partition |
| Mark the Windows partition in the provider | Add `isWindows` | A D: target must not be labelled "Keep Windows" |
| Test-build warning | Badge on the choose screen; full warning once at Review | Choosing space changes nothing on disk, and repeated warnings get skimmed |
| Card after preparing | Redesign last, as its own PR (PR 4) | Separate screen from the disk-map work |

## Problems being fixed

Measured in `test-fixtures/staged-partitions.html` at 1000 px wide:

| Screen | Words | Sizes shown | Text blocks | Decisions |
| --- | --- | --- | --- | --- |
| Choose space | 175 | 10 | 17 | 1 |
| Review | 207 | 12 | 14 | 1 |
| Card after preparing (disclosures open) | 168 | 1 | 9 | 1 per step |

- **The disk bar shows nothing useful.** It is 14 px tall, one color,
  `aria-hidden`, and labelled only by hover tooltips. A typical disk renders as
  one solid bar.
- **The result is never drawn.** It is spread across "Can release",
  "7.5 GiB needed" and the GiB field. It appears in full only as prose at
  Review.
- **One decision uses five controls:** disk cards, partition list, dropdown,
  Keep/Replace radios and a GiB field.
- **Every partition gets a full row.** Manufacturer disks often have 5–7
  partitions, of which one is usable.
- **Limits are hidden or misleading.** The GiB field's limits are not shown.
  "7.5 GiB needed" understates the need when keeping Windows.
- **Unrelated notes share the card.** Secure Boot, BitLocker and "How is this
  space used?" sit with the size choice, although none needs action there.
- **Resize-check progress is far from the partition.** It is printed at the
  bottom of the page.
- **Review stacks below the partition list** instead of replacing it.
- **Bug: Cancel at Review reports a failure.** Declining the review makes the
  helper fail with "Operation was not confirmed". The snapshot becomes
  `failed`, the UI shows "Couldn't complete this step", and **Review changes**
  stays disabled until **Refresh**.
- **The card after preparing** has five long steps, a disclosure and up to four
  buttons of similar weight. It is the last thing read before restarting.

## Goals and success criteria

Measure all numbers with the fixture script described under Testing.

- **Choose space** (happy path): at most 60 words. From the disk heading to
  **Review changes**, at most 480 px tall.
- **Review:** at most 70 words, including the test-build warning.
- **Card after preparing:** at most 60 words in any state.
- **No hover-only information.** Every action and detail is reachable by
  keyboard and screen reader.
- **No horizontal overflow** at the 820 px minimum window or at 390 px (the
  existing check).
- **Current behavior kept:**
  - browsing while limits are measured;
  - keeping the selection across polling and refresh;
  - submitting the exact target;
  - in-app review, copy progress and cancellation;
  - recovery actions.
- **Declining Review** returns to the choose screen with the selection intact
  and no error.

## Design principles

1. **One question per screen.** Choose space asks how much space Omarchy gets
   and whether Windows stays. Everything else moves to where it applies.
2. **Draw it instead of describing it.** Sizes, "kept unchanged", unused space
   and release limits become visual states.
3. **Details follow selection, not hover.** Selecting a segment (click or arrow
   keys) shows one line about it.
4. **Draw only what this app does.** Replacing Windows happens in the
   installer, so the app never draws Windows as deleted.
5. **Use words people already know:** Windows, Omarchy, space. Default text
   avoids EFI, unallocated, alignment, staging and resize limit.
6. **The provider decides; the UI previews.** The editor is an estimate. Review
   shows the plan's numbers. The provider re-validates everything.

## Screen 1: Choose space

```
← Back                                                        Test build
Install without USB
Make room for Omarchy on this computer.

Disk 0 · 954 GiB     Disk 1 · 2 TB · can't use                  Refresh
┌─┬────────────────────────────────────────────────────────────────┬─┐
│▪│ C:  Windows · 953 GiB                                           │▪│
└─┴────────────────────────────────────────────────────────────────┴─┘
C: · NTFS · can give Omarchy up to 92 GiB

┌──────────┬//┬───────────────────┬──────────┃──┬────────────────────┐
│ 773 used │  │ ╳ can't release 80│ free 28.5┃▒▒│ Omarchy 64 GiB      │
└──────────┴//┴───────────────────┴──────────┃──┴────────────────────┘
 Windows 881.5 GiB                     drag ◀▶  installer 7.5 GiB · temporary

[ Keep Windows | Replace Windows ]      Omarchy [ 64 ] GiB · 32–92

▸ All partitions (3)                                 [ Review changes → ]
```

Example numbers come from the fixture: a 954 GiB disk with C: at 953 GiB,
180 GiB free inside C:, a 100 GiB release limit, a 20 GiB reserve and a
7.5 GiB temporary installer.

### Before the disk check

- **No ISO yet:** "Download the image above first."
- **Ready:** **Check available space** button, with the line "Windows will ask
  for administrator access. Nothing is changed."
- **First read, before any disk data:** a placeholder map and "Reading disks…".

### Disk switcher

- **One supported disk:** a heading (`Disk 0 · 954 GiB`) with **Refresh**.
  No card.
- **Several disks:** compact `aria-pressed` buttons, one per disk, as today.
  Disks that can't be used stay listed with "can't use". Selecting one shows
  the provider's reason in place of the map.
- **No disks:** "No internal disks found. Check Disk Management, then refresh."

### Overview map

- **Scale.** Draw to scale, with a minimum width of 10 px for each segment.
- **Grouping.** Neighboring `unavailable` partitions that are each under 1% of
  the disk are grouped into one segment, for example "2 small partitions".
- **Gaps.** Free regions under 5 MiB are not drawn.
- **Selection.** Segments form a radio group, and any segment can be selected.
  Selecting a usable segment also selects it as the space source (free region
  by start offset, shrink by partition GUID). This replaces the dropdown.
- **Labels.** Show a label inside a segment only when it fits. Small segments
  show their label in the selected-partition line.
- **Fill.** Partitions show a used/free fill when `freeBytes` is known.

### Selected-partition line

One line below the overview, from the state table below. A **Recheck** or
**Refresh** text button appears only where the table says so. Resize-check
progress appears here, not at the bottom of the page.

### Space editor (shrink targets)

The editor covers only the selected partition:

- **Used space** collapses to a fixed-width stub (`773 used //`), about 15% of
  the bar. The adjustable range gets the rest.
- **Order follows the provider:** shrunk partition, temporary installer
  (512 MiB EFI plus installer files), then Omarchy's unallocated space, then
  whatever followed the partition. `Get-StagingPlan` places the start at
  `alignDown(partitionEnd) − (temporary + Omarchy)`.
- **Can't-release zone.** The hatched area is `freeBytes − maximumReleaseBytes`.
  Selecting it shows one line built from `reserveBytes`:
  - "Windows keeps 20 GiB free." when the rest is under 1 GiB;
  - "Windows keeps 20 GiB free and can't move some files near the end of C:."
    otherwise.

  This replaces the "Why can't all unused space be used?" disclosure.
- **Handle.** It sits between the partition and the temporary installer.
  Dragging left gives Omarchy more space. Limits:
  - minimum: `ceil(minimumLinuxBytes / GiB)`;
  - maximum: `floor((maximumReleaseBytes − temporaryBytes) / GiB)`;
  - steps of 1 GiB;
  - default 64 GiB, clamped to the limits, as today.
- **Control.** A native `<input type="range" dir="rtl">` spans the adjustable
  range and stays in sync with the number field. If limits change while
  dragging, apply them on release so the handle does not jump.

### Keep or replace

- **Choice.** A two-way toggle: `Keep Windows | Replace Windows`. When the
  target is not the Windows partition (`isWindows` false), use its label, for
  example `Keep D: | Replace D:`.
- **Keep:** shows the editor handle and the number field with its limits
  (`32–92`).
- **Replace:** no size control. The editor shows only the shrink for the
  temporary installer. One line: "In the installer, delete Windows to make room
  for Omarchy." This replaces the "More space needed to finish" warning, which
  reads as an error on a path chosen on purpose.
- **Can't keep** (maximum below minimum): no toggle. One line: "Windows can't
  give enough space to keep it. You can replace it in the installer."

### Free-space targets

- **No handle**, because the provider accepts an Omarchy size only when
  shrinking.
- **Editor:** the temporary installer at the start of the free region, then
  "Omarchy can use the remaining 112 GiB".
- **Below the minimum:** "Only 20 GiB remains; Omarchy needs 32 GiB. Delete a
  partition in the installer to make room."

### No usable space

Where the editor would be: "No space can be used yet. Free up space in Windows,
then refresh." **Review changes** is hidden, as today.

### Segment states

| Region state | Visual | Selected-partition line | Action |
| --- | --- | --- | --- |
| `unavailable`, system partition | dimmed | "EFI system · 260 MiB · kept as is" | none |
| `unavailable`, other reason | hatched | label, size and provider reason | none |
| `insufficient` | hatched | "C: needs 5.4 GiB more free space. Free up space in Windows, then refresh." | Refresh |
| `unchecked` while checking | shimmer (static under reduced motion) | "Checking how much C: can shrink. This can take a few minutes." | none |
| `unchecked`, not checking | dimmed | "C: wasn't checked. Refresh to try again." | Refresh |
| `checked`, enough | normal | "C: · NTFS · can give Omarchy up to 92 GiB" | editor |
| `checked`, short | hatched | "Windows can release only 3 GiB from C:. 4.5 GiB more is needed." | Recheck |
| `failed` | hatched | provider reason, or "Windows couldn't measure C:." | Recheck |
| free, enough | outlined | "Free space · 120 GiB" | editor |
| free, too small | dimmed, outlined | "Free space · 2 GiB · too small" | none |
| group of small partitions | dimmed | "EFI system 260 MiB · Microsoft reserved 16 MiB · kept as is" | none |

### Default selection

Keep the current selection while it still exists, as today. Otherwise choose,
in order:

1. the largest free region whose space after the installer meets the minimum;
2. the shrink target with the highest release limit that can keep Windows;
3. the first usable target;
4. with no usable target, the largest NTFS partition, so the selected-partition
   line explains why.

### All partitions table

A closed disclosure, "All partitions (n)", at the bottom of the screen. Columns:

- name;
- filesystem;
- size;
- free inside;
- status (same text as the selected-partition line).

The footer shows total unallocated space, including small gaps. It is the
exact-numbers view and the text alternative to the map.

### What leaves the choose screen

| Current | New place |
| --- | --- |
| "Experimental" badge and "Test build · real disk changes" disclosure | One "Test build" badge (testing builds only); full warning at Review |
| Disk card ("View partitions and resize options") | Heading or compact button |
| "7.5 GiB needed · For temporary installer files and an EFI loader…" | Editor segment "installer 7.5 GiB · temporary" |
| "0 MiB unallocated" and the small-gaps disclosure | All-partitions table footer |
| Partition list | Map, selected-partition line and All-partitions table |
| "Use space from" dropdown | Selecting a map segment |
| Secure Boot paragraph | One line in PR 1; moves to Review's "Next" bullet in PR 3 |
| BitLocker notice (`BitLockerPreparation.svelte`) | Review bullet; component deleted |
| "How is this space used?" | Removed; the editor shows it |
| "Review changes before applying" | Removed |
| Bottom-of-page resize status | Selected-partition line |

## Screen 2: Review

Review replaces the choose-space content instead of appearing below it.

```
Prepare Disk 0?

Now    ▪│████████████████ C: 953 GiB ████████████████│▪
After  ▪│███████████ C: 881.5 GiB ██████│▒│ Omarchy 64 │▪

• C: shrinks from 953 to 881.5 GiB. Your files stay.
• Omarchy gets 64 GiB. The 7.5 GiB installer is removed later.
• BitLocker on C: pauses until you resume it. Files stay encrypted.
• Next: restart into firmware settings to turn off Secure Boot.

Test build: real disk changes. This ISO may not start, or may refuse this disk.
▸ Full details
[ Cancel ]                                            [ Prepare installer ]
```

- **Bars from the plan.** Draw the After bar from the plan payload, not from
  the editor's estimate. Draw the bars only when the plan matches the inspected
  layout (see the review payload under Data changes). Otherwise show the
  bullets alone.
- **Bullets** are built from the plan payload, as listed in the copy table.
  "Next" uses the inspection's `secureBoot`.
- **Full details** contains the provider's full summary text, which also states
  that Windows Boot Manager and the boot order are kept.
- **Cancel** returns to the choose screen with the selection intact, without an
  error (bug fix in PR 3).
- **While preparing** (after **Prepare installer**): a panel in the same place
  shows the stage message, the copy progress bar and **Stop preparing** when
  available.

## Screen 3: After preparing

This is the "Temporary installer ready" card. It appears once preparation
finishes, and on the home screen whenever a temporary installer exists
(`recoveryOnly`).

```
Temporary installer ready · Disk 0

 ● Turn off Secure Boot   ○ Start installer   ○ Install Omarchy   ○ Clean up

Restart into firmware settings, turn off Secure Boot, then return to Windows.
[ Restart to firmware settings ]

Remove temporary installer · Prepare again
```

Only the current step shows its text and one primary button. The other actions
sit in a secondary text row.

| Operation state | Current step | Text | Primary button |
| --- | --- | --- | --- |
| `staged`/`arming`, Secure Boot on | Turn off Secure Boot | "Restart into firmware settings, turn off Secure Boot, then return to Windows." | Restart to firmware settings |
| `staged`/`arming`, Secure Boot off or unknown | Start installer | "Choose the installer for your next restart." plus the installer instruction below | Start installer on next restart |
| `boot-scheduled`, not restarted | Install Omarchy | "Restart Windows to start the installer." plus the installer instruction below | none (restart Windows) |
| `boot-scheduled`, restarted | Clean up | "If Omarchy starts on its own, resume BitLocker when reminded, then remove the temporary installer." | Remove temporary installer |
| any other status | (no steps) | "Installer preparation needs attention." | Remove temporary installer |

- **Installer instruction**, shown before restarting because the app can't be
  seen from the installer: "In the installer, choose the disk marked "installer
  disk, free space only", then Free space install."
- **Secure Boot unknown:** the first step shows "Turn off Secure Boot if it's
  on", with **Restart to firmware settings** as a secondary button.
- **One disclosure, "Starting Windows later"**, holds:
  - the boot-menu key hint (often F12, F11 or Esc) and `limine-scan`;
  - "If you replace Windows, the temporary installer stays on the disk."
- **Keep current button names** so recovery behavior and tests stay
  recognizable: **Restart to firmware settings**, **Start installer on next
  restart**, **Start installer again on next restart**, **Remove temporary
  installer** and **Prepare again**.
- **`nextAction()`** already picks the current step. Extend it to map states to
  steps.

## Copy

Final English strings. Sizes drop a trailing `.0`: "953 GiB", "881.5 GiB",
"7.5 GiB", "260 MiB".

### Choose space

| Place | Text |
| --- | --- |
| Title | Install without USB |
| Subtitle | Make room for Omarchy on this computer. |
| Badge (testing builds) | Test build |
| Before ISO | Download the image above first. |
| Check button / line | Check available space / Windows will ask for administrator access. Nothing is changed. |
| First read | Reading disks… |
| Disk button | Disk 0 · 954 GiB; Disk 1 · 2 TB · can't use |
| No disks | No internal disks found. Check Disk Management, then refresh. |
| Editor labels | Windows 881.5 GiB · installer 7.5 GiB · temporary · Omarchy 64 GiB · can't release · free |
| Toggle | Keep Windows / Replace Windows (or Keep D: / Replace D:) |
| Size field | Omarchy [n] GiB · 32–92 |
| Replace line | In the installer, delete Windows to make room for Omarchy. |
| Can't keep | Windows can't give enough space to keep it. You can replace it in the installer. |
| Free target | Omarchy can use the remaining 112 GiB. |
| Free target, short | Only 20 GiB remains; Omarchy needs 32 GiB. Delete a partition in the installer to make room. |
| No usable space | No space can be used yet. Free up space in Windows, then refresh. |
| Secure Boot (PR 1 and 2 only) | Secure Boot is on. Leave it on; you'll turn it off after preparing. |
| Button | Review changes |
| Table | All partitions (n) |

### Review

| Place | Text |
| --- | --- |
| Heading | Prepare Disk 0? |
| Shrink | C: shrinks from 953 to 881.5 GiB. Your files stay. |
| Free | Uses free space. Existing partitions stay. |
| Keep | Omarchy gets 64 GiB. The 7.5 GiB installer is removed later. |
| Replace | Adds a 7.5 GiB temporary installer. In the installer, delete Windows to make room. |
| Free target | Omarchy can use 112 GiB. (Below the minimum, use the short free-target line.) |
| BitLocker | BitLocker on C: pauses until you resume it. Files stay encrypted. |
| Next, Secure Boot on | Next: restart into firmware settings to turn off Secure Boot. |
| Next, otherwise | Next: choose the installer for your next restart. |
| Warning, official ISO | Test build: real disk changes. This ISO may not start, or may refuse this disk. |
| Warning, unsigned ISO | Unsigned test image: real disk changes. It may not start or install. |
| Disclosure | Full details |
| Buttons | Cancel / Prepare installer |

### After preparing

Use the texts in the Screen 3 table. Step titles: Turn off Secure Boot, Start
installer, Install Omarchy, Clean up.

## Visual design

- **Sizes:** overview bar 40 px tall, editor bar 56 px, 2 px gaps between
  segments, 3 px radius at the bar ends. Content width stays at most 760 px.
- **Fills:**
  - other partitions: `--surface-raised`, with used space in `--border-strong`;
  - Omarchy: `--accent`;
  - temporary installer: diagonal stripes of `--accent-hover` on
    `--surface-deep`;
  - can't release: hatching in `--border` on `--surface-deep`;
  - free space: dashed `--border-strong` outline;
  - selected segment: 2 px `--accent` outline.

  Add `--map-*` aliases to `styles.css` only if needed.
- **Color use:** `--error` only for real errors. Patterns, not just color,
  distinguish temporary space and space that can't be used.
- **Typography**, three levels:
  - title, 22 px (as now);
  - section headings and key numbers, 13 px; the Omarchy size is the largest
    number on screen, at 20 px bold;
  - body, 12 px, up from 11 px now that there is less text; dim labels 11 px.
- **Patterns:** `repeating-linear-gradient`. No images and no new dependencies.

## Data changes

### 1. Windows partition flag (provider)

In `providers/staged-iso/Staging.ps1`, `Get-StagingChoices`, add to each
partition region:

```powershell
isWindows=[bool]$p.IsBoot
```

`$p` is the `MSFT_Partition` from `Get-InspectedPartitions`, which `Staging.ps1`
already reads `IsBoot` from. The Rust side passes the inspection through as
JSON, so no Rust change is needed. Add `isWindows?:boolean` to `StagedRegion`
in `setup.ts`, and a provider assertion to
`testkit/windows-direct/Test-StagedIso.ps1`.

### 2. Structured review payload (Rust)

In `apps/desktop/src-tauri/src/setup_helper.rs`, move the staged summary
building into a pure function:

```rust
fn staged_review(plan: &Value, linux_bytes: u64, warning: Option<&str>) -> Result<(String, Value), String>
```

It returns the existing summary text and a `review` object built from the same
plan:

```json
{
  "diskNumber": 0,
  "diskSizeBytes": 1024349700096,
  "targetKind": "shrink",
  "shrink": { "label": "C:", "partitionGuid": "…", "offsetBytes": 314572800,
              "beforeSizeBytes": 1023275958272, "afterSizeBytes": 946503417856 },
  "temporary": { "offsetBytes": 946817990656, "sizeBytes": 8053063680 },
  "omarchy": { "offsetBytes": 954871054336, "sizeBytes": 68719476736 },
  "largestFreeAfterBytes": 68719476736,
  "minimumOmarchyBytes": 34359738368,
  "bitLocker": ["C:"],
  "warning": "experimental"
}
```

Field rules:

- `temporary.offsetBytes` is `plan.partitions[0].offsetBytes`.
  `temporary.sizeBytes` is `512 MiB + plan.partitions[1].sizeBytes`.
- `omarchy` is present only when `linuxBytes > 0`. It starts where the
  temporary partitions end.
- `shrink` is `null` for free-space targets.
- `warning` is `"experimental"`, `"unsigned"` or `null`, chosen by the same
  conditions as today's prefix.

Plumbing:

- Add an optional extra field to `confirm()` and emit `review` beside `summary`
  in `confirmation_required`, **outside `plan`**, so the confirmation binding
  hash is unchanged.
- In `setup.rs`, pass `value["review"]` to `review_staged_write`, which stores
  `{"summary": …, "review": …}` in `stagedReview`.
- Add `review?:StagedReviewPlan` to the `stagedReview` type in `setup.ts`.

**UI matching rule.** Draw the Now and After bars only when the plan matches
the inspection the user saw:

- shrink targets: an inspected region has the same `partitionGuid`, offset and
  size as `review.shrink` before the shrink;
- free targets: `review.temporary` starts inside an inspected free region.

Otherwise the layout changed since the check: show the bullets without bars.

### 3. Declined review is not a failure (Rust bug fix)

When `review_staged_write` gets `false`, record that the review was declined.
When the helper then fails with "Operation was not confirmed", `Setup::fail`
reports:

- status `cancelled`;
- no `error`;
- message "Nothing was changed."

Keep `staged_iso` (the inspection) so the choose screen returns intact.
Reset the flag in `begin()`.

No other provider, protocol or helper changes.

## Front-end structure

New files, flat in `apps/desktop/src/lib/` to match the existing layout:

| File | Contents |
| --- | --- |
| `stagedLayout.ts` | Pure, DOM-free model and copy (below) |
| `stagedLayout.test.ts` | Unit tests for it |
| `StagedDiskMap.svelte` | Overview radio group; read-only mode for the Now and After bars |
| `StagedSpaceEditor.svelte` | Zoomed bar, range handle, number field, Keep/Replace toggle |
| `StagedPartitionTable.svelte` | "All partitions (n)" disclosure |
| `StagedReview.svelte` | Review screen and preparing panel |
| `StagedSteps.svelte` | Card after preparing (PR 4) |

`stagedLayout.ts` functions:

- `formatSize(bytes)`: MiB below 1 GiB; otherwise GiB with one decimal and no
  trailing `.0`.
- `regionKey(region)`: partition GUID, or `free:<offset>`.
- `choiceFor(region, choices)`: the `StagedChoice` whose target matches the
  region.
- `overviewSegments(disk)`: grouping, minimum widths and hidden gaps; widths in
  percent.
- `limits(region, temporaryBytes, minimumBytes)`: `{minGiB, maxGiB, canKeep}`.
- `editorSegments(region, {temporaryBytes, omarchyBytes, mode})`: used stub,
  can't-release zone, remaining free, temporary installer and Omarchy, in
  provider order.
- `afterLayout(disk, review)`: the projected regions, or `null` when the
  matching rule fails.
- `defaultRegion(disk, choices, temporaryBytes, minimumBytes)`: the order from
  Default selection.
- `regionStatus(region, context)`: `{tone, text, action}` from the state table.
  All region-state copy lives here.
- `reviewBullets(review, secureBoot)`: Review bullet texts.

`StagedInstaller.svelte` keeps the inspection merge, polling, actions and
recovery handling, and composes the new components:

- **State:** replace `selectedSpace` (a stringified target) with
  `selectedRegion` (a region key). Map it with `choiceFor`.
- **Remove:** `.disk-map`, the partition list, the dropdown, `.space-budget`,
  `.space-details`, the small-gaps disclosure and the bottom-of-page browsing
  status.
- **Delete:** `BitLockerPreparation.svelte` and `BitLockerPreparation.test.ts`.

## Accessibility and narrow windows

- **Overview:** `role="radiogroup"`, labelled "Partitions on Disk 0", with
  roving tab index. Arrow keys move the selection; Home and End jump to the
  ends. Each segment's accessible name is its selected-partition line.
- **Handle:** native range input labelled "Space for Omarchy", with
  `aria-valuetext` such as "Omarchy 64 GiB, Windows keeps 881.5 GiB". Arrow
  keys change 1 GiB, Page Up/Down 10 GiB, Home/End jump to the limits. The
  number field is a separate spinbutton with the same label.
- **Toggle:** a radio group labelled "Windows".
- **Table:** the All-partitions table is the text alternative to the map.
  Review bullets are the text alternative to its bars.
- **Contrast:** check label contrast on `--accent` and stripe fills against the
  Tokyo Night palette.
- **Narrow widths:** below 550 px (the existing breakpoint), segment labels are
  hidden except for the selected segment, and editor labels move to a legend
  row below the bar.

## Testing

### Unit (`stagedLayout.test.ts`)

- Overview widths, minimum widths, grouping and hidden gaps.
- Editor maths matching `Get-StagingPlan` for shrink and free targets, with MiB
  alignment and provider order.
- Limit rounding and `canKeep`.
- Default selection order, including free space first.
- Every row of the segment-state table.
- `afterLayout`, including the mismatch case that returns `null`.
- `formatSize`.

### Component (`StagedInstaller.test.ts`, `AppStaging.test.ts`)

Rewrite text queries as role queries where possible: segment radios, slider,
spinbutton, toggle radios, review region and step buttons.

Keep every current behavioral case:

- browsing during measurement;
- the selected disk preserved across polling and refresh;
- exact target and `linuxBytes` submitted;
- destinations invalidated after a failed refresh;
- an encrypted target with suspension deferred to the confirmed write;
- in-app review;
- copy progress and Stop;
- recovery records and actions;
- prepare again.

Tests that change meaning:

| Current test | New expectation |
| --- | --- |
| testing build shows "Test build · real disk changes" | "Test build" badge |
| tiny gaps moved out of the partition list | gaps not drawn; table footer total includes them |
| partition that can't provide enough space | selected-partition line text and Refresh |
| full disk while Windows measures | checking state on the segment; Review hidden until checked |
| measured shortfall keeps partitions and retry | selected-partition line, Recheck, table present |
| encrypted selection shows BitLocker notice | no notice on the choose screen; BitLocker bullet at Review |
| leave Secure Boot on | PR 1–2: one line; PR 3: Review "Next" bullet; nothing on the choose screen |
| review shown in the app | Review replaces the choose content |
| operation card tests | step mapping (PR 4), same button names |

New cases:

- keyboard selection of segments and the submitted target;
- slider limits, `aria-valuetext` and sync with the spinbutton;
- free space chosen by default when it fits;
- `Keep D:` labels for a non-Windows target;
- bars hidden when the plan doesn't match;
- declining Review returns to the choose screen without an error.

### Rust

- `staged_review`:
  - shrink and keep;
  - shrink and replace;
  - free space;
  - unsigned warning;
  - summary and payload agree on sizes.
- `confirm()` binding unchanged when `review` is added.
- Declined review ends as `cancelled` with no error and keeps `staged_iso`.

### Provider

`Test-StagedIso.ps1`: `isWindows` is true only for the `IsBoot` partition.

### Fixtures

Extend `apps/desktop/test-fixtures/staged-partitions.html` with query
parameters:

- `oem`: EFI, MSR, C:, recovery and a manufacturer tools partition;
- `free`: a usable free region;
- `two-ntfs`: C: and D:;
- `two-disks`: a second disk blocked with an MBR reason;
- `limited` (exists);
- `failed`;
- `replace-only`;
- `review` (exists, plus a review payload);
- `layout=<name>`: loads `test-fixtures/staged-layouts/<name>.json`.

Real layouts:

- Capture 2–4 disk-check results from the test VM and a physical PC with a
  testing debug build. After the check, run
  `await window.__TAURI_INTERNALS__.invoke('setup_status')` in devtools and
  copy `stagedIso`.
- Anonymize disk IDs, GUIDs and volume labels, then save them in
  `test-fixtures/staged-layouts/`.

### Screenshots and measurements

- Extend `artifacts/staged-partition-ux/check.cjs` to:
  - capture each fixture scenario at 1180, 820 and 390 px;
  - assert no horizontal overflow;
  - print word count, number of sizes and the choose-screen height.
- It uses `playwright-core` with the installed Edge channel. Install it in a
  temporary folder rather than adding it to the workspace, to avoid
  dependency-policy work.
- Attach the screenshots and numbers to each PR.

## Delivery

Each PR runs:

- `pnpm desktop:check` and `pnpm desktop:test`;
- `cargo test --workspace --all-features --locked` when Rust changes;
- the provider test script when the provider changes;
- the screenshot script.

Branch names have no tool prefix.

### PR 0: Foundation (no visible change)

Branch: `staged-disk-map-foundation`.

- `stagedLayout.ts` and its tests.
- Provider `isWindows`, its test assertion and the TS type.
- Fixture scenarios, the `staged-layouts/` folder and the extended screenshot
  script.
- Baseline measurements recorded in the PR.

Acceptance: all tests pass; every scenario renders with the current UI.

### PR 1: Disk map

Branch: `staged-disk-map`.

- Disk switcher, overview map, selected-partition line, All-partitions table
  and default selection.
- Remove the list, dropdown, budget line, small-gaps disclosure, BitLocker
  notice (component deleted), "How is this space used?", "Review changes
  before applying" and the bottom browsing status.
- Test-build disclosure becomes the badge. Secure Boot paragraph shortened to
  one line.
- Keep the current Keep/Replace radios and number field, showing the limits,
  the replace line and the free-target lines.
- Review the real layouts before merging.

Acceptance: the choose screen meets the word and height targets except for the
editor; keyboard selection works; no overflow.

### PR 2: Space editor

Branch: `staged-space-editor`.

- Zoomed bar with the used stub, can't-release zone, temporary installer and
  Omarchy.
- Range handle in sync with the number field.
- Keep/Replace toggle and all variants (replace, can't keep, free target,
  non-Windows labels).

Acceptance: the choose screen meets all its targets; the handle respects
limits by pointer and keyboard; submitted `linuxBytes` equals the shown
GiB × 2³⁰.

### PR 3: Review

Branch: `staged-review`.

- `staged_review()` with the `review` payload and its plumbing.
- Declined-review fix.
- `StagedReview.svelte` with Now/After bars, bullets, one warning, Full
  details and the preparing panel.
- Secure Boot moves from the choose screen to the "Next" bullet.

Acceptance: Review meets its word target; bars match the plan; on a mismatch,
bullets show without bars; Cancel returns intact without an error.

### PR 4: Card after preparing

Branch: `staged-steps`.

- `StagedSteps.svelte` with the step mapping, the installer instruction before
  restart and the "Starting Windows later" disclosure.
- Used both in the flow and on the home screen (`recoveryOnly`).

Acceptance: at most 60 words in each state; one primary button; recovery tests
pass with unchanged button names.

## Docs to update

- `docs/staged-iso-first-plan.md`: a dated update note for each PR that changes
  behavior.
- `providers/staged-iso/README.md`: the review description (structured review,
  cancel behavior) and `isWindows`.
- `CHANGELOG.md`: user-visible changes for PRs 1–4.
- `docs/support-matrix.md`: no change expected. The capability does not change.

## Risks

| Risk | Mitigation |
| --- | --- |
| Real layouts read worse than the fixture | Real layouts are required before PR 1 merges; tune grouping and minimum widths with them |
| Limits arrive while dragging | Apply on release; clamp the number field immediately |
| `dir="rtl"` range behaves oddly in WebView2 | Map the value manually and keep the native input for keyboard and screen readers |
| Editor estimate and plan differ by rounding | Review always shows the plan's numbers |
| Disk changes between check and Review | Matching rule hides the bars; the provider re-validates the plan |
| Text queries in tests break widely | Switch to role queries in PR 1, before further copy changes |
