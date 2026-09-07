# Windows direct installation: completion pass

**Subsequent distribution correction:** Windows now defaults to one portable
self-extracting executable containing the app and provider files. The user
rejected both installation of the setup application and manual ZIP extraction.
The installed NSIS artifact below records
the earlier packaging pass; it is no longer the default distribution. See
[desktop packaging](../../apps/desktop/README.md) and the
[portable executable verification](windows-portable-2026-09-06.md).

Date: 2026-09-06. Model: `gpt-6-astra`. This pass resumes automated and
disposable-environment validation after the user's instruction to complete the
remaining Windows work. It does not qualify physical deployment or authorize
changes to this workstation's disks, BitLocker, firmware or startup tasks.

## Implemented

- **Upstream owner setup and reset:** the menu pre-hook accepts the exact shipped
  entry-free reset template. The post-hook tolerates that same template only
  during `limine-install`, which runs before kernel generation. Other callers
  and final verification still require a complete menu. Unknown or corrupt
  configurations remain errors. Upstream owner setup, rekeying and factory
  reset remain intact; no replacement onboarding was introduced.
- **BitLocker eligibility:** inspect each persistent protector's type and TPM
  PCR profile. The already-disabled-Secure-Boot path accepts the defined native
  UEFI profile `0,2,4,11`; custom, Secure Boot-bound and unknown profiles block
  deployment. Current `BootCurrent` must match the retained Windows entry.
  For TPM protection, parse the current TCG log and require a single PCR 4 EFI
  application route to `bootmgfw.efi` on the exact Windows ESP. Bind this evidence
  into the plan and recheck before firmware mutation. This is a conservative
  eligibility policy, not hardware qualification or a promise against recovery.
- **Secure Boot preparation:** a separate reviewed action suspends only Windows
  protection that was originally active, requests Windows' supported restart
  into firmware settings, and asks the user to disable Secure Boot while leaving
  TPM enabled. Protection restoration waits for the next Windows startup or a
  five-minute cancelled-restart deadline. The user returns to Windows and runs
  inspection again before installation. The application does not change the
  Secure Boot setting itself. Existing suspension is preserved.
- **Construction runtime:** release staging can include the pinned 119,989,248
  byte Docker archive. A separate action verifies and imports that archive into
  an already running Linux Docker engine. No image pull, host feature change or
  Docker installation is performed by this action. Docker Desktop installation,
  Linux-engine readiness and any Windows setup reboot remain prerequisites.
- **Cleanup and recovery:** remove only manifest-listed provider copies and
  known transient image/source files in the authenticated operation workspace.
  Failed construction requires positive confirmation that its Docker container
  stopped before those files can be removed. Preserve plans, receipts and
  recovery records; show their location and mutation status after failure.
  Cleanup is not secure erasure or partition rollback. Confirmed shrinking or
  deletion can persist after a failed installation.
- **Windows packaging:** a Tauri NSIS configuration bundles the application and
  provider resources. The packaging script supports a real signing certificate
  and timestamp service, verifies output signatures and records hashes. An
  explicit unsigned-preview mode supports local review. No public release or
  installer execution is part of this pass. The user has no signing certificate,
  so signing remains pending; no self-signed certificate was substituted.

## Evidence and limits

The five boot-menu tests use the actual ISO reset template and exercise the
owner-reset/rebuild sequence, retry, strict final verification and rejection of
unknown configurations. The pure Windows harness checks supported/unsupported
protector policy, recovery timing, and 541 synthetic measured-boot parser cases,
including malformed and truncated logs. It forbids host CIM calls and loads
only the recovery worker's pure decision function.

Windows PowerShell 5.1 parses the provider and packaging scripts and compiles
`NativeDisk.cs`, `NativeBootEvidence.cs` and `EncryptedImage.cs` without calling
their host APIs. Rust desktop tests pass, including bounded cleanup and existing
download behavior. Thirteen frontend tests pass, including saved-image gating
and the separate firmware acknowledgement/runtime import actions. Svelte
checking reports zero errors and four pre-existing
warnings. The packaged Docker archive was loaded successfully and Docker
reported the exact pinned image ID. Package output and its hash/signature status
are recorded in `artifacts/windows-package/package-record.json` when built.

The completed local debug NSIS preview is
`apps/desktop/src-tauri/target/debug/bundle/nsis/Omarchy Setup_0.1.0_x64-setup.exe`
(223,749,844 bytes), with SHA-256
`113023cc546c3e66621d7bc083aa7c2351c57218bbd5a0d0a487f8b4685f218a`.
Both outputs are unsigned. Extraction verified all 6,151 provider files against
their recorded lengths and hashes. The packaged application differs from the
loose build only in Tauri's documented three-byte `UNK` to `NSS` bundle marker;
every other byte matches. The installed Tauri source confirms this patching
contract. Details and the packaged application's own hash are retained in
`artifacts/windows-package/package-verification.json`. The installer was not run.

The full encrypted product construction needs **85 GiB free working storage
after source staging and 10 GiB currently free RAM**. At the resource check this
workstation had approximately 47 GiB free on C: and 8.7 GiB available RAM. The
large VM test remains pending additional resources; the production gates were
not reduced to force it through. No physical disk, TPM log, BitLocker lifecycle,
firmware preparation or independent installed-system boot was exercised here.

## Remaining qualification and release gates

1. On a sufficiently provisioned scratch machine, run the complete encrypted
   product builder from the verified ISO. Independently boot its result and
   check hardware finalization, capacity growth, owner setup/retry, temporary
   key removal, reboot and factory reset. Keep all VM disks file-backed.
2. On disposable Windows UEFI test machines, exercise the actual inspection,
   firmware transition, cancellation and watchdog restoration. Verify subsequent
   Windows boots through the menu with protection active, including TPM+PIN
   and recovery scenarios. Synthetic log parsing does not establish firmware
   behavior or resealing correctness.
3. Qualify free-space, NTFS shrink and explicitly confirmed deletion deployments,
   readback, interrupted writes, container/helper termination and retained
   recovery records. No current host mutation should be inferred from test
   authorization in this pass.
4. Sign and verify the application and installer with the release owner's
   certificate, then test installation/uninstallation on a disposable machine.
   Complete bundled-runtime licensing/source-availability review and branding
   authorization before publication. An unsigned local preview is not a signed
   or installation-qualified release.

References for the implemented Windows APIs:
[protector types](https://learn.microsoft.com/en-us/windows/win32/secprov/getkeyprotectortype-win32-encryptablevolume),
[PCR profiles](https://learn.microsoft.com/en-us/windows/win32/secprov/getkeyprotectorplatformvalidationprofile-win32-encryptablevolume),
[current TCG log](https://learn.microsoft.com/en-us/windows/win32/api/tbs/nf-tbs-tbsi_get_tcg_log_ex).
The earlier [source audit](deferred-setup-compatibility-2026-09-06.md) and
[boot research](bitlocker-direct-boot-research-2026-09-06.md) remain historical
evidence; their statements that these source changes were missing are superseded
by this report.
