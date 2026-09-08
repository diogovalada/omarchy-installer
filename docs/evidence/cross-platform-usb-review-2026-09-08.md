# Linux and macOS USB review

Date: 2026-09-08. Reviewer/implementation model: `gpt-6-astra`.
Scope: the USB erase-and-write paths introduced in `669a220`, including discovery,
system/source exclusions, administrator handoff, protected source/runtime staging,
unmount/open, identity/geometry checks, verification, cancellation and receipts.

## Findings fixed

1. **USB identity could inherit a hub serial.** Linux walked past the drive's USB
   device into hub ancestry when its serial was absent. It now stops at the nearest
   USB device and accepts only that device's USB serial or its own SCSI serial.
   macOS now requests IORegistry class inheritance and recognizes USB subclasses,
   so a child lacking a serial cannot inherit a hub's identity either.
2. **Linux hidden mounts could select the wrong filesystem for unmounting.** Device
   numbers identified the underlying target mount, but a different filesystem
   mounted over the same pathname (or an ancestor directory) could hide it.
   Mount-parent relationships now reject those ambiguous paths before any unmount.
   Source resolution applies the same check, and mount identity is refreshed
   immediately before each unmount command.
3. **macOS lacked a final discovered-mount check.** A mount reported during the
   retained-descriptor identity check was not rejected. Both POSIX paths now abort
   if that discovery reports mounted target volumes, before enabling any writes.
   Linux also retains its independent mount-table check.
4. **An empty macOS USB inventory failed the prerequisite probe.** Successful
   `ioreg` output with no matching objects is now an empty inventory. Malformed
   nonempty plist output still fails.
5. **Linux NTFS-3G mounts have synthetic device numbers.** FUSE block mounts now
   resolve their actual block-device source for exclusions and normal unmounting.
   A FUSE-mounted AppImage gives instructions to use extract-and-run or an installed
   package; its backing storage and privileged execution cannot be assumed safe.
6. **Native CI lacked packaged resources.** The macOS desktop build stopped before
   its tests because the configured providers directory did not exist. CI now
   stages authenticated native providers before compiling desktop tests, and runs
   media/discovery checks first.
7. **macOS disk lookup received file paths instead of volume identifiers.** The
   inspector passed ISO, executable and records-directory paths to diskutil.
   It now resolves their mounted disk with `df -P`, queries diskutil using that
   exact identifier, and rejects inconsistent or non-local results. This also
   avoids guessing APFS firmlink and snapshot mappings from pathname prefixes.
8. **Receipt-export fixtures used macOS's symlinked temporary path.** Tests now
   resolve their temporary parent before exercising valid exports. An additional
   POSIX regression verifies that a symlinked records parent is still rejected
   without creating records; production path checks remain in force.

These fixes preserve exact hardware/geometry matching, administrator confirmation,
source authentication, mandatory SDK verification and full image readback. No
physical device was written or unmounted during this review.

## Validation

- Windows: 69 media tests, 65 passed, four POSIX-only skips, no failures.
- Native Linux under WSL: 69 media tests, 58 passed, 11 Windows-only skips, no failures.
- Native Linux x64 CI: 58 media tests and all 25 native desktop tests passed.
  Real discovery protected the runner's source/system disk; dependency loading,
  packaged-writer and isolated-inspector checks passed in the same validation run
  linked below.
- Native macOS ARM64 CI: 69 media tests, 57 passed, 12 platform-specific skips,
  no failures. All 25 native desktop tests passed, including receipt exports and
  the new linked-parent refusal. Real discovery enumerated eight disks and
  protected the source disk and four system disks; packaged-runtime checks passed.
  Evidence: [review validation run](https://github.com/diogovalada/omarchy-installer/actions/runs/34241835966),
  commit `0dd5434`.
- The final test-fixture helper preserves ordinary Windows paths while resolving
  POSIX temporary paths. Both receipt-export regressions passed locally on Windows.
- Real Linux read-only discovery: six disks enumerated; the source filesystem
  resolved to an excluded disk; the prerequisite probe passed. No eligible USB
  was attached to this Linux environment.
- New CI integration check executes the real POSIX prerequisite and discovery
  commands and requires source/system exclusions, without requiring a USB or
  performing any write, unmount or eject operation.
- The [second review CI run](https://github.com/diogovalada/omarchy-installer/actions/runs/34240922966)
  passed media tests, dependency loading, real discovery, packaged-writer checks
  and provider staging on both Linux and macOS. Its macOS desktop tests exposed
  the temporary-path fixture issue above. Its Windows native job could not install
  the pinned native dependencies because node-gyp did not find a compatible Visual
  Studio installation on the runner.
- The earlier green run was a separate Dependabot workflow and is not native test
  evidence. The [first review CI run](https://github.com/diogovalada/omarchy-installer/actions/runs/34240345975)
  exposed the missing-resource issue above. Repository-wide spelling and license
  policy checks also reported failures in existing files/dependencies outside this
  USB change; they are not evidence of a successful full CI run.

Regression fixtures cover a serial-less drive behind a hub, macOS USB subclasses,
overmounts at the target or an ancestor, mount replacement before unmount, and
mount detection after raw-device acquisition. The new pre-write refusal tests
require zero device writes and closed handles.

The native locking interpretation was checked against Apple's
[IOMediaBSDClient source](https://github.com/apple-oss-distributions/IOStorageFamily/blob/main/IOMediaBSDClient.cpp):
`O_EXLOCK` requests storage-level exclusive access. The class-inheritance handling
uses the `IOObjectInheritance` array emitted by Apple's
[ioreg implementation](https://github.com/apple-oss-distributions/IOKitTools/blob/main/ioreg.tproj/ioreg.c).

Physical USB preparation and boot on Linux/macOS remain unqualified. No full
desktop release executable was rebuilt during this review.
