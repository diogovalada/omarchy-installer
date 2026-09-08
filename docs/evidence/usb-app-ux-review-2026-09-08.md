# USB and app UX review — 2026-09-08

Reviewed the current Svelte application in the Codex in-app browser using native
response fixtures. The fixtures mount the production components and keep every
command in memory. No administrator prompts, USB writes, firmware changes, or
physical installation were performed.

## Findings and fixes

1. **Entry and download:** the two installation methods now explain their target.
   A cancelled existing-file verification is distinguished from a paused download.
   The ready-image guidance requires a completed download with an actual saved path.
2. **USB discovery:** the verified image collapses to the same compact summary used
   by direct installation. Initial drive discovery runs automatically after image
   verification; it never selects a drive or requests elevated compatibility checks.
   Empty-state and refresh labels explicitly refer to USB drives.
3. **Preparation choices and confirmation:** keep-files and erase remain separate,
   with the existing native review-token gate intact. Confirmation no longer asks
   users to choose a drive they already selected. Preserve mode explains that boot
   changes will be reviewed. Administrator wording follows the host platform.
4. **Progress:** the current USB work is visible at the app's minimum window size.
   Pending commands no longer repeat stale ready-state messages. Users are told to
   keep the USB connected through verification.
5. **Cancellation and failure:** a dedicated result explains that the USB is not a
   verified installer and requires fresh discovery before retrying. Partly changed
   drives receive USB-specific guidance. Windows partition/Boot Manager recovery
   instructions are shown only for direct installation.
6. **Completion:** success distinguishes verified writing from verified ejection,
   gives three boot/install steps, and offers Create another USB. Failed or unknown
   ejection does not claim that unplugging is safe. Preserve mode retains its file
   preservation statement and recovery archive reference.
7. **Accessibility and errors:** action errors survive successful background polls
   until the next action; polling-only connection errors clear after reconnecting.
   New setup errors receive focus. Flow entry, drive inspection, cancelled review,
   and Back preserve useful keyboard focus. USB rows form a named radio group.
   Setup body copy is 12px, secondary copy 11px, and normal action buttons at least
   44px high. The existing theme and visible keyboard outlines are preserved.

## Validation and limits

The browser review covered home/download recovery, empty/undersized USBs,
preserve/erase choices, confirmation cancellation, simulated write progress,
interrupted writes, completion, failed ejection, incompatible layouts, persistent
command failures, and the shared Windows direct-install screen. Captures include
1280×720, 1180×790, the native minimum 820×620, and a 410px narrow reflow check.
The narrow check had no horizontal overflow in the inspected controls.

All 30 UI/controller tests passed. Regression coverage includes error persistence/reconnection, explicit native
review gates, fresh discovery after cancellation, ejection truthfulness, and
focus restoration. Svelte checking reports no errors; its four existing warnings
belong to the unused legacy prototype screens/components.

This validates the rendered UI and state handling. It is not physical USB/boot
qualification, a screen-reader certification, or a macOS native-installer audit.

The complete screenshot report and logs are stored locally in
`artifacts/ux-usb-app-2026-09-08/`. The repeatable fixture is
`apps/desktop/test-fixtures/usb-app.html`; it is outside the production entry point.
