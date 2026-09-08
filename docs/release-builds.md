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

Download packages from the completed workflow's **Artifacts** section. They are
retained for 14 days. This workflow builds previews; it does not publish a GitHub
Release or claim production signing or hardware qualification.

CI sets `OMARCHY_DISTRIBUTION=usb-preview`: it includes download and USB support,
keeps No USB installation disabled, and omits the machine-local Docker image and
direct-install providers. The default maintainer packaging profile still includes
the pinned construction runtime. Preview staging requires a fresh checkout if
full development providers have already been staged.

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
