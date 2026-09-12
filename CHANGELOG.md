# Changelog

Versions here belong to Omarchy Installer, independently of the Omarchy image it
downloads. Published versions and their artifacts are immutable.

## [0.1.0-preview.2] - Unreleased

- Run lightweight checks for documentation-only changes; keep a final CI result
  for pull requests and run the full suite for code changes or manual checks.
- Synchronize desktop application versions from VERSION and reject release tags
  that disagree with the packaged version.
- Clarify Apple Silicon USB creation status and preserve the direct-install
  alternatives, BitLocker considerations and boot-menu research.

## [0.1.0-preview.1] - 2026-09-08

- First manual preview with a Windows portable executable and Linux AppImage/DEB
  packages, checksums and build/provider records.
- Includes downloads and USB preparation; direct installation is disabled in
  preview packages. macOS packages and further hardware testing remain pending.
- Historical packaging metadata identifies this preview as 0.1.0. Subsequent
  previews use the same full version in the tag and application metadata.
