# USB inspection and existing ISO reuse — 2026-09-07

The desktop now defaults to `Downloads/Omarchy` and saves the official ISO
directly into the displayed destination. After release resolution and each folder
selection, a metadata-only existence check selects the Verify button. This is a
hint, never an authentication result. Existing destination files are authenticated
in place against official metadata and the embedded signing key. A wrong file is
reported without replacement. Reuse bypasses cache rehash and export hashing.

Signature verification and SHA-256 consume the same buffered sequential stream.
The reader checks exact length, supports cancellation, and reports byte progress.
Windows denies concurrent write/delete access for existing files. Active downloads
retain their owned write handle until promotion and require read/write sharing
during authentication; the normal session digest check and promotion remain.

USB Check disks uses a separately packaged read-only inspector with the shared
Node executable and a pinned drivelist/bindings/file-uri-to-path dependency tree.
It authenticates 51 files (83,604,865 bytes), then runs the same physical inventory
and exclusions as the writer. It accepts only list requests with a source path.
The redundant full SDK probe is removed from this read-only path. Privileged
operations still copy and authenticate the full provider bundle, reauthenticate
the ISO, and reidentify the target before writing.

## Validation

- Release client: 13 tests passed, one explicitly network-only smoke ignored.
  Single-pass tests cover valid signature/digest, incorrect digest despite a valid
  signature, changed bytes, length mismatch, and cancellation. Existing download,
  resume, corruption, signature and cache tests pass on Windows.
- Desktop native: 9 tests passed, including authenticated inspector dependency
  tampering and confirmation that full verification still detects writer tampering.
- Frontend: 14 tests passed, including Verify on file detection/folder changes,
  percentage progress, and no verified state before native completion.
- Svelte check: zero errors; four existing warnings in unrelated components.
- Media provider: 14 tests passed. Inspector rejects write/malformed requests;
  SDK file tests use temporary ordinary files, not physical media.

## Read-only timing

`scripts/profile-usb-inspection.cjs` hashes compiled manifest files and executes
the packaged provider's fixed protocol commands. Source:
`C:/Users/example/Downloads/Omarchy/omarchy-4.0.2.iso`.

Initial baseline: 3,348 files / 269,207,004 bytes; verification 55.289696 s,
SDK probe 3.8120825 s, list 2.8312055 s; total 61.932984 s, two drives.
New inspector: verification 0.1300772 s, list 3.558039 s; total 3.6881162 s,
two drives. These are sequential observations on this host; filesystem and
antivirus caches affect timings. No persistent trust cache is introduced.

A repeat of the old algorithm against the current staged bundle with cached
files measured 11.8422318 s: full verification 1.7724673 s (3,400 files), probe
6.1815987 s, and list 3.8881658 s. It also found two drives. Timings were collected
on a working development machine with build activity, not an isolated benchmark.

`cargo run --release -p omarchy-release-client --example official_release --
C:/Users/example/Downloads/Omarchy/omarchy-4.0.2.iso` resolved the official release
and authenticated the existing 6,227,752,960-byte ISO in 21.589 seconds, without
downloading an ISO body or exporting a copy. SHA-256:
`2ef8e624aa1bec7e277e28056b8535a6c9373ba48d7ede3f1a01cb6d2373cfb8`;
signer `40DFB630FF42BCFFB047046CF0134EE680CAC571`.

## Portable artifact

`artifacts/windows-portable/2af12708/Omarchy-Setup-0.1.0-x64-portable.exe`
is the optimized unsigned Windows x64 preview: 184,505,782 bytes, SHA-256
`757e35379c0e376d1d70d016f6140c8e7e944ca1d39e53a8a18ed859d6bb621f`.
The Windows GUI subsystem and embedded provider manifest were checked. The
staged SDK loaded its native modules and wrote/verified a 2,097,225-byte ordinary
test file. The single EXE extracted and authenticated its full payload in
25.9347118 seconds. No physical disk was written and no elevated operation was
run during this change's validation.

Normal startup passed: extraction indicator at 0.8622645 seconds, main window
at 12.4995566 seconds, normal window closure, launcher exit code 0, and complete
temporary payload cleanup. The artifact's `startup-verification.json` and
`portable-record.json` contain the recorded results.
