# Release builds

[Release builds](../.github/workflows/release-builds.yml) runs on code pushes to
`main`, `v*` tags, and manual dispatch. Each platform uploads its own Actions
artifact with release-mode packages, SHA-256 checksums and build/provider records.
Missing packages or failed provider verification fail the job. Version tags also
run full CI and publish a GitHub Release after both validation and packaging pass.

| Platform | Runner | Packages |
| --- | --- | --- |
| Windows x64 | Windows 2022 | Portable EXE |
| Linux x64 | Ubuntu 24.04 | Portable AppImage |
| macOS Intel | macOS 15 Intel | Portable APP ZIP |
| macOS Apple Silicon | macOS 15 ARM64 | Portable APP ZIP |

The [first manual preview](https://github.com/diogovalada/omarchy-installer/releases/tag/v0.1.0-preview.1)
provides Windows x64 and Linux x64 downloads, SHA-256 checksums and build/provider
records. That historical release did not include macOS packages.

CI packages appear in the completed workflow's **Artifacts** section and are
retained for 14 days. Main-branch builds upload artifacts only. Version-tag builds
publish downloads; neither path establishes production signing or hardware qualification.

Initial validation (2026-09-08): GitHub blocked [all four jobs before startup](https://github.com/diogovalada/omarchy-installer/actions/runs/34245346651)
because of an account billing/spending-limit issue. No release artifacts have
been verified on those runners yet. Local checks passed for actionlint 1.7.12,
the Tauri config, script syntax and Linux preview staging, including native SDK file writing and
rejection of mixed development/preview providers.

Follow-up (2026-09-12): [native CI ran successfully again](https://github.com/diogovalada/omarchy-installer/actions/runs/34696651980)
on Windows, Linux and macOS. That run's failures were spelling and dependency
policy checks; the earlier billing block no longer prevented CI execution.
The [September 12 packaging run](https://github.com/diogovalada/omarchy-installer/actions/runs/34709711534)
subsequently passed for all four platforms. Its packages were uploaded as Actions
artifacts. The September 13 publishing change adds the missing GitHub Release
step; the [dependency-policy review](evidence/dependency-policy-2026-09-13.md)
records the separate CI corrections.

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
launching automatically switches to extraction mode for USB use. On hosts without
FUSE, use `--appimage-extract-and-run` explicitly; see the root README.

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
pnpm release:version 0.1.0-preview.5
```

Use the next unused version; preview.4 is currently prepared. This synchronizes
VERSION, the desktop package.json, Tauri configuration, desktop Cargo.toml and
its Cargo.lock package record. Internal libraries and provider dependencies keep
their independent versions. The command refuses an already tagged version and
does not commit, tag, publish or change released artifacts.

Add the matching entry to [CHANGELOG.md](../CHANGELOG.md), then run:

```sh
pnpm release:check
node --test scripts/ci-changes.test.mjs scripts/release-version.test.mjs scripts/prepare-release.test.mjs
```

CI and application packaging check version consistency. For a release, date the
changelog entry, complete validation, commit the version changes and create the
matching v-prefixed tag (for example v0.1.0-preview.4). Tag builds must match
VERSION exactly. Pushing that version tag requests publication. The packaging
workflow invokes full CI for the tagged source, waits for that and all four
platform packages, verifies every input checksum and build commit/version, and
uploads a draft release. It publishes the draft only after every upload succeeds.
Main pushes continue to produce Actions artifacts without publishing a release.
Preview versions keep **Experimental preview** in the title and notes but use
GitHub's normal release flag and Latest designation so downloads appear in the
repository sidebar. This visibility setting does not imply hardware qualification.

Published release artifacts are not overwritten. If an upload fails and leaves a draft,
inspect that draft before retrying; the workflow deliberately refuses to replace
an existing release automatically. Public releases contain only the four portable
packages and a short link to usage instructions. CI still verifies all platform
checksums, build records and notices before publishing. Those supporting files
remain in the Actions artifacts for their configured retention period; license
notices also remain bundled inside the applications. GitHub displays each public
asset's SHA-256 digest. Published application bytes and tags are not changed when
cleaning up release notes or removing redundant standalone metadata attachments.

Documentation edits need no application version bump. Published previews receive
unique versions; intermediate builds are distinguished by the full commit in
build.json and the commit-qualified Actions artifact name. Versioned package
filenames are not a substitute for those build records. The historical preview.1
release used 0.1.0 inside its packages and is retained unchanged.
