# Changelog

Versions here belong to Omarchy Installer, independently of the Omarchy image it
downloads. Published versions and their artifacts are immutable.

## Unreleased

- Install without USB (testing builds): choose space on a to-scale disk map.
  Selecting a partition explains it in one line; an "All partitions" table
  gives exact sizes. A non-Windows partition is named by its drive letter in
  the Keep and Replace choices.
- Install without USB (testing builds): a zoomed bar shows how the chosen
  space will be laid out, with a handle that sets Omarchy's size in step with
  the size field. Keep and Replace are a two-way toggle, and selecting the
  space Windows can't release explains why.
- Install without USB (testing builds): Review replaces the choose screen with
  Now and After bars drawn from the actual plan, four short bullets and one
  test-build warning; the full summary is under "Full details". Preparation
  progress shows in the same place. Cancelling Review returns to the choose
  screen with the selection intact instead of reporting a failure. The Secure
  Boot advice moved from the choose screen to Review's "Next" bullet.
- Install without USB (testing builds): a prepared temporary installer shows
  four steps (Turn off Secure Boot, Start installer, Install Omarchy, Clean up)
  with only the current one spelled out and one primary button; other actions
  sit in a text row. The installer-disk instruction shows before restarting,
  and boot-menu tips move to "Starting Windows later". While a temporary
  installer exists, its steps replace the choose screen.
- Install without USB (testing builds): design review fixes. The space editor
  says what stays with Windows ("Windows (C:) keeps 881.5 GiB") and uses
  plainer labels; clicking a label no longer changes the size. With no usable
  space, "What can help?" suggests next steps and the USB route. Replace-only
  shows why Windows can't be kept. Review names the installer and Omarchy in
  its bars. The after-preparing card has clearer steps and help for turning
  off Secure Boot.
- Install without USB (testing builds): fixes from a full VM run. Cancelling
  Review says "Nothing was changed."; the card no longer says preparing
  "didn't finish" while it is still running, and says when a removal is under
  way; the home screen confirms a removal; status messages no longer
  contradict the current step; the Review wait and the disk check's Stop
  button are labelled for what they do.
- Install without USB (testing builds): declining the firmware, next-restart
  or removal confirmation says "Nothing was changed." instead of reporting a
  failure; the removal message survives reloading the window; the home
  screen's steps know whether Secure Boot is on.
- Install without USB (testing builds): a partition without enough free space
  also says how much keeping Windows needs; removing the temporary installer
  reads plainly, shows as under way only once confirmed, and reminds you to
  resume BitLocker if it's still paused.

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
