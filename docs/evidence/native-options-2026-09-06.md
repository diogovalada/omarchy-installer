# Native setup options — September 6, 2026

The user requested implementation of both setup options and explicitly deferred
testing. This change wires the development implementation. It does not promote any
hardware support cell or claim a successfully installed/booted system.

## Implemented

- **Desktop:** the two buttons open actual disk/requirements flows, with native
  target identity retained behind opaque UI choice IDs, progress, stage-dependent
  cancellation, native confirmation dialogs and completion/error receipts.
- **Helper:** one-shot same-executable elevation, private OS-authenticated IPC,
  helper-owned operation directories, compiled provider hashes, protected copies
  of runtime/source bytes, and offline verification with the pinned Omarchy key.
  The webview cannot provide an executable, shell command, image path or disk path.
- **USB:** pinned Etcher SDK 10.2.14 physical discovery and policy, fresh identity
  checks, source/system/internal-disk exclusion, native locking/unmount, bounded
  writes, flush, SDK verification, complete SHA-256 readback and explicit ejection
  status. A partial final physical sector is zero-padded and separately verified;
  the original ISO length/hash remain unchanged. Failed/aborted writes may leave
  the selected USB partly modified.
- **Windows no-USB:** official ISO -> isolated local installed-system construction
  -> protected manifest -> exact reviewed GPT partition plan -> ESP/Btrfs writes
  and complete readback -> permanent Omarchy UEFI entry. Existing GPT entries and
  the current default boot order are preserved. No ISO staging boot partition,
  automatic reboot, host OS replacement or filesystem shrinking is implemented.
- **Product first boot:** fresh per-build identities, portable initramfs, offline
  upstream hardware setup and upstream owner provisioning. No feasibility-harness
  account, password or SSH key is included in the product recipe.
- **Apple:** clean pinned upstream source, a private NDJSON Swift adapter around
  the original session/coordinator, Rust companion client and shared Tauri plan,
  progress and Recovery screens. Native owner credentials are kept out of webview
  state, argv and logs. Approval objects remain upstream native objects.

## Current constraints

Windows direct installation admits basic local GPT disks with 512-byte logical
sectors and existing contiguous space of 42,947,575,808 bytes, on a native x64 UEFI
host with Secure Boot disabled and no target BitLocker encryption. The recipe is
explicitly unencrypted. Construction requires the exact locally pinned Docker
runtime, 85 GiB of free working storage after staging the ISO, and 10 GiB of free
memory. Docker Desktop must already run its Linux engine. Its bind-mount access to
administrator-owned directories and engine control-channel trust still need
qualification; image-ID pinning is not attestation of the engine itself.

The Mac source pins upstream commit
`00daf3ebef9e4fbe89fb9b32bd184e65aa75d357` and the published `.7` engine. The sealed
published catalog admits only `apple,j314s` and expires on 2026-11-30. Compilation,
signed parent/companion/helper packaging, daemon installation and notarization
must happen on macOS. The upstream Team ID cannot be reused as this project's
signing identity. The bridge refuses incomplete or inconsistent packaging.
Recovery handoff is reported as such, not as an independently booted installation.
Linux and Intel Mac direct deployment are still outside this implementation.

Operation workspaces and journals are retained under an administrator-owned
`Omarchy-Setup-<UUID>` directory in Program Files on Windows (or `/var/tmp` for the
Unix USB helper). A failed operation is not automatically rolled back or retried.
The raw development executable and its provider resources are unsigned.

## Validation boundary

Only compilation, type checking, formatting and static source/ABI inspection are
authorized for this change. No new test suite, VM construction, elevation, disk
write, partition/firmware change or reboot was run. The Windows provider worker
did run a read-only unprivileged inventory; this host has only 1 MiB unallocated.
Previous feasibility work never completed an installed-system construction or
independent boot; that limitation remains.

Final integration checks:

- `pnpm desktop:check`: zero errors; four existing warnings in unmounted prototype
  components. No new flow warnings.
- `pnpm desktop:build:native`: frontend production build and Windows native linking
  succeeded. The final incremental build also passed with `--locked --features
  tauri/custom-protocol`, preserving the embedded frontend after the final edits.
- Native Rust formatting check passed. USB TypeScript compilation and the Windows
  provider's PowerShell/Python/bash syntax and C# compilation checks passed.
- Static `TOKEN_PRIVILEGES` layout inspection confirmed size 16 and field offsets
  0/4/12 after correcting C# packing. No firmware function was invoked.
- All 6,130 packaged provider files matched their lengths/SHA-256 values in the
  staging manifest. Manifest SHA-256:
  `5cc84f09818ac86ccd76dfa5aff7c8a90a5c8533954fddadd0fe871c2d1f0632`.
- Final development executable:
  `apps/desktop/src-tauri/target/debug/omarchy-setup-desktop.exe`.
  SHA-256: `6b74643007707eb07ba776f45b1fcb87a50543cb4703978e64eed50b8223fc29`.
  It was built, not launched for this change.

## Provenance

Root integration and bounded workers used `gpt-6-astra`. Workers: USB physical
provider/helper review, Windows direct provider/product builder, and native Apple
bridge/client. Provider-specific pins and evidence are kept in their directories.
The generated `apps/desktop/.native-providers/provider-lock.json` records the exact
host runtime files; Rust embeds it when compiling. Restage after provider edits.
