# Release builds

[Release builds](../.github/workflows/release-builds.yml) runs on code pushes to
`main`, `v*` tags, and manual dispatch. Each platform uploads its own Actions
artifact with release-mode packages, SHA-256 checksums and build/provider records.
Missing packages or failed provider verification fail the job.

| Platform | Runner | Packages |
| --- | --- | --- |
| Windows x64 | Windows 2022 | Portable EXE |
| Linux x64 | Ubuntu 24.04 | AppImage and DEB |
| macOS Intel | macOS 15 Intel | DMG and APP ZIP |
| macOS Apple Silicon | macOS 15 ARM64 | DMG and APP ZIP |

The [first manual preview](https://github.com/diogovalada/omarchy-installer/releases/tag/v0.1.0-preview.1)
provides Windows x64 and Linux x64 downloads, SHA-256 checksums and build/provider
records. macOS packages are pending access to a macOS build machine.

CI packages appear in the completed workflow's **Artifacts** section and are
retained for 14 days. The workflow does not publish GitHub Releases or claim
production signing or hardware qualification.

Initial validation (2026-09-08): GitHub blocked [all four jobs before startup](https://github.com/diogovalada/omarchy-installer/actions/runs/34245346651)
because of an account billing/spending-limit issue. No release artifacts have
been verified on those runners yet. Local checks passed for actionlint 1.7.12,
the Tauri config, script syntax and Linux preview staging, including native SDK file writing and
rejection of mixed development/preview providers.

Follow-up (2026-09-12): [native CI ran successfully again](https://github.com/diogovalada/omarchy-installer/actions/runs/34696651980)
on Windows, Linux and macOS. That run's failures were spelling and dependency
policy checks; the earlier billing block no longer prevented CI execution.
The packaging workflow still needs a successful run. Its old failure is not
evidence of a current billing block or a macOS compilation defect.

Local release validation (2026-09-08): the Windows portable EXE passed extraction,
provider hashes and a bundled SDK write/readback check. Both Linux packages were
extracted and all 3,292 provider files matched their compiled manifest; each
extracted runtime passed the SDK write/readback check. The AppImage passed a
30-second headless launch check under Ubuntu 24.04 with Xvfb, software rendering
and temporary desktop-folder settings. Physical Linux USB and boot testing remain
pending.

CI sets `OMARCHY_DISTRIBUTION=usb-preview`: it includes download and USB support,
keeps No USB installation disabled, and omits the machine-local Docker image and
direct-install providers. The default maintainer packaging profile still includes
the pinned construction runtime. Preview staging requires a fresh checkout if
full development providers have already been staged.

Linux packaging uses `node scripts/build-linux-packages.mjs`. It bundles the GUI
dependencies first, then adds the authenticated USB runtime before the final
AppImage packing step. This prevents linuxdeploy from rewriting provider binaries
or trying to resolve dependencies of SDK binaries for other platforms.

Windows builds are unsigned. macOS builds use ad-hoc signing without notarization;
the separately signed Apple direct-install bridge is not included. macOS 15+ and
Ubuntu 24.04 or compatible newer Linux are the current package baselines. AppImage
USB use requires `--appimage-extract-and-run`; see the root README.

The workflow checks frontend types/tests, USB tests and native dependency loading.
It verifies provider hashes and runs an ordinary-file SDK write from packaged
Linux/macOS resources. Windows packaging verifies its self-extraction and bundled
runtime. These checks do not write physical disks or prove installation/boot.

Build references: [Tauri packaging](https://github.com/tauri-apps/tauri-action),
[macOS preview signing](https://tauri.app/distribute/sign/macos/), and
[GitHub runner platforms](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

## CI for documentation and code changes

CI still starts for pushes to main and pull-request updates. A lightweight job
compares the complete push or the pull request against its merge base. Prose-only
changes run metadata/helper checks and spelling; code, configuration and image
changes run the full suite. Unknown history, new branches and manual dispatch
also run the full suite. Rename detection is disabled so moving code into a
documentation path cannot conceal the code deletion.

The final **CI result** job always runs and requires all applicable checks to
succeed. Use it as the required branch-protection check when configuring that
policy. This change does not modify repository branch-protection settings.
The release-packaging workflow separately skips documentation-only main pushes.

## Preparing a version

[VERSION](../VERSION) is the canonical desktop application version. To prepare
the next preview, run from the repository root:

```sh
pnpm release:version 0.1.0-preview.3
```

Use the next unused version; preview.2 is currently prepared. This synchronizes
VERSION, the desktop package.json, Tauri configuration, desktop Cargo.toml and
its Cargo.lock package record. Internal libraries and provider dependencies keep
their independent versions. The command refuses an already tagged version and
does not commit, tag, publish or change released artifacts.

Add the matching entry to [CHANGELOG.md](../CHANGELOG.md), then run:

```sh
pnpm release:check
node --test scripts/ci-changes.test.mjs scripts/release-version.test.mjs
```

CI and application packaging check version consistency. For a release, date the
changelog entry, complete validation, commit the version changes and create the
matching v-prefixed tag (for example v0.1.0-preview.2). Tag builds must match
VERSION exactly. Publishing downloads remains a deliberate release step; the
packaging workflow only uploads build artifacts.

Documentation edits need no application version bump. Published previews receive
unique versions; intermediate builds are distinguished by the full commit in
build.json and the commit-qualified Actions artifact name. Versioned package
filenames are not a substitute for those build records. The historical preview.1
release used 0.1.0 inside its packages and is retained unchanged.
