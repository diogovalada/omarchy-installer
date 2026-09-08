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

These fixes preserve exact hardware/geometry matching, administrator confirmation,
source authentication, mandatory SDK verification and full image readback. No
physical device was written or unmounted during this review.

## Validation

- Windows: 66 media tests, 62 passed, four POSIX-only skips, no failures.
- Native Linux under WSL: 66 media tests, 55 passed, 11 Windows-only skips, no failures.
- Real Linux read-only discovery: six disks enumerated; the source filesystem
  resolved to an excluded disk; the prerequisite probe passed. No eligible USB
  was attached to this Linux environment.
- New CI integration check executes the real POSIX prerequisite and discovery
  commands and requires source/system exclusions, without requiring a USB or
  performing any write, unmount or eject operation.
- The [previous commit's CI run](https://github.com/diogovalada/omarchy-installer/actions/runs/34239289467)
  passed, including native macOS/Linux desktop tests, file-backed media tests and
  isolated staged-runtime loading. This is software evidence, not hardware testing.

Regression fixtures cover a serial-less drive behind a hub, macOS USB subclasses,
overmounts at the target or an ancestor, mount replacement before unmount, and
mount detection after raw-device acquisition. Failure tests require zero device
writes and closed handles.

The native locking interpretation was checked against Apple's
[IOMediaBSDClient source](https://github.com/apple-oss-distributions/IOStorageFamily/blob/main/IOMediaBSDClient.cpp):
`O_EXLOCK` requests storage-level exclusive access. The class-inheritance handling
uses the `IOObjectInheritance` array emitted by Apple's
[ioreg implementation](https://github.com/apple-oss-distributions/IOKitTools/blob/main/ioreg.tproj/ioreg.c).

Physical USB preparation and boot on Linux/macOS remain unqualified. No full
desktop release executable was rebuilt during this review.
