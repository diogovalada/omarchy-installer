# Omarchy identity refresh — design QA

final result: passed

## Findings

No actionable P0/P1/P2 findings remain in the visual identity refresh.

This is an adaptation of Omarchy's identity to the existing setup application,
not a reproduction of the public homepage. The source is a marketing page;
the implementation is an installer overview. Their copy, navigation, and
composition intentionally differ. The static green upstream wordmark replaces
the website's animated treatment; the original SVG geometry is retained.

## Evidence and comparison conditions

- Source visual truth: https://omarchy.org/ and
  `docs/evidence/design-2026-09-06/official-reference-compact.jpg`.
- Implementation: http://127.0.0.1:1420/ and
  `docs/evidence/design-2026-09-06/overview-compact.png`.
- Combined comparison: `docs/evidence/design-2026-09-06/identity-comparison.png`.
  Source and implementation were viewed together in this composite.
- Both compact pages used a 433 × 746 CSS viewport. The source browser reported
  devicePixelRatio 1.75 and its screenshot service returned 418 × 720 pixels;
  the implementation reported approximately 1 and returned 433 × 746 pixels.
  The source capture was resampled to 433 × 746 using Lanczos before comparison.
  The small service scaling difference was not treated as design drift.
- State: public homepage versus setup overview, dark theme, compact navigation
  closed. No authenticated state is required.
- Desktop evidence at 1180 × 790 CSS/capture pixels:
  `overview-desktop.png`, `download-desktop.png`, `install-desktop.png`, and
  `usb-desktop.png`, all under the evidence directory above.
- Minimum desktop window: `overview-minimum.jpg`, 820 × 620 CSS/capture pixels.
- Compact download: `download-compact.png`, 433 × 746 pixels.
- Separate focused crops were unnecessary: typography, wordmark geometry,
  palette, and image quality are readable in the normalized compact composite
  and the full desktop captures. This compares identity, not pixel-exact
  marketing-page placement.

## Required fidelity surfaces

- **Fonts and typography:** locally bundled JetBrains Mono regular and bold
  match the official font files. Browser font loading passed. Mono hierarchy,
  line wrapping, and small navigation labels remain readable at the checked
  widths. The installer uses larger functional headings than the homepage.
- **Spacing and layout rhythm:** flat panels, small corner radii, restrained
  borders, and consistent spacing carry through the four routes. The 1180 × 790
  overview fits without content overflow. At 820 × 620 and 433 × 746, content
  scrolls within the main region while persistent controls remain available;
  no horizontal overflow was measured. Compact navigation opens and closes.
- **Colors and tokens:** Tokyo Night backgrounds, blue active controls, green
  wordmark/icon, and pale foregrounds replace the previous yellow/Inter design.
  Secondary text is intentionally brighter than the website's subdued labels
  to support small desktop controls.
- **Image quality and asset fidelity:** the original Quattro and winding-road
  wallpapers, actual desktop screenshot, upstream SVG wordmark, and upstream
  application icon are used. The overview preserves the wallpaper's aspect
  ratio. No substitute logo geometry or generated imitation was introduced.
- **Copy and content:** setup actions retain their task-specific copy and
  availability labels. Browser preview explains unavailable native downloads;
  direct installation remains marked as coming soon. Community edition wording
  continues to identify the project's affiliation accurately.

## Comparison history

1. Initial implementation review found two P2 issues visible in
   `overview-first-pass.png`: an unquoted SVG mask URL produced a solid green
   rectangle, and the desktop overview overflowed vertically.
2. Quoting the mask URL restored the source silhouette. Adjusting the hero
   grid proportions, main padding, and section gap removed the desktop overflow.
3. Post-fix `overview-desktop.png` shows the corrected wordmark and complete
   overview. The normalized source/implementation composite confirms the
   identity surfaces after these fixes. Compact and minimum-window checks found
   no additional actionable visual issues.

## Verification and limits

- Asset refresh against the pinned Omarchy revision succeeded; offline asset
  verification succeeded. Hashes, source revisions, and licenses are bundled.
- Svelte check: zero errors; four existing unused-component warnings.
- Frontend tests: 8 passed.
- Production frontend and Windows native debug builds succeeded.
- Tested overview, download, USB, and install route navigation, the install
  screen's download action, and compact drawer opening/closing on selection.
- Browser console warning/error query returned no entries.
- This pass checked browser-rendered UI and native compilation. It did not
  repeat native download or physical USB operations, or visually inspect the
  rebuilt native window. Those are outside this visual refresh's QA evidence.

## Implementation checklist

- [x] Reuse upstream artwork and fonts with licenses and provenance.
- [x] Apply the palette and typography across existing routes.
- [x] Correct wordmark rendering and desktop overflow.
- [x] Compare source and final implementation together.
- [x] Check compact and minimum desktop layouts and navigation.
- [x] Validate assets, frontend checks/tests, and native build.

## Follow-up polish

No blocking visual work remains. Any future shared branding package from
upstream can replace the small bundled asset set and its explicit refresh script.
