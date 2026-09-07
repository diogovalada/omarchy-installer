# Single-screen setup — design QA

final result: passed

The user approved a single setup screen: compact Omarchy branding, one download
area with destination and progress, and USB/install actions underneath. The
sidebar, separate overview/download pages, slogans and repeated verification
explanations were intentionally removed. Unsupported actions remain disabled.

## Evidence

- Source visual truth: the existing Omarchy identity and the user-approved
  simplification of `docs/evidence/simplification-audit-2026-09-06/01-overview.jpg`
  and `02-download.jpg`.
- Implementation URL: http://127.0.0.1:1420/.
- Implementation screenshot:
  `docs/evidence/single-screen-2026-09-06/setup-desktop.jpg`.
- Full-view combined comparison:
  `docs/evidence/single-screen-2026-09-06/before-after.jpg`.
  The old and new screens were inspected together in this composite.
- Desktop source and implementation captures: 1280 × 720 pixels each, same
  browser-preview starting state and dark theme. Implementation CSS viewport:
  1280 × 720. Composite: 2560 × 720; both sides use the same scale.
- Additional captures: `setup-compact.jpg` at 433 × 746 and
  `setup-minimum.jpg` at 820 × 620, in the same evidence directory.
- Focused crops were not needed: individual full-size captures make the
  typography, thumbnail, controls and spacing readable. This is an intentional
  layout simplification, not a pixel-exact clone of the earlier overview.

## Required fidelity surfaces

- **Typography:** original bundled JetBrains Mono and upstream wordmark remain.
  Browser font loading passed. A single functional heading replaces the slogans.
- **Spacing/layout:** one centered column with the two subsequent actions below.
  Desktop and compact captures fit without overflow; the minimum desktop size
  scrolls vertically but all four controls remain above its initial fold.
  No horizontal overflow was measured.
- **Colors:** existing Tokyo Night tokens remain, with blue for downloading and
  green plus the word Verified for successful native completion.
- **Images:** the original Quattro wallpaper is a small, uncropped 16:9 thumbnail.
  The upstream wordmark geometry is unchanged. No replacement artwork was drawn.
- **Copy:** version/architecture/size, destination, progress and actions are the
  primary content. Hash and signature explanations are collapsed into Details.
  One shared sentence explains the unavailable USB/install actions.

## Findings and iteration history

No actionable P0/P1/P2 visual findings remain. The first complete visual
comparison passed; no visual fixes were required after that comparison.

A functional state test found that assigning undefined to a Svelte progress
value produced value=0 during verification. Separate determinate and
indeterminate progress elements fixed this; the regression test now passes.
This was a functional correction, not a visual QA iteration.

An intermediate browser screenshot was blank despite a populated DOM. It was
rejected and replaced with a valid capture through the same browser's CUA
screenshot API; only inspected captures are included in the evidence.

## Verification

- Frontend: 11 tests passed, including explicit download, destination chooser
  dispatch, pause/resume, 100% transfer versus verification, completion requiring
  a saved path, error display and unavailable-action gating.
- Native: 5 tests passed, including folder-change validity, rejection during an
  operation, clearing prior completion and preserving existing saved files.
- Svelte check: zero errors; four existing warnings in unused prototype files.
- Final production frontend and Windows native debug build succeeded.
- Browser console error/warning query returned no entries.
- The native command accepts the folder selected by the OS dialog; it accepts
  no path from JavaScript. Downloads is the default. Existing verified cache
  data can be reused when exporting to a new destination.

## Limits and follow-up

This pass visually checked the browser preview and tested native state logic.
It did not operate the native folder dialog or repeat a multi-gigabyte native
download. Native transfer states were checked through component tests, not
browser screenshots. Screen-reader announcements and full native keyboard
behavior have not been manually verified.

USB writing and direct installation remain unimplemented in this desktop
build, so a verified download cannot enable them yet. No inaccessible next-step
screens or fake device choices were added.

## Implementation checklist

- [x] Remove duplicate navigation and introductory copy.
- [x] Combine download, destination, progress and next actions.
- [x] Add native folder selection and retain resumable cache behavior.
- [x] Show Verified only after native completion and saving.
- [x] Keep verification details collapsed.
- [x] Check responsive layouts, state transitions, tests and native build.

Previous identity-refresh QA is preserved at
`docs/evidence/design-2026-09-06/design-qa.md`.
