# Windows USB raw-device open failure — 2026-09-07

Implementation/diagnostic model: `gpt-6-astra`.

The user's operation `0ec95118-2a00-4a25-918c-6c590a676548` failed after
preparation, authentication and consent. Its preserved `failure.json` recorded
`EIO: i/o error, open '\\.\PhysicalDrive1\'`, including the trailing slash.
The raw open in `HeldSdkDevice.acquire` precedes `enableWrites` and all SDK
destination writes. No diagnosis step retried the write operation.

Read-only elevated diagnostics matched the selected silicon-power USB by serial
(redacted), capacity 8,015,314,944 bytes, USB bus, and non-boot/non-system flags.
The disk remained online, writable and healthy according to Get-Disk. Its existing
partition was still present. These observations are not a media health test.

Direct Win32 `CreateFileW` probes used GENERIC_READ and shared read/write access.
With each of flags 0, NO_BUFFERING, WRITE_THROUGH and both flags:

| Device path | Result |
| --- | --- |
| `\\.\PhysicalDrive1` | Open and close succeeded |
| `\\.\PhysicalDrive1\` | Failed with Win32 error 31 (mapped to EIO) |

The packaged Node v22.13.1 reproduced the extra slash using
`path.toNamespacedPath`. Its `src/node_file.cc` calls `ToNamespacedPath` inside
`OpenFileHandle`; `src/path.cc` resolves and overwrites the input path, including
device names. Buffer input also goes through that native conversion and is not
a reliable workaround.

The fix uses the same `PhysicalDriveN` link in the global DOS-device namespace via
`\\?\GLOBALROOT\GLOBAL??\PhysicalDriveN`, so Node sees the disk name as a leaf.
It leaves the consent/discovery identity and locking/verification policy intact.
Regression checks cover namespace normalization, invalid input rejection,
unchanged POSIX names, and actual read-only fs.open of a nonexistent disk.

An additional elevated Node read-only device probe was cancelled at UAC. It was
not retried with elevation. Running the same probe without elevation returned
EPERM for all paths, as expected; both original string and Buffer inputs appeared
with the unwanted trailing slash in Node's error, and the corrected alias did not.
The corrected path has not yet been qualified by writing a physical
USB or by a completed elevated Node open on this device. Do not describe this as
an end-to-end burn test or remove the hardwareQualified=false receipt field.

Primary references:

- [Node v22.13.1 native fs implementation](https://github.com/nodejs/node/blob/v22.13.1/src/node_file.cc)
- [Node v22.13.1 namespace conversion](https://github.com/nodejs/node/blob/v22.13.1/src/path.cc)
- [Microsoft device and NT namespace definitions](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file#nt-namespaces)

Local diagnostic records are in `artifacts/usb-open-diagnostic/` and are not part
of the distributed application.
