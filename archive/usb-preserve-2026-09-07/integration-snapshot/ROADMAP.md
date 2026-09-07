# Omarchy Setup — Project Roadmap

Status: Windows direct completion pass implemented; automated checks passing; machine qualification pending

Last reviewed: 2026-09-06

The [Windows completion report](docs/evidence/windows-direct-readiness-2026-09-06.md)
supersedes earlier deferred-test and missing-implementation statements below.
The upstream reset-template integration is corrected and pure regression tests
pass. Protector/PCR and current-boot inspection, Secure Boot preparation,
packaged runtime import, bounded cleanup and Windows installer packaging are
implemented. Full encrypted VM construction needs additional scratch resources;
physical deployment, return-boot recovery and signed release remain unqualified.

**Next Windows no-USB milestone:** establish local system construction and native
deployment, preserving upstream first-boot owner setup. Read the
[latest priority decision](docs/direct-install-priority-2026-09-06.md) for staging
findings, shared BitLocker boot risks and the retained prepared-image option.

Windows direct startup now uses a configurable Omarchy / Windows menu, initially
Omarchy after five seconds. The menu is first in firmware order; existing entries
are retained. Mac keeps Apple's startup picker. The maintained
[platform behavior guide](docs/platform-behavior.md) records these differences for
users and the wider team. This startup behavior remains unqualified on hardware.

Current implementation results and remaining gates are in the
[September 6 integration report](docs/evidence/native-options-2026-09-06.md).
The [encryption and allocation update](docs/evidence/encryption-allocation-2026-09-06.md)
adds upstream LUKS2, selectable Windows allocation, native NTFS shrinking and
conditional BitLocker suspension/restoration. All installation execution and
recovery qualification remain pending; automated tests resumed in the completion pass.
The [partition replacement update](docs/evidence/deletion-confirmation-2026-09-06.md)
adds an explicitly selected Windows deletion path with typed target confirmation.
The [shared storage update](docs/evidence/shared-storage-2026-09-06.md) reuses these
frontend controls on macOS and connects native detected-install replacement and
alongside selection. Mac build/signing and execution qualification remain pending.

The latest September 6 discussion returns local construction/native deployment
to first priority after investigating staging. It remains implemented but
unqualified. Staging is retained as a secondary option; published prepared-system
images remain a future research option. See
[the implementation strategy](docs/installation-plan.md) and
[preserved staging findings](docs/staged-iso-first-plan.md).

## 1. Objective

Build a community-incubated, cross-platform Omarchy onboarding application with
one coherent product experience and replaceable platform backends.

The application should let a user:

1. Download and verify an official ISO or platform image without writing it.
2. Download, verify, write, verify, and eject an Omarchy installation USB.
3. Install Omarchy directly when a qualified platform backend exists.
4. Resume, recover, or export diagnostics for operations it owns.

Try Omarchy is outside v1 and may return as an optional later integration.
Removal and repair are offered only where a backend implements and qualifies
them; a generic Manage screen is not evidence that those operations work.

The working name is **Omarchy Setup**. Until OmaCom grants branding permission,
all public builds must say **community preview — not an official Omarchy
release** and must not imply official endorsement.

## 2. Product boundary

This project is an integration and safety layer, not a new universal disk
installer and not a reimplementation race against announced Omarchy work.

- Use the unchanged official Omarchy ISO for x86 USB media and as the source
  for local x86 system-image construction. Do not publish a derivative ISO or
  maintain a separately hosted Omarchy distribution image in the planned path.
- For the first Windows no-USB milestone, construct from the verified official
  ISO on a file-backed virtual disk, then deploy through the native helper.
  Preserve upstream deferred owner setup and its required state. Resolve
  construction resources, portability, BitLocker restoration across boot changes
  and cleanup before qualification. Staging remains secondary; machine tests remain pending.
- Keep one shared Tauri GUI and reuse the Apple-Silicon installer's native
  coordinator, privileged helper, and Asahi engine through an adapter.
- Use one maintained USB-writing dependency behind a narrow adapter. Etcher SDK
  is planned for all three hosts, subject to integration qualification; avoid
  a full flasher-application fork or two production engines by default.
- Include Windows, macOS, and Linux in the product architecture and download/USB
  scope. Enable direct installation per qualified hardware and storage layout.
- Do not make further implementation depend on an announced unpublished engine
  or an upstream reply. Preserve attribution, licenses, and unofficial status.

One product does not require one executable or one privileged implementation.
The user experience can be shared while disk and boot operations remain native
to each host platform.

## 3. Capability matrix

| Capability | Windows x64 | Intel Mac | Apple Silicon | Linux host |
| --- | --- | --- | --- | --- |
| Download and verify | v1 target | v1 target | v1 target | v1 target |
| Create x86 USB | v1 target | v1 target | v1 target, for another PC/Intel Mac | v1 target |
| Boot that x86 USB on the host | Yes | Model-dependent | No | Yes on supported x86 hardware |
| Try in VM | Deferred beyond v1 | Deferred beyond v1 | Deferred beyond v1 | Deferred beyond v1 |
| Direct install alongside | Local-image feasibility first | Same x86 path, model qualification | Native Asahi backend integration | Same image strategy, layout qualification |
| Replace current OS | Offline, last | Hardware-dependent, last | Do not promise | Offline, last |
| Resume/repair/remove | Per provider | Per provider | Upstream-defined | Per future provider |

Apple Silicon requires an Asahi-aware installation path. An ordinary x86 ISO
written to USB does not install Omarchy on a pristine Apple-Silicon Mac.

## 4. Safety and release principles

These are release-blocking invariants:

1. The main GUI always runs as the ordinary user.
2. Read-only inspection and protected local construction can request elevation.
   The exact destructive plan must be displayed and confirmed before disk mutation.
3. A small platform helper performs only typed, allowlisted operations. It has
   no shell, arbitrary command, generic file-write, network, or update API.
4. Nothing is written, mounted, or executed before cryptographic verification.
5. The privileged helper distrusts the GUI, re-verifies the artifact, and
   re-identifies the target immediately before mutation.
6. USB mode refuses the running system disk, internal disks, ambiguous devices,
   and devices whose identity changes after selection.
7. All writes are bounded by an immutable, hashed operation plan.
8. A write is successful only after durable flush and read-back verification.
9. Crashes, cancellation, and reboots leave a crash-safe operation journal and
   never silently report success.
10. Unknown hardware, layouts, identities, and states fail closed.
11. Diagnostics are locally generated, redacted, previewable, and manually
    uploaded. There is no default telemetry.
12. Simulation can produce a developer preview, but never by itself qualifies
    a destructive feature as stable.

## 5. Recommended architecture

### 5.1 Technology

- Shared core: Rust workspace with a pinned stable toolchain.
- Desktop UI: Tauri 2 with Svelte and TypeScript. Safety policy remains in Rust.
- Windows helper: one-shot signed native launcher/adapter launched with UAC;
  the selected engine may run as a separately packaged process.
- macOS helper: small Swift `SMAppService`/XPC component using native code
  identity and Disk Arbitration APIs.
- Linux helper: root-owned packaged service authorized through Polkit; use
  UDisks2 where suitable.
- USB engine: Etcher SDK is the shared backend to qualify, with a bundled Node
  runtime/native modules behind the privileged adapter. Do not assume a Rust
  rewrite or an SDK default provides our full target/verification policy.
- Local image builder: unprivileged orchestration of a disposable Linux runtime
  that sees only file-backed virtual disks; physical target access stays in the
  native deployment helper. See section 5.4 and the linked strategy.
- Operation state: append-only, crash-safe journal with an immutable plan hash.
- Provider protocol: versioned typed messages with strict schemas, bounded
  inputs, per-operation nonces, and no unknown-field privilege expansion.
- Artifact trust: embedded offline root plus versioned signed catalog with
  expiry, rollback protection, key rotation, exact lengths, hashes, supported
  hosts, and provider protocol constraints. Preserve and verify upstream
  Omarchy signatures as a separate trust layer.

### 5.2 Logical structure

```text
Desktop UI
    |
    +-- shared domain, policy, catalog, download, journal, diagnostics
    |
    +-- Media provider ------- adapter + maintained writing engine
    +-- Direct providers ----- local x86 image builder + native deployment
    |                         retained Apple-Silicon native backend
    +-- Try providers -------- optional, beyond v1
```

Proposed repository:

```text
omarchy-setup/
  apps/
    desktop/
    helper-windows/
    helper-macos/
    helper-linux/
  crates/
    domain/
    catalog/
    downloader/
    device-policy/
    media-writer/
    provider-contract/
    journal/
    diagnostics/
    ipc/
  providers/
    media/
    image-builder-x86/
    direct-apple-silicon/
    direct-windows/
    direct-intel-mac/
    direct-linux/
  catalog/
    schemas/
    trusted-root/
    test-vectors/
  testkit/
    fake-block-device/
    synthetic-disks/
    malformed-catalogs/
    fault-injection/
    qemu/
  docs/
    adr/
    threat-model.md
    support-matrix.md
    tester-playbooks/
    releasing.md
  tools/
    catalog-publisher/
    support-bundle-sanitizer/
```

### 5.3 Provider contract

Every Direct provider, and any future Try provider, should expose the same lifecycle:

```text
probe_host() -> HostFacts
capabilities(HostFacts, SignedPolicy) -> Capability[]
preflight(Request) -> Finding[]
plan(Request) -> ImmutablePlan + Digest
authorize(Plan) -> AuthorizationResult
execute(Plan) -> ProgressEvents
resume(OperationId)
recover(OperationId)
cancel(OperationId)
diagnostics(OperationId) -> SanitizedBundle
```

USB creation now has a requested second mode: **keep files and add installer**
when the filesystem, layout and boot method are compatible. The Windows preview
now implements this for an existing GPT USB with NTFS data and FAT32 EFI, with
verified bootloader backup/restoration. The raw writer remains the separate
**erase USB and create installer** path. See the
[implementation and virtual qualification](docs/usb-preserve-implementation-2026-09-07.md) and
[the preservation requirement and source findings](docs/usb-preserve-files-2026-09-06.md).
The [product direction](docs/usb-product-direction-2026-09-07.md) defines checking
the selected drive, two add-alongside/erase choices with separate confirmations,
and staged compatibility qualification. Backup-and-recreate remains a separate
possible migration workflow, never an implicit fallback from adding alongside.

For the existing erase mode, the pipeline is:

```text
resolve signed image
-> download/resume
-> verify signature, length, and hash
-> enumerate eligible whole removable disks
-> show exact target and destructive effect
-> request elevation
-> re-identify and lock target
-> unmount/dismount
-> stream write
-> flush durable caches
-> hash-read the written image length
-> eject
-> issue an operation receipt
```

### 5.4 Local construction and deployment for x86 direct installation

```text
download and verify official ISO
-> construct installed system using upstream packages/scripts in local Linux
-> produce operation-specific partition images and a provenance receipt
-> inspect/verify images and review the physical deployment plan
-> native host resize/allocation where qualified
-> write only the planned destination extents and configure boot
-> boot on the destination and finish hardware/user setup
-> confirm independent boot and clean owned temporary files
```

The ISO contains a live installer and an offline package repository, not a
finished desktop filesystem to extract. Local construction is real installation
work and needs a maintained recipe. It can avoid a dedicated ISO boot partition
without avoiding temporary ISO, virtual-disk, and image files in the host cache.
Unique encryption, hardware finalization outside the build VM, filesystem growth,
and boot configuration are feasibility gates, not details assumed to work.

Booting a staged official ISO remains an alternative if evidence invalidates the
preferred path. It is not the default direct-install mechanism in this revision.

## 6. Implementation phases

### Phase 0 — Charter, upstream alignment, and governance

Deliverables:

- Product brief, capability matrix, non-goals, and terminology.
- MIT license proposal and `THIRD_PARTY_NOTICES.md` policy.
- Omarchy branding/trademark question and unofficial-build language.
- Upstream GitHub Discussion proposing a shared setup/USB foundation.
- Coordination requests for Apple Silicon, Windows, Intel Mac, and VM projects.
- Threat model and architectural decision records.
- Governance: code owners, two-review rule for safety-critical code, private
  security contact, protected release environment, and release authority.

Exit gate:

- The standalone work can proceed even without an upstream reply, but it may
  implement only non-overlapping foundation work and may not claim official
  status.

### Phase 1 — Non-destructive foundation

Deliverables:

- Rust workspace, Tauri shell, and cross-platform CI.
- Connect the existing UI to real Download, Create USB, and qualified Install
  operations; remove Try from v1 navigation when implementation resumes.
- Label remaining design previews honestly; a completion Boolean is not a
  provider simulation or a completed operation.
- Host capability model and provider simulator.
- Signed-catalog schema with valid, corrupt, expired, rollback, and key-rotation
  test vectors.
- Resumable verified downloader and cache.
- ISO/image download-only journey.
- Operation state machine, journal, progress events, and sanitized diagnostics.
- Unit, property, model-based, mutation, and fuzz-test foundations.

Exit gate:

- Every user journey works in dry-run mode.
- No path can reach a real disk.
- Invalid or stale metadata always fails closed.

### Phase 2 — Reused media engine and privilege boundary

Deliverables:

- File-backed fake block devices and synthetic GPT/MBR layouts.
- Target-classification and stable-identity policy.
- Qualify a pinned Etcher SDK adapter for streaming, cancellation, durable
  flushing, and full read-back verification. Inspect API behavior and packaged
  native dependencies on each host before choosing the production engine.
- Retain the current Rust media-writer/fake-device code as test infrastructure
  while comparing engines; do not maintain duplicate production writers by default.
- Windows elevated-helper and macOS XPC-helper protocols.
- Authenticated IPC, target re-enumeration, replay protection, and helper
  compatibility checks.
- Fault injection for short writes, I/O errors, disconnect/reconnect, stale
  identity, helper/UI crashes, and power-loss checkpoints.
- Dry-run plan export and before/after disk-image comparison.

Exit gate:

- All safety invariants are represented by tests.
- Two sentinel disks accompany every destructive simulation; any changed byte
  outside the selected target blocks the build.
- Release packaging proves the fake backend cannot be selected accidentally.

### Phase 3 — External-media developer preview

Deliverables:

- Unsigned or developer-signed Windows and macOS packages.
- Exact tester playbooks for disposable USB drives.
- Locally previewable support bundle and structured GitHub issue form.
- Public hardware evidence matrix.
- Emergency signed policy capable of disabling a broken artifact/provider
  combination without downloading new executable behavior.

Promotion to experimental USB support requires, at minimum:

- Independent Windows and macOS hosts.
- Multiple physical USB controller/device models and capacities.
- Successful write, full verification, eject, and boot.
- Cancellation and unplug/replug exercises.
- Zero wrong-target or non-target mutations.
- Every failure classified, reproduced in simulation where possible, and fixed
  or explicitly excluded.

### Phase 4 — Production-grade USB release

Deliverables:

- Windows Authenticode signing and timestamping.
- macOS Developer ID signing, hardened runtime, notarization, and stapling.
- Production catalog-key ceremony and rotation/revocation runbook.
- Pinned CI actions, least-privilege jobs, SBOM, license report, provenance, and
  independent public-asset verification.
- Signed update and repair path; no update during an active operation.
- Independent security review of helper, IPC, update, and disk-target logic.

Exit gate:

- Required physical matrix is green.
- No unresolved data-loss or high-severity security issue.
- Recovery, support, and signing ownership are documented.
- A human approves promotion through the protected production environment.

### Phase 5 — Try Omarchy integration

Deferred beyond v1; this phase does not block any direct-install phase. A Linux
runtime used internally to construct an image is not a consumer Try feature.

Start with signed handoff providers rather than merging the VM codebases:

- Discover an installed provider or download its verified release.
- Launch it and report provider version/health.
- Offer its supported cleanup/delete journey.
- Keep VM runtime and guest-image updates independently disableable.
- Add QEMU/KVM as the Linux provider.

Deep embedding requires maintainer agreement, compatible licensing, and a stable
provider contract.

### Phase 6 — Guided installation preparation

Before owning direct disk mutation, provide read-only or reversible assistance:

- Windows UEFI/GPT, BitLocker, Fast Startup, free-space, storage-controller,
  power, and firmware checks.
- Intel-Mac model/T1/T2, external-boot, Secure Boot, disk-space, and recovery
  guidance.
- Apple-Silicon model/Asahi support, APFS space, macOS version, power, and
  Recovery-flow checks.
- Linux storage/encryption/layout classification.
- Exact explanation of alongside, separate-disk, and replace modes.

Unsupported states should route the user to verified USB creation or download,
not offer a force switch.

### Phase 7 — Apple-Silicon direct-install provider

- Replace the native screens with the shared Tauri flow while retaining the
  native coordinator, inspection, state machine, authenticated helper, and engine.
- Build a narrow bridge or signed companion; the current Swift protocol is not
  an already available external API. Signed handoff is a fallback integration.
- Preserve its reciprocal application/helper identity checks.
- Use its signed catalog and exact model allowlist.
- Present Recovery boot-policy completion as a required installation stage.
- Add common journaling, progress, diagnostics, and ownership records only
  through an agreed interface.
- Never market one tested M1 Pro configuration as M1–M5 support.

Exit gate per Mac model:

- Multiple independent successful installations.
- Deliberate interruption/recovery exercises on sacrificial systems.
- macOS and Recovery remain intact.
- Removal/reclamation follows an upstream-reviewed process.

### Phase 8 — Windows and Intel-Mac direct providers

First establish [local construction and native deployment](docs/direct-install-priority-2026-09-06.md)
for Windows x64 UEFI/GPT. Preserve upstream deferred owner configuration,
provisioning state and factory-reset behavior. Intel Mac qualification remains
separate work; staged installation is a secondary option with unresolved
source-disk and BitLocker integration.

Prove local construction from a pinned official ISO into a file-backed
virtual disk, export usable partition images, then boot the result as an
independent installed system. Record the ISO digest, builder/tool versions,
configuration, and output hashes. Do not expose the physical disk to the build VM.

Begin deployment qualification with Windows x64 UEFI/GPT alongside installation
into unallocated space. Add native NTFS shrinking as a separately tested part
of the planned workflow; it is not forbidden merely because Windows is running.
Write the prepared system into the target space, configure permanent boot, and
finish machine-specific setup after reboot. The target must boot without the
source ISO, build VM, or host cache.

Per-operation encryption keys, generic boot support, real hardware detection,
target identifiers, filesystem growth, credential handling, and crash recovery
must be resolved before direct installation is enabled. Building in a VM must
not silently select a VM-only driver/boot configuration for physical hardware.

Do not publish a derived ISO or reusable encrypted master image. The preferred
path avoids a temporary physical installer partition at the cost of host
runtime/resources. Full replacement, Storage Spaces, dynamic disks, ambiguous
RST/VMD, legacy BIOS, Windows ARM, and unqualified encryption layouts remain out
of the first support cell. Intel Macs reuse the x86 strategy only after their
model-specific boot/hardware path is qualified.

Exit gate per support cell:

- Exact host, firmware, disk-controller, encryption, and installation mode is
  recorded.
- Multiple independent bare-metal successes and recovery drills.
- Host OS boot preservation verified for recoverable failures.
- Independent review of the disk and boot plan.
- No out-of-plan mutation or unexplained data-loss report.

### Phase 9 — Linux host

The shared core should make these early Linux features comparatively small:

- Download and verification.
- USB discovery, writing, verification, and ejection through UDisks2/udev and a
  Polkit-authorized packaged helper.
- Local Linux image construction using the same upstream recipe, followed by
  a native deployment adapter for supported layouts.

An AppImage may carry the unprivileged frontend, but it must not install or
contain a user-writable privileged helper. Ship the helper through authenticated
`.deb`/`.rpm` packages.

Direct installation belongs to the shared strategy, with separate qualification
for LUKS, LVM, Btrfs, ZFS, RAID, and bootloader combinations. Start with a simple
destination layout; neither dual boot nor native deployment is categorically
unavailable on Linux. Try/QEMU/KVM product integration stays beyond v1.

### Phase 10 — Manage, repair, and official-adoption path

- Resume and explain incomplete operations.
- Re-download/repair owned assets and boot entries.
- Export redacted support evidence.
- Remove only resources proven to belong to Omarchy Setup.
- Treat removal and host-filesystem expansion as separate operations.
- Prepare repository transfer or provider-by-provider upstream adoption once
  OmaCom accepts the security, support, and maintenance model.

## 7. Validation without local hardware

The initial absence of hardware is a release constraint, not an implementation
blocker.

### Automated layers

1. Unit tests for catalogs, signatures, plans, device policy, arithmetic,
   progress, cancellation, journal recovery, and diagnostics redaction.
2. Property tests generating thousands of arbitrary disk topologies, including
   512-byte, 512e, and 4Kn sectors, duplicate IDs, missing serials, malformed
   GPTs, multiple ESPs, undersized media, and sparse multi-terabyte disks.
3. Model-based testing of every legal and illegal state transition.
4. Fuzzing of catalogs, GPT/MBR parsers, IPC, resume metadata, and device
   inventory. Run smoke fuzzing per change and longer nightly campaigns.
5. Virtual devices: Linux loop/NBD, Windows VHDX, macOS disk images, and a fully
   simulated backend with deterministic failure injection.
6. QEMU/OVMF end-to-end tests: write the ISO, boot it, install to a new virtual
   disk, reboot, and verify the resulting Omarchy version and layout.
7. CI builds and package inspection on Windows, macOS, and Linux across available
   x64/ARM runners.

Virtualization cannot faithfully prove Apple Recovery/Asahi behavior, OEM UEFI,
T2/VMD/RST, physical USB controller behavior, BitLocker/APFS failure recovery,
or real power-loss safety. Those remain explicit external gates.

### Community test rings

| Ring | Allowed activity |
| --- | --- |
| 0 | UI, simulator, catalog, and dry-run only |
| 1 | Download and cryptographic verification |
| 2 | Write a disposable USB with no valuable data |
| 3 | Boot the USB on a spare computer |
| 4 | Install to a spare disk or dedicated test machine |
| 5 | Direct/dual-boot install after platform-specific qualification |

Tester rules:

- Require backup and recovery preparation.
- Never recruit an employer-managed or only computer for destructive trials.
- Never collect passwords, recovery keys, disk contents, Wi-Fi details, full
  serial numbers, or persistent hardware identifiers.
- Give each candidate an immutable build ID and structured checklist.
- Convert every useful failure into a synthetic fixture and regression test.
- Freeze the affected feature immediately after any credible wrong-disk,
  out-of-plan, lost-data, or unrecoverable-boot report.

Support is assigned per matrix cell—not broadly to an operating system. A cell
includes host version/architecture, mode, firmware/storage/encryption class and,
for Macs, exact model identifier.

Release channels:

```text
development -> canary -> hardware-qualified beta -> stable
```

Stable requires repeatable physical testing, no unresolved data-loss/security
blocker, documented recovery, signed packages/catalog, and human release
approval. Passing CI alone is never sufficient.

## 8. Upstream and community workflow

1. Open an Omarchy Suggestions Discussion with the product boundary and provider
   architecture before a public announcement.
2. Ask whether Windows/Intel work already has a public-home plan, which provider
   boundary maintainers prefer, and whether the name/artwork may be used.
3. Open an Apple project design discussion about a stable handoff/provider
   interface.
4. Publish the neutral repository and non-overlapping foundation even if the
   answer is delayed.
5. Keep architecture decisions and evidence in GitHub; use Discord for tester
   recruitment and support, not as the system of record.
6. Submit small, independently useful upstream changes only after a proven
   vertical slice. Avoid a giant speculative installer pull request.
7. Integrate newly public upstream engines behind providers instead of replacing
   or racing them.

Recommended governance:

- Two independent approvals for device policy, raw writes, privileged helpers,
  catalog/update trust, signing, or direct-install changes.
- No self-merge for safety-critical code.
- Required CI, signed tags, protected release environments, and least-privilege
  CI permissions.
- Developer Certificate of Origin rather than a new CLA unless OmaCom requests
  otherwise.
- Private security-reporting route and published response process.

## 9. Autonomous execution contract

The following describes delegation after implementation is explicitly resumed.
The subsequent September 5 "let's work on that" request resumes implementation
with at most four active workers, including the coordinator. Current work covers
downloads and virtual/file-backed feasibility. It does not authorize publication,
upstream messages, or physical disk/firmware operations.

Codex can autonomously perform:

- repository scaffolding and architecture decisions within this roadmap;
- product UI and accessibility implementation;
- shared Rust core and provider contracts;
- platform helpers and packaging scripts;
- simulated and virtual-device tests, fuzzing, fault injection, and CI;
- unsigned/self-signed development builds;
- threat model, SBOM, notices, documentation, tester playbooks, issue forms, and
  support tooling;
- report triage, regression reproduction, fixes, and release-candidate assembly;
- clean commits, review preparation, upstream RFC/PR drafts, and responses once
  access is authorized.

External gates that cannot be substituted autonomously:

1. Repository/account ownership, MFA, provider terms, and public visibility.
2. Omarchy name/logo permission and the decision to claim official status.
3. Apple Developer identity, Developer ID certificates, and notarization
   credentials.
4. Windows legal publisher validation and production code-signing credentials.
5. Production catalog root-key custody and human key ceremony.
6. Physical USB, firmware, Recovery, encryption, boot, and destructive-install
   testing.
7. Upstream acceptance, independent security review, and stable promotion.

The default low-interruption workflow is:

1. Record one kickoff decision packet.
2. Work autonomously through the local dry-run and simulated foundation.
3. Ask for repository/publication access only when a public repo is useful.
4. Batch production-signing setup into one credential-provisioning session;
   secrets are entered directly into protected storage, never chat or source.
5. Publish tester candidates and autonomously process their evidence.
6. Stop only when an irreversible external choice or release gate is reached.
7. Require a human approval for beta expansion and stable publication.

## 10. Kickoff defaults

Unless changed before implementation begins:

- Standalone local repository with a neutral history, not a fork.
- Provisional name: `omarchy-setup`.
- Proposed license: MIT with preserved third-party notices.
- Status: unofficial community project.
- No telemetry; manual, previewable diagnostics only.
- v1 scope: download-only, automatic download plus USB creation, and direct
  installation where each backend is qualified; Try is optional beyond v1.
- Windows, macOS, and Linux are target hosts; image architecture describes the
  destination computer and may differ from the machine creating USB media.
- Shared Tauri frontend with retained native Apple backend and locally
  constructed x86 images from the official ISO; no custom ISO publication.
- USB reuse direction: qualify a pinned Etcher SDK on all hosts behind an adapter;
  use a narrow maintained patch only if an integration gap requires one.
- All real-device and direct-install functionality is feature-gated and fails
  closed until its hardware evidence exists.

## 11. Definition of project completion

The whole project is complete only when:

- Windows, macOS, and Linux provide signed, maintained download/verify/USB
  journeys.
- Try is not a v1 completion requirement. Any later advertised Try provider
  must have its own complete supported lifecycle.
- Each advertised direct-install support cell has a qualified provider,
  recovery path, hardware evidence, and maintainer.
- Updates, catalog rotation/revocation, repair, resume, uninstall, diagnostics,
  and security response are operational.
- The public support matrix states exactly what is stable, experimental, or
  unavailable.
- Ownership, signing, tester infrastructure, and release governance can continue
  without depending on one person or one agent session.

## 12. Next work package

After explicit authorization to start implementation:

1. Audit and reuse the existing Rust/Tauri foundation; do not restart scaffolding
   or count the mock UI as functioning installation support.
2. Connect one verified download path to download-only and USB preparation.
3. Prove Etcher SDK packaging and a typed adapter on Windows, macOS, and Linux
   using test images/file-backed devices before physical USB qualification.
4. Prove official-ISO-to-installed-image construction entirely on virtual disks,
   including an independent boot without the ISO attached.
5. Resolve encryption, real-hardware finalization, source-cache requirements,
   image provenance, and cleanup from that evidence.
6. Prototype the native Apple backend bridge while preserving its identity and
   Recovery rules.
7. Implement native x86 deployment only after the image-construction gate passes,
   then qualify resize, boot preservation, and recovery on the exact support cell.

These are bounded feasibility and integration milestones, not a promise that
local image construction or USB engine packaging is already straightforward.

## 13. Relevant upstream projects

- Omarchy: <https://github.com/omacom/omarchy>
- Omarchy ISO: <https://github.com/omacom/omarchy-iso>
- Apple-Silicon work: <https://github.com/maralcbr/omarchy-mx-mac>
- Etcher SDK: <https://github.com/balena-io-modules/etcher-sdk>
- Etcher helper packaging reference: <https://github.com/balena-io/etcher>
- Fedora Media Writer alternative: <https://github.com/FedoraQt/MediaWriter>
- Try Omarchy for macOS (deferred): <https://github.com/omacom/try-omarchy>
- Try Omarchy for Windows (deferred): <https://github.com/omacom/try-omarchy-windows>
