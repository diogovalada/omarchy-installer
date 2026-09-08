# Windows direct pure regression tests

These tests do not access disks, firmware, TPM, BitLocker or scheduled tasks.

From the repository root:

```powershell
powershell.exe -NoProfile -File testkit/windows-direct/Test-Preflight.ps1
powershell.exe -NoProfile -File testkit/windows-direct/Test-StoragePlan.ps1
powershell.exe -NoProfile -File testkit/windows-direct/Test-RuntimeDistribution.ps1
powershell.exe -NoProfile -File testkit/windows-direct/Test-PartitionTransfer.ps1
```

This runs protector-policy and recovery-timing checks and compiles/runs the
synthetic measured-boot parser cases. Host CIM entry points deliberately throw.
The recovery worker body is never executed.

`Test-StoragePlan.ps1` loads the production planning functions with synthetic
host boundaries. It covers blank GPT disks, failed or incomplete inventories,
physical-sector limits, protected-path discovery with `ProgramData` absent,
and allocation identity, size and alignment checks.

`Test-RuntimeDistribution.ps1` checks probe-only archive metadata and confirms
runtime import still requires the actual archive. It does not invoke Docker.

After `pnpm providers:stage`, measure disk-check tool preparation with:

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib --locked profile_packaged_disk_check_staging -- --ignored --nocapture
```

This copies and verifies the selected package files into a temporary directory;
it does not run the disk probe or require administrator access.

`Test-PartitionTransfer.ps1` compiles the production C# transfer loop and runs
seven cases using real Windows unbuffered I/O into newly created temporary files.
It checks fragmented input, short/long sources, wrong source hashes, corrupted
readback, flush failures and read failures. A sentinel after the approved image
must remain unchanged. It never opens a disk, partition or firmware handle.

For repeatable UI review, run `pnpm --dir apps/desktop dev --host 127.0.0.1` and
open `/test-fixtures/windows-direct.html?state=ready`. The available scenarios
are `ready`, `preparation`, `running`, `complete` and `failed`. Select **Install
without USB** where necessary. This development-only fixture renders the real
components with synthetic state and an in-memory command bridge. It cannot
install an OS and is not an entry point in the production Vite build.

Run `test_boot_menu.py` with Python 3 on Linux. The test imports the repository's
menu module and calls pure transformation functions; it does not install hooks
or edit a live ESP. The existing pinned Linux construction container can run
it with a read-only repository mount, no networking and no extra capabilities.

`fixtures/limine-reset.conf` is the exact template extracted from the verified
Omarchy 4.0.2 ISO at `usr/share/omarchy/default/limine/limine.conf`, shipped by
`omarchy-settings-4.0.2-1-any.pkg.tar.zst`. It is an Omarchy upstream configuration
fixture, retained under upstream's MIT license; see the full upstream notice in
`apps/desktop/src/assets/omarchy/LICENSE-Omarchy.txt`. Its purpose is to test compatibility with
the actual released reset input rather than an invented empty menu.

These tests do not qualify an encrypted build, owner setup, factory reset,
hardware deployment, Windows return boot or real protection restoration. See
[the completion report](../../docs/evidence/windows-direct-readiness-2026-09-06.md).
