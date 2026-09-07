# Windows USB alignment-query compatibility, 2026-09-07

Implementation and validation: **gpt-6-astra**.

The user reported `IOCTL_STORAGE_QUERY_PROPERTY failed` after source
authentication and before USB writing. The pinned direct-io implementation
required `StorageAccessAlignmentProperty`, and discarded the Win32 error code.

A read-only metadata handle on the attached 8 GB USB reproduced:

| Query | Result |
| --- | --- |
| `StorageAccessAlignmentProperty` | Win32 error **1**, `ERROR_INVALID_FUNCTION` |
| `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX` | Success, 112 bytes returned |
| Handle capacity / logical sector | **8,015,314,944 bytes / 512 bytes** |
| Fresh `Get-Disk` logical / physical sectors | **512 / 512 bytes** |

The probe requested metadata access only. It did not write, dismount, lock,
format, eject, or change partitions. Hardware serials and raw diagnostic output
remain in ignored local artifacts.
Repeating with the native library's exact 40-byte geometry buffer also
succeeded and returned 40 bytes with the same capacity and logical sector size.
An additional read-only check through Node required elevation; the UAC prompt
was canceled, so that check did not execute and was not retried.

Windows builds now reproducibly patch and rebuild `@ronomon/direct-io 3.0.1`.
The patch only accepts errors 1 and 50 as unsupported alignment queries. It
still requires a valid capacity and logical sector size from the retained raw
descriptor, and reports an explicit missing physical sector value. The provider
reidentifies the disk while holding that descriptor and resolves the missing
value from Windows' current disk information. All resolved values must match
the originally selected identity before enabling writes. A physical size is
never guessed from the logical size. Native query failures now retain their
Win32 error numbers.

The native production query code is exercised against 14 mocked Windows
responses, including the observed error, valid geometry, permission/device/I/O
failures, truncated results, nonzero alignment offsets, and conflicting sector
sizes. TypeScript tests cover fallback reconciliation, changed/unsafe disks,
invalid geometry, and supported native responses. `npm test` runs these along
with the existing SDK file-write, verification, cancellation, path, and source
handoff checks. Windows native tests require the MSVC tools already needed to
build the native provider dependencies.
The final provider suite passed **29/29 tests**, including all 14 native query
scenarios, on this Windows host.

The upstream lockfile remains pinned, the original MIT license is retained, and
the patched source and binary are covered by the packaged runtime manifest.
Staging and the portable bundle check require the native patch version marker.
Full physical USB writing and boot qualification remain untested by this fix.

Microsoft documents the query's alignment fields in
[STORAGE_ACCESS_ALIGNMENT_DESCRIPTOR](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddstor/ns-ntddstor-_storage_access_alignment_descriptor)
and the alternate OS sector properties in
[MSFT_Disk](https://learn.microsoft.com/en-us/windows-hardware/drivers/storage/msft-disk).
