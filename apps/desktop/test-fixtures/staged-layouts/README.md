# Captured staged layouts

Disk-check results for `staged-partitions.html?layout=<name>`. Each file is
the `stagedIso` value from `setup_status`. `example.json` is synthetic: a
512 GB laptop disk.

To add a real layout:

1. Run a testing debug build, open **Install without USB** and finish the disk
   check.
2. In devtools, run
   `copy((await window.__TAURI_INTERNALS__.invoke('setup_status')).stagedIso)`.
3. Replace disk unique IDs, partition GUIDs and volume labels with neutral
   values, then save the result here as `<name>.json`.
