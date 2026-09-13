# Changelog

Versions here belong to Omarchy Installer, independently of the Omarchy image it
downloads. Published versions and their artifacts are immutable.

## [0.1.0-preview.4] - 2026-09-13

- Offer Download replacement or Choose another folder when an existing ISO has
  incorrect bytes. Keep the original until the replacement is verified; refuse
  replacement if the original changes. Access errors and cancellation do not
  authorize replacing a file.
- Simplify downloads to portable Windows EXE, macOS APP ZIP and Linux AppImage.
  macOS no longer includes a DMG, and Linux no longer includes a DEB.
- Automatically relaunch Linux AppImages in extraction mode for USB helper access.
  Hosts without FUSE can use the explicit extraction flag.
- Feature published previews in GitHub's Releases sidebar while retaining the
  Experimental preview title and hardware-testing limitations.

## [0.1.0-preview.3] - 2026-09-13

- Add Intel Mac and Apple Silicon Mac downloads alongside Windows and Linux.
  These previews support downloads and USB preparation; direct installation
  remains in development.
- Publish all four platforms together after full CI, checksum and build-record
  verification. Synchronize application versions and skip heavy CI for prose-only
  changes.
- Include reviewed dependency license notices and policy corrections from the
  unpublished preview.2 candidate.
- Fix duplicate license-notice staging that blocked Windows packaging, and test
  actual staging and payload verification before compiling the Windows release.

## [0.1.0-preview.2] - 2026-09-13

Unpublished candidate: Windows packaging failed. The tag is retained; preview.3
includes these changes and the packaging correction.

- Run lightweight checks for documentation-only changes; keep a final CI result
  for pull requests and run the full suite for code changes or manual checks.
- Synchronize desktop application versions from VERSION and reject release tags
  that disagree with the packaged version.
- Clarify Apple Silicon USB creation status and preserve the direct-install
  alternatives, BitLocker considerations and boot-menu research.
- Publish verified packages for Windows, Linux, Intel Mac and Apple Silicon Mac
  from version tags after full CI passes.
- Declare local dependency versions and document the reviewed certificate-data
  license and public-key-only RSA advisory exception. Include the license notice.

## [0.1.0-preview.1] - 2026-09-08

- First manual preview with a Windows portable executable and Linux AppImage/DEB
  packages, checksums and build/provider records.
- Includes downloads and USB preparation; direct installation is disabled in
  preview packages. macOS packages and further hardware testing remain pending.
- Historical packaging metadata identifies this preview as 0.1.0. Subsequent
  previews use the same full version in the tag and application metadata.
