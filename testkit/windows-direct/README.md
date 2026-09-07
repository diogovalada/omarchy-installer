# Windows direct pure regression tests

These tests do not access disks, firmware, TPM, BitLocker or scheduled tasks.

From the repository root:

```powershell
powershell.exe -NoProfile -File testkit/windows-direct/Test-Preflight.ps1
```

This runs protector-policy and recovery-timing checks and compiles/runs the
synthetic measured-boot parser cases. Host CIM entry points deliberately throw.
The recovery worker body is never executed.

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
