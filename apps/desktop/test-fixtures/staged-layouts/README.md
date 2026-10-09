# Captured staged layouts

Disk-check results for `staged-partitions.html?layout=<name>`. Each file is
the `stagedIso` value from `setup_status`.

- `example.json`: synthetic, a 512 GB laptop disk.
- `laptop.json`: a real laptop (2026-10-07). Windows can't shrink C: although
  free space minus the reserve would fit the installer, and the OEM RESTORE
  partition is too small for the installer. No space is usable. The two
  `resizeReason` strings were updated on 2026-10-09 to the provider's current
  wording; the numbers are as captured.

To add a real layout, run the read-only capture script in PowerShell. It
asks for administrator access, like the app's disk check:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/capture-staged-layout.ps1 -Out apps/desktop/test-fixtures/staged-layouts/<name>.json
```

On a PC without the repo, copy `scripts/capture-staged-layout.ps1`,
`providers/staged-iso/Invoke-StagedIso.ps1`, `providers/staged-iso/Staging.ps1`,
`providers/direct-x86/StoragePlan.ps1` and `providers/direct-x86/BitLocker.ps1`
into one folder and run the script there. It replaces disk IDs and partition
GUIDs and prints the volume labels; replace personal labels before committing.

Prefer the script. A testing debug build works too: after the check, run
`copy((await window.__TAURI_INTERNALS__.invoke('setup_status')).stagedIso)` in
devtools. That copy is not anonymized: before committing, replace every
`diskUniqueId` (it often contains the drive's serial number, and sometimes the
computer name) and every partition GUID, consistently across `disks`,
`choices` and `blocked`, as well as personal volume labels.
