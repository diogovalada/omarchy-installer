# Omarchy identity assets

These are upstream assets used by the desktop app, not generated substitutes.

- `logo.svg`, `icon.png`, `desktop-preview.png`, `quattro.webp` and
  `winding-road.webp` are copied without modification from
  [omacom/omarchy](https://github.com/omacom/omarchy) at the exact revision
  recorded in `sources.json`.
- The two JetBrains Mono WOFF2 files match those served by
  [omarchy.org](https://omarchy.org/). They are sourced from a pinned revision
  of JetBrains' font repository and bundled for offline use. The SIL Open
  Font License is included from that same source revision.
- `sources.json` records exact source URLs and SHA-256 digests. The upstream
  Omarchy license and font license are retained alongside the assets.
- The wordmark uses the original SVG silhouette with the upstream green color.
  No geometry is redrawn. The website's animated wordmark is intentionally not
  reproduced in this setup utility.
- Colors come from Omarchy's Tokyo Night theme and website `root.css`:
  night `#1a1b26`, storm `#24283b`, blue `#7aa2f7`, green `#9ece6a`,
  bright foreground `#c0caf5`. Secondary text is slightly brighter for
  legibility at desktop UI sizes.

Application icons are generated from `icon.png` using:

```powershell
pnpm --filter @omarchy-setup/desktop exec tauri icon src/assets/omarchy/icon.png --output src-tauri/icons
```

The product continues to identify itself as a community edition. Asset reuse
does not change the project's affiliation.

## Updating the bundle

From the repository root, after installing the normal workspace dependencies:

```powershell
pnpm assets:sync --ref <Omarchy-commit-tag-or-branch>
pnpm assets:check
```

The updater resolves a tag or branch to a full commit, downloads only the
selected files, validates the complete set before writing, records hashes and
lengths, preserves license texts, and regenerates application icons. Fonts keep
their existing pin unless `--font-ref <font-commit-or-tag>` is supplied.
`assets:check` works offline and also runs in frontend CI. Runtime builds and
the installed app never fetch visual assets from upstream.

Review the asset/lock diff and run desktop checks plus visual QA before accepting
an update. Changes to upstream asset paths fail explicitly. Color, typography
and layout decisions still require review; updating images alone cannot adapt
the interface to a redesigned upstream brand automatically.

This deliberately uses a small vendored bundle instead of a Git submodule:
ordinary clones remain self-contained, the shipped bytes are reviewable, and
adopting the app does not require a second checkout of the complete OS source.
If upstream creates a shared branding package, that can replace this bundle.
