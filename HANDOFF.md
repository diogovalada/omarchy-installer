# Omarchy Installer — Project and Research Handoff

Status: Windows completion pass implemented; automated validation resumed; machine qualification pending

Updated: 2026-09-06 (Europe/Lisbon)

> **September 12 direct-install brainstorming:** read the
> [consolidated design record](docs/direct-install-brainstorm-2026-09-12.md)
> before revisiting the architecture. It separates Windows deployment from
> staged installation, local construction from published images, payload access
> from BitLocker boot trust, and installer policy from UEFI/Secure Boot. It also
> preserves the Libertix findings, upstream image proposal, corrections and
> qualification gaps. This discussion does not select a replacement architecture.

> **Latest USB requirement:** offer erasing and preserving existing USB contents
> as separate modes, with preservation available only on compatible layouts.
> Current USB code is erase/raw-write only. The
> [preservation investigation](docs/usb-preserve-files-2026-09-06.md) records
> UEFI/BIOS conflicts, actual ISO file-size/boot-identity constraints and existing
> Ventoy as a candidate first integration. Ordinary data USB conversion remains
> in scope but needs its own backend and qualification. No new preserving action
> or physical USB changes were made by this research/design update.

> **Latest distribution correction:** the user rejected installing the installer
> application itself, then explicitly requested bundling everything into one
> executable. Windows packaging now defaults to one portable self-extracting
> `.exe`; it temporarily unpacks the app and providers and opens the app.
> No manual ZIP extraction, application installation wizard or Start menu
> registration is required. The older installed NSIS
> command is explicitly optional. See [desktop packaging](apps/desktop/README.md).
> This changes distribution only; signing, construction resources and actual
> installation/boot qualification remain pending.

> **Read first — latest completion pass:**
> [Windows readiness report](docs/evidence/windows-direct-readiness-2026-09-06.md)
> records the reset-template fix, actual protector/PCR and measured-route
> inspection, separate Secure Boot preparation, packaged runtime import,
> bounded cleanup and NSIS packaging. Pure tests and compilation pass. The full
> encrypted VM build needs more scratch space and available RAM; physical
> execution and signed-release qualification remain pending. Earlier statements
> below about missing code and deferred automated tests are historical. No host
> disk, BitLocker, TPM, firmware or scheduled-task action was executed.

> **BitLocker research conclusion:** our restart-to-original-Windows-entry menu
> permits same-session suspension/deployment/restoration in the defined
> already-compatible UEFI case; an extra Windows boot is not universally
> required. Actual PCR-profile/current-boot-route inspection is now implemented, and
> Secure Boot enabled-to-disabled remains a separate transition. See
> [the specification research](docs/evidence/bitlocker-direct-boot-research-2026-09-06.md).

> **Known source-audit blocker:** our menu pre-hook requires an existing advanced
> group, but upstream owner password setup and factory reset replace the menu
> with an entry-free template before rebuilding. The hook aborts that rebuild.
> Read [the compatibility audit](docs/evidence/deferred-setup-compatibility-2026-09-06.md).
> The correction belongs in our hook; preserve upstream setup and reset. This
> was a read-only source audit. The completion pass now fixes both hook phases
> and adds pure regression coverage; the actual owner/reset boot remains pending.

> **Latest priority decision:** the user now favors local construction followed
> by native Windows deployment, with personal configuration deferred to first
> boot. Read [the current decision](docs/direct-install-priority-2026-09-06.md).
> Staging is a secondary option: retain its source-disk and BitLocker findings.
> Direct deployment offers better control of suspension/restoration, but does
> not eliminate boot-related recovery risk. Its restoration timing and later
> Windows boots still need qualification. Prepared-image publication remains
> a research option. The later completion pass implements the remaining source
> work and resumes automated validation; machine qualification remains pending.

> **Current startup policy:** Windows direct install now configures an OS menu as
> the first firmware entry, with Omarchy / five seconds initially selected in the
> installer. Windows remains available through its original firmware entry, using
> a brief extra restart when selected. This supersedes the earlier
> append-and-preserve-default policy. Read [platform behavior](docs/platform-behavior.md)
> for user/support/team expectations. Boot execution testing remains deferred.

> **Current storage UI:** Windows and macOS now use the same selection, allocation
> and typed deletion controls. The Mac bridge accepts retained native alongside
> and existing-Omarchy replacement choices. See the
> [shared storage update](docs/evidence/shared-storage-2026-09-06.md).
> macOS compilation, signed packaging and execution qualification remain pending.

> **Latest addition:** Windows partition replacement now requires a warning modal
> and typed `Disk N Partition M` confirmation, followed by the existing native
> final plan approval. See [the deletion update](docs/evidence/deletion-confirmation-2026-09-06.md).
> Deletion/locking, UI execution, installation and recovery tests remain deferred;
> static checks and compilation do not qualify destructive operations.

> **Latest authorized implementation:** The user additionally requested upstream
> default encryption, automatic conditional BitLocker suspension/restoration,
> selectable space and native NTFS shrinking. See the
> [encryption and allocation update](docs/evidence/encryption-allocation-2026-09-06.md).
> This supersedes the earlier fixed-size, unencrypted, free-space-only limits.
> Keep execution tests deferred: source review, syntax/type checks and compilation
> are permitted; do not launch installation, elevate, change disks/firmware,
> change BitLocker, schedule recovery tasks on this host, or reboot.

> **Current implementation:** [September 6 integration report](docs/evidence/native-options-2026-09-06.md)
> records the native USB and Windows direct-install flows, privileged helper,
> Apple bridge/client and deferred execution/qualification. The user asked to get
> both options implemented and leave testing for later. Builds/static checks are
> permitted; no installation/elevation/reboot was performed. Mac signed packaging
> needs a Mac. Read this report before historical state below.

> **Current plan — read first:**
> [Installation and USB strategy](docs/installation-plan.md) reflects the user's
> September 5 acceptance of local x86 image construction from the official ISO,
> followed by native deployment and first-boot finalization. No custom ISO or
> hosted root image is planned; avoiding an ISO boot partition is a design goal,
> with temporary host files and construction work still required.
> v1 keeps download-only, USB creation, and qualified direct installation behind
> one Tauri frontend, reuses the native Apple backend, and defers Try.
> USB creation plans one shared Etcher SDK backend on Windows, macOS, and Linux;
> it is the engine used by Omarchy's recommended Etcher app. Caligula is a
> fallback candidate only. Integration still requires qualification.
> [Strategy learnings](docs/installer-strategy-learnings.md) preserve the research.
> These documents supersede conflicting historical recommendations below. The
> user subsequently authorized implementation with selective delegation and a
> maximum of four active workers. Work now covers native verified downloads,
> file-backed Etcher integration, and the virtual-disk image-construction proof.

This document is the entry point for a new agent or maintainer. It consolidates
the working goal, the public research performed through September 5, the impact
of recent upstream developments, the current local implementation state, and
the decisions that must precede further work.

> **Current user boundary:** Implementation was explicitly resumed after the
> plan discussion. The current work package excludes physical disk and firmware
> mutation, publication, or upstream communication. Progress from actual tests,
> not the historical mock UI, determines what can be offered as implemented.

All external-state claims below are point-in-time findings. Reverify them before
making a product, architecture, publication, or upstream-coordination decision.

## 1. Historical review summary (superseded where the current plan differs)

The underlying goal remains valid: make it substantially easier and safer for a
Windows or macOS user to get from interest in Omarchy to a verified trial or
installation, while retaining a path to Linux-host support.

The original broad `Omarchy Installer` roadmap should **not** resume unchanged.
Between September 3 and 5, both Try Omarchy VM applications moved into the
official Omacom GitHub organization and advanced quickly. A new independent
application should not recreate or absorb their VM lifecycle, updater,
recovery, diagnostics, or platform integration.

The clearest unfilled first product is now a first-party-quality Omarchy media
creator/install companion for Windows and macOS:

1. Resolve and download the official Omarchy ISO and verification material.
2. Verify its signature, digest, exact length, and release identity before use.
3. Identify an eligible whole removable drive and exclude system/source disks.
4. Show and bind an exact destructive plan before elevation.
5. Write, durably flush, fully read back, verify, and eject the USB.
6. Explain firmware, Secure Boot/TPM, boot selection, and supported install
   choices.

A genuine no-USB/direct Windows installer is also still missing publicly and
remains part of the longer-term goal. It should be treated as a later,
separately qualified offline-install provider, not as part of the initial USB
release and not as disk mutation performed casually from the running Windows
desktop.

The VM trial experience is no longer a missing implementation. A future front
end may discover or launch the official Try applications if Omacom wants that
integration. Apple-Silicon direct-install work should likewise be integrated or
handed off rather than independently reimplemented.

## 2. Goal recorded before the upstream change

The September 2 project roadmap, subsequently revised on September 5,
defined a community-incubated cross-platform onboarding product with replaceable
platform backends and these historical user outcomes:

1. Try Omarchy in a disposable or persistent VM.
2. Install directly where a qualified platform backend exists.
3. Download, verify, write, verify, and eject an installation USB.
4. Download and verify an image without writing it.
5. Resume, repair, remove, or export diagnostics for operations the product
   owns.

It already contains a crucial non-goal: integrate maintained platform engines
rather than race them. Recent official adoption makes that boundary much more
important and calls the value of a broad standalone shell into question.

### Current interpretation of the goal

| Area | Current status | Recommended ownership |
| --- | --- | --- |
| Try Omarchy on macOS | Public official application exists | Omacom `try-omarchy`; link, launch, or contribute upstream |
| Try Omarchy on Windows | Public official application exists | Omacom `try-omarchy-windows`; link, launch, or contribute upstream |
| Verified USB creation on Windows/macOS | No public official Omarchy-specific implementation found | Strongest initial scope for this project |
| Verified USB creation on Linux | Two community implementations exist | Coordinate, contribute, or deliberately differentiate |
| Direct Apple-Silicon install | Active community and announced official work | Integrate/defer; do not rebuild its disk engine |
| Direct Windows install | No public implementation found | Keep as a later upstream-coordinated offline provider |
| No-USB Intel-Mac install | No public implementation found | Keep provisional and deferred |
| One unified official front door | No public implementation found | Product/ownership question for Omacom before building broadly |

## 3. Research findings as of September 5, 2026

### 3.1 Both Try applications are now under Omacom

#### macOS

- Repository: <https://github.com/omacom/try-omarchy>
- Latest normal release found: `v0.3.0`, published September 2:
  <https://github.com/omacom/try-omarchy/releases/tag/v0.3.0>
- On September 3, commit `d116f5c` removed the non-affiliation disclaimers:
  <https://github.com/omacom/try-omarchy/commit/d116f5c1d97a905646bcac7d9ba1ad5b514731e6>
- On September 5, commit `088ceae` adopted the official Omarchy mark:
  <https://github.com/omacom/try-omarchy/commit/088ceaea247d0b5858f5a41c3a5a3c9c666ba896>

The product is a native Swift/AppKit launcher for a hardware-accelerated ARM64
QEMU/HVF VM on Apple Silicon. It is a trial application, not a bare-metal
installer. Its project scope deliberately targets that native Apple-Silicon VM
experience:
<https://github.com/omacom/try-omarchy/blob/main/CONTRIBUTING.md>

#### Windows

- Repository: <https://github.com/omacom/try-omarchy-windows>
- The repository and readiness material state that the Windows and macOS apps
  now live under Omacom. A September 4 readiness commit records official v1
  alignment:
  <https://github.com/omacom/try-omarchy-windows/commit/a986a63af16dd95bf90dd6d806c801379258a3d2>
- `v0.0.13-preview` was released September 5:
  <https://github.com/omacom/try-omarchy-windows/releases/tag/v0.0.13-preview>

The public Windows app already covers a large part of the old Try/manage scope:

- Windows Hypervisor Platform/QEMU VM startup and supervision;
- verified and resumable runtime/guest downloads;
- authenticated update metadata, staged updates, health gating, and rollback;
- graphics probing and CPU fallback;
- settings, shared folders, clipboard, port forwarding, and SSH support;
- diagnostics bundles;
- backup, restore, reset, and uninstall;
- configuration export for migration to a real installation; and
- an experimental offline portable-USB VM mode.

The portable mode stores and runs the persistent **VM** from removable media. It
does not create a bootable Omarchy installation USB and does not install
Omarchy onto the computer.

#### Consequence

Do not build independent macOS or Windows VM engines in this repository. Do not
duplicate the official apps' update, backup, recovery, or VM-data ownership.
Any shared front end should use a simple verified handoff/deep link unless the
maintainers explicitly request a richer provider protocol.

### 3.2 The verified USB-writer gap remains

The official Getting Started manual still says to download the ISO, write it
with balenaEtcher on Windows/macOS or caligula on Linux, and boot from it:
<https://github.com/omacom/omarchy/blob/quattro/manual/02-getting-started.md>

The official ISO repository still describes the ISO as the only supported
installation route. It publishes a `.sha256` and a `.sig` alongside the ISO,
which a dedicated writer can preserve and verify as separate upstream trust
material:
<https://github.com/omacom/omarchy-iso>

No public Omacom repository implementing a generic Omarchy ISO-to-USB writer or
cross-platform installer shell was found during the September 5 audit. Public
`omarchy` changes after September 2 were unrelated to this install path, and
`omarchy-iso` had no commit after September 1 at the time of the audit.

### 3.3 USB-writer prior art and coordination risk

The USB work is not entirely greenfield:

- [OmaFlash](https://github.com/Bmontythe3rd/OmaFlash) is a Linux-only Rust,
  Quickshell, UDisks2, and Polkit writer. It supports checksum validation,
  stable device revalidation, durable synchronization, optional full readback,
  multi-target writes, cloning, backups, and recovery formatting. The audit
  found only two commits, a `v0.1.0` release from August 19, no visible user
  validation, and a GPL-3.0-only license. Its code cannot simply be copied into
  the proposed MIT project.
- [OmSticker](https://github.com/paulogeyer/omsticker) is a Linux-only Qt 6 and
  UDisks2 Omarchy writer. It downloads the latest ISO, blocks system disks,
  supports abort/resume, and can use leftover space as a data partition. The
  audit found nine commits and activity through September 4, but no GitHub
  release.
- Official Omarchy Discussion
  [#9145](https://github.com/omacom/omarchy/discussions/9145), opened August 30,
  proposes an official one-click product that downloads, checksums, and flashes
  the ISO. It had no response or published implementation on September 5. It
  validates demand but creates an obvious coordination point.

Recommended implication: prioritize the currently unfilled Windows/macOS
writer, contact the Discussion author and Omacom before public positioning, and
decide deliberately whether Linux should integrate an existing project, share
a media core, or be deferred.

### 3.4 Direct Windows installation is still missing publicly

The official Try application makes no partitions and changes no bootloader. Its
migration export transfers selected user configuration; it is not a VM-to-disk
conversion or bare-metal installer.

The documented Windows dual-boot journey still requires preparing free space,
booting installation media, and installing from the ISO:
<https://github.com/omacom/omarchy/blob/quattro/manual/50-dual-boot-install.md>

No public official direct/no-USB Windows installer repository, branch, or
implementation was found. DHH publicly said on September 2 that Windows work
was moving quickly, so private or unpublished work may exist:
<https://x.com/dhh/status/2095172848994103701>

Absence from public GitHub must not be interpreted as permission to race an
announced official engine. The best near-term Windows work that does not own
disk mutation is read-only preflight: UEFI/GPT, BitLocker/recovery readiness,
Fast Startup, free/unallocated space, storage controller/VMD/RST ambiguity,
power, and firmware guidance.

If a direct provider is eventually built here, its safe initial shape is:

1. Require UEFI/GPT and pre-existing unallocated space.
2. Download and authenticate a pinned offline installer and immutable plan.
3. Stage the installer without using the target area as its only source.
4. Configure a one-time reboot into the offline environment.
5. Re-identify disks and perform partition/boot mutation offline.
6. Preserve Windows bootability and provide an exercised recovery path.

Initial exclusions should include automatic partition shrinking, arbitrary
BitLocker changes, Storage Spaces, dynamic disks, unresolved Intel RST/VMD,
legacy BIOS, Windows on ARM, and whole-disk replacement. Physical qualification
and independent review are release gates, not follow-up tasks.

### 3.5 Apple-Silicon and Intel-Mac direct installation

DHH's September 2 announcement described an `Omarchy M` app that lets a user try
Omarchy and later choose how much disk space to dedicate:
<https://x.com/dhh/status/2095158161535590612>

Public bare-metal Apple-Silicon work remains active outside the Omacom org:

- <https://github.com/maralcbr/omarchy-mx-mac>
- Latest normal release found during the audit:
  <https://github.com/maralcbr/omarchy-mx-mac/releases/tag/v4.0.2-mac.1>
- Open upstream runtime-support PR found during the audit:
  <https://github.com/omacom/omarchy/pull/9835>

The project received substantial installer hardening after September 2, but its
physical acceptance remains model-specific. Do not translate one or several
successful models into generic M-series support.

Additional community work exists at
<https://github.com/omarchy-mac/omarchy-mac> and
<https://github.com/omarchy-mac/omarchy-mac-iso>. The latter documented a
two-stage NVMe/no-USB path after prior Asahi UEFI provisioning. These are useful
evidence and possible integration partners, not proof of a universal native
macOS installer.

The official Mac manual still says M-series direct installation is unsupported
and documents Intel Mac installation through bootable USB with platform caveats:
<https://github.com/omacom/omarchy/blob/quattro/manual/44-mac-support.md>

No public native no-USB Intel-Mac installer was found. Keep it provisional and
prefer the official ISO/USB route until a maintained engine and hardware matrix
exist.

## 4. Recommended revised product boundary

### Initial product: verified media creator/install companion

Target Windows and macOS first. Preserve Linux in the architecture, but do not
duplicate OmaFlash or OmSticker by accident.

The initial product should own only:

- canonical release discovery and explicit version selection;
- upstream signature, checksum, length, and provenance verification;
- resumable download and a verified content-addressed cache;
- removable whole-device enumeration and fail-closed target classification;
- immutable, user-reviewed destructive plans;
- narrowly allowlisted platform elevation helpers;
- streaming raw writes, durable flush, complete readback verification, and
  ejection;
- crash-safe operation records and locally previewable redacted diagnostics;
- boot/install preparation and exact next-step guidance.

It should not initially own:

- VM creation, runtime updates, guest recovery, or VM deletion;
- Apple-Silicon partitioning/install logic;
- Windows partition shrinking or boot mutation;
- Intel-Mac direct installation;
- a private replacement for upstream release authenticity; or
- branding that implies official status without Omacom permission.

### Later product: direct Windows provider

Keep direct Windows installation as a stated goal, but make it a later provider
with its own threat model, offline architecture, support cells, recovery plan,
and hardware evidence. Prefer integrating an official engine when it becomes
public. A Windows preflight/export-to-USB journey can ship earlier without
claiming direct installation.

### Optional shell: official handoffs

A unified shell is valuable only if users and upstream maintainers want one.
Its safe minimum is discovery and verified launch of the official Try apps and
qualified direct installers. Avoid inventing a universal provider lifecycle
until the actual maintainers agree to support it.

## 5. Impact on the existing roadmap

| Existing area | Recommended change |
| --- | --- |
| Objective and product name | Reframe around verified media creation/install companionship; keep the broad name provisional |
| Phase 0 | Make upstream ownership and Discussion #9145 coordination a real decision gate before public positioning |
| Phase 1 | Keep the pure domain, catalog, download, journal, diagnostics, fixtures, and dry-run safety work; revise Try/Install mock flows to represent external ownership accurately |
| Phases 2–4 | Preserve the simulated and physical USB safety progression; this is the strongest surviving core |
| Phase 5 | Replace generic Try-provider development with optional verified handoff to official Omacom apps |
| Phase 6 | Preserve read-only install preflight, especially for Windows |
| Phase 7 | Treat Apple-Silicon direct installation as upstream/community integration, not an owned engine |
| Phase 8 | Preserve direct Windows as a later offline provider; keep Intel Mac provisional |
| Phase 9 | Reassess Linux scope against OmaFlash and OmSticker before implementation |
| Phase 10 | Manage only resources and operations this project actually owns |

The old Try repository links in the original roadmap and `docs/support-matrix.md` are now
stale. Do not mechanically update them until the broader roadmap revision is
approved; their staleness is evidence that the current documents are a dated
baseline, not the final plan.

## 6. Current repository state

Repository path:
`C:\Users\diogo\Documents\Code\omarchy`

### Git and handoff constraints

- Git is initialized on an unborn `main` branch.
- There are zero commits, zero tracked files, and no remote.
- Every source, document, fixture, and build cache is currently untracked.
- A new Git worktree cannot recover this state from history. A handoff task must
  use this exact saved checkout until the user approves a baseline commit or
  another deliberate preservation mechanism.
- Untracked build caches include `target-device-policy/`,
  `target-domain-check/`, `target-journal-agent/`, and
  `target-provider-contract/`. The current `.gitignore` excludes only `/target/`,
  so a broad `git add` would capture those caches. Do not stage or delete them
  without first resolving the exact intended file set.
- Nothing has been published, pushed, or posted upstream.

### What exists locally

Substantial non-destructive foundations and a simulated desktop exist:

- `apps/desktop`: Svelte/TypeScript views for Overview, Try, Download, USB,
  Install, and Manage, plus a minimal Tauri 2 shell. The shell exposes no native
  commands; journeys run through an in-memory simulation.
- `crates/domain`: host facts, capabilities, policies, immutable operation
  plans/digests, and guarded state transitions.
- `crates/catalog`: verification of this project's custom domain-separated
  Ed25519 catalog envelope without network or filesystem I/O. It does not yet
  establish compatibility with the upstream ISO `.sig` format; trusted-key
  distribution and the bridge between upstream verification material and this
  catalog remain design and implementation decisions.
- `crates/downloader`: verified artifact ingestion, resume metadata, and a
  content-addressed cache; it does not perform network requests.
- `crates/device-policy`: fail-closed removable whole-device selection and
  revalidation, including system/source topology protection.
- `crates/media-writer`: streaming write, durable flush, and full readback over
  an abstract `BlockDevice`; it cannot open a physical disk.
- `testkit/fake-block-device`: memory/file devices and injected short I/O,
  generic errors, and corruption.
- `crates/provider-contract`: typed lifecycle/protocol models and a simulated
  provider; no process execution or dynamic provider loading.
- `crates/ipc`: strict messages, immutable plan bindings, replay guards, and
  operation tracking; no OS transport, elevation, or disk/network integration.
- `crates/journal`: append-only JSON-lines hash chains and durable head anchors.
  These hashes are not signatures and do not prevent replacement of both the
  journal and anchors.
- `crates/diagnostics`: local in-memory sanitized bundles; no upload or network
  API.
- `testkit/synthetic-disks` and `testkit/synthetic-hosts`: schemas, fixtures,
  and PowerShell validators for Windows, Macs, Linux, BitLocker, sector sizes,
  and removable/system-drive scenarios.
- `docs`: architecture, threat model, governance, support matrix, tester policy,
  hardware evidence playbook, ADRs, and an upstream RFC draft.

### What does not exist or is not established

- No native Windows/macOS/Linux raw-disk backend.
- No privileged helper or actual elevation path.
- No production network downloader or live upstream catalog resolver.
- No VM integration with the official Try applications.
- No direct Windows, Apple-Silicon, Intel-Mac, or Linux installer.
- No signed installers, production update channel, release keys, or hardware
  qualification.
- No physical USB test evidence.
- No integrated milestone verification after the interrupted September 2 work.

Tests are present throughout the Rust crates, the fake-device writer, and the
frontend model, but this handoff does not claim that the complete suite is
green. The September 5 inventory intentionally did not run it. There is no
`pnpm-lock.yaml`; the current CI frontend job is gated on that file and would
skip. The Tauri shell is also an independent Cargo workspace, so root workspace
tests do not cover it.

Concise status: **substantial pure/simulated foundations exist; native
integration, production artifact transport, physical USB writing, direct
installation, signed packaging, and hardware qualification are not
established.**

## 7. Documents to read before deciding

Read these in order:

1. This `HANDOFF.md`.
2. [README.md](README.md) — current public-facing status.
3. [docs/upstream-rfc.md](docs/upstream-rfc.md) — pre-change coordination draft;
   do not post in its current form.
4. [docs/architecture.md](docs/architecture.md).
5. [docs/threat-model.md](docs/threat-model.md).
6. [docs/support-matrix.md](docs/support-matrix.md).
7. The ADRs under `docs/adr/`, especially provider architecture and privilege
   separation.

## 8. Required first task for the successor agent

The successor should not start by writing code. Its first deliverable to the
user should be an independent, evidence-backed review that:

1. Revalidates the time-sensitive upstream facts in this handoff.
2. Checks whether the existing architecture still fits the narrowed product.
3. Identifies which local foundations are reusable, overbuilt, or misdirected.
4. Proposes an exact roadmap diff rather than silently rewriting the goal.
5. Makes a clear recommendation on whether to keep a standalone Tauri shell,
   build only a shared Rust media core, or contribute platform-specific flows
   upstream.
6. Gives direct Windows installation an explicit position, architecture, and
   gate rather than omitting it.
7. Waits for the user's authorization before modifying implementation files.

After approval, update the roadmap and public documents before resuming the
implementation. Preserve the user's untracked work, avoid broad staging, and do
not publish, create remotes, post the RFC, contact maintainers, or perform any
real-device/destructive operation without the corresponding user authority.

## 9. Decisions still needed

The next substantive user decision should cover:

1. Is the product primarily a Windows/macOS verified media creator, or should it
   still present a broader unified shell?
2. Should direct Windows installation be an explicit later deliverable of this
   repository, or only an integration point for forthcoming Omacom work?
3. Should the project seek Omacom alignment before any further code, or continue
   only the clearly non-overlapping simulated/media core while waiting?
4. Should Linux be deferred, integrated with an existing writer, or supported
   through a shared core?
5. Which existing scaffolding should be retained once the revised roadmap is
   accepted?

## 10. Research provenance

- Primary audit date: 2026-09-05.
- Public repositories, commits, releases, manuals, and discussions were used as
  primary evidence wherever possible.
- Five independent research subtasks used model ID `gpt-5.6-sol` to audit
  official changes, Windows, Macs, USB competitors, and repository deltas.
- The final local implementation inventory used model ID `gpt-6-astra` and was
  read-only.
- No project files were modified during the September 5 research audit. This
  handoff document is the first deliberate project-file change after that
  pause.
