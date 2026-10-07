# Captured staged layouts

Disk-check results for `staged-partitions.html?layout=<name>`. Each file is
the `stagedIso` value from `setup_status`.

- `example.json`: synthetic, a 512 GB laptop disk.
- `laptop.json`: a real laptop (2026-10-07). Windows can't shrink C: although
  free space minus the reserve would fit the installer, and the OEM RESTORE
  partition fails the precheck. No space is usable.

To add a real layout, run the read-only capture script in PowerShell. It
asks for administrator access, like the app's disk check:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/capture-staged-layout.ps1 -Out apps/desktop/test-fixtures/staged-layouts/<name>.json
```

On a PC without the repo, copy `scripts/capture-staged-layout.ps1`,
`providers/staged-iso/Invoke-StagedIso.ps1`, `providers/staged-iso/Staging.ps1`,
`providers/direct-x86/StoragePlan.ps1` and `providers/direct-x86/BitLocker.ps1`
into one folder and run the script there. It replaces disk IDs and partition
GUIDs; replace personal volume labels before committing.

A testing debug build works too: after the check, run
`copy((await window.__TAURI_INTERNALS__.invoke('setup_status')).stagedIso)` in
devtools.
