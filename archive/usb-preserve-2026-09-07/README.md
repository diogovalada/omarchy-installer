# Historical USB preservation and deferred restoration

The user clarified that only **Restore previous boot setup** should be deferred.
The keep-files UI, installer, verified boot backup, loader and qualification
assets have been restored to active source. Restoration remains excluded from
the UI, IPC protocol and native helper.

See [current scope](../../docs/usb-preserve-implementation-2026-09-07.md).
The files below are historical snapshots; do not copy their restore integration
into the active app unless the user asks to resume that feature.

## Contents

- `apps/`: dormant Rust implementation and frontend tests.
- `providers/usb-preserve/`: loader, corresponding GNU GRUB source and licenses,
  reproducible build recipe, virtual boot qualification scripts and evidence.
- `integration-snapshot/`: exact pre-archive copies of files that wired the
  experiment into the UI, elevated helper, protocol and packaging, plus design docs.

These archived copies are outside active Rust modules, frontend test discovery, and provider
staging. Selected keep-files assets also have active copies. Integration snapshots are historical references, not files to copy over
newer app code wholesale. Existing preview executables are retained as historical
artifacts; bebf264d and 3099c1ff contain the experiment and are superseded.

## Status when paused

18 native and 17 frontend tests passed; the official Omarchy 4.0.2 ISO reached its
installer start screen from a GPT virtual USB with NTFS data and FAT32 EFI under
QEMU/OVMF. No physical USB qualification was performed. Saved evidence does not
establish compatibility with ordinary single-partition USBs or other layouts.

Resume only when requested. Reconcile the snapshots with the current app and
reassess identity binding, interrupted writes, bootloader restoration, physical
USB compatibility, and source/license packaging before re-enabling the feature.
