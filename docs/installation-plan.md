# Installation and USB implementation strategy

Updated: 2026-09-06 (Europe/Lisbon).

**Current priority:** establish local system construction from the official ISO,
then native Windows deployment, with personal configuration at first boot.
The [latest decision](direct-install-priority-2026-09-06.md) supersedes the earlier
staging-first proposal and records why direct control of BitLocker maintenance
is preferable, without claiming boot-related recovery risk is eliminated.
Staged installation and published prepared images remain secondary options.

The [September 8 retained learnings](installation-options-learnings-2026-09-08.md)
summarize virtualization, prebuilt-image and encryption tradeoffs from subsequent
discussion; they do not select a replacement architecture.

The [September 12 consolidated brainstorm](direct-install-brainstorm-2026-09-12.md)
is the entry point for both direct-install routes, image-source variants,
Libertix findings, BitLocker/boot distinctions and unresolved evidence. Read its
corrections before revisiting the architecture; no new route has been selected.

Status: both setup options are wired for development. The shared Etcher physical
USB provider, native elevation/staging helper, Windows local construction and
GPT deployment, and Apple native bridge/client are implemented. The user explicitly
deferred new tests and installation execution. No real disk or firmware mutation
was performed. Mac execution additionally needs a macOS build and correctly signed
packaging. Linux and Intel Mac direct deployment are not implemented.

Current results, review fixes and qualification limits are recorded in the
[September 6 integration report](evidence/native-options-2026-09-06.md).
The earlier virtual-disk construction proof remains incomplete; there is no
independent boot or physical installation evidence for the new recipe.

## Product scope

One Tauri interface on Windows, macOS, and Linux provides download-only,
automatic download plus USB creation, and direct installation where qualified.
Try is outside v1. A VM used internally to construct an installed system is a
build component, not a user-facing Try feature.

Use native backends where host storage or firmware requires them. Preserve the
Apple installer's coordinator, inspection/state logic, authenticated helper,
Asahi engine, and Recovery handoff behind the shared frontend. The bridge and
bundle assembly are implemented; macOS compilation, signing and qualification remain.

## How upstream dependencies are included

- Mac native backend source: a Git submodule pinned to an inspected upstream
  commit, with our integration adapter outside that checkout. The native provider
  now retains commit `00daf3ebef9e4fbe89fb9b32bd184e65aa75d357` with separate published
  engine/catalog pins documented in `providers/direct-apple/`.
  A narrow fork is only needed if concrete integration changes require one.
- Etcher SDK: a version-pinned package dependency with its lockfile and native
  build/runtime dependencies, not a submodule of the complete Etcher application.
- Asahi engine and Mac payloads: exact verified release artifacts, separately
  pinned from the Mac source commit. A release tag alone did not describe the
  released engine in the inspected Mac release.
- Official x86 ISO: downloaded release plus checksum and detached signature,
  verified with the pinned upstream public key. It is not stored in Git.

Upstream pin updates are reviewed and tested explicitly; a submodule does not
automatically supply later upstream fixes. Preserve source/version/license
provenance for both package dependencies and downloaded release components.

## Preferred approach: local construction, then deployment

The intended alongside-installation sequence is:

1. Download and verify the official Omarchy ISO in the ordinary host cache.
2. Run its reusable installation logic and bundled packages in a local Linux
   environment to construct an installed system on a file-backed virtual disk.
   A disposable VM is the leading mechanism to evaluate; do not attach any
   physical host disk to it.
3. Export the required partition images, verify their layout/content, and record
   source ISO digest, builder/tool versions, configuration, and output hashes.
4. Reinspect the destination, show the concrete allocation/deployment plan, then
   use the native helper to shrink/allocate supported space and write only the
   authorized destination extents. Include permanent boot configuration.
5. Boot the installed system on the actual hardware and complete the remaining
   hardware, filesystem, encryption, or user setup defined by the recipe.
6. Verify that the installation boots independently of the ISO, build VM, and
   host cache. Clean only operation-owned temporary files after success.

Preserve upstream's complete deferred owner workflow, required provisioning and
boot state, and factory baseline. Its inspected x86 state lives inside the root
filesystem and boot artifacts, rather than a dedicated configuration partition.
Do not replace its forms or discard setup state during deployment. Existing
hardware/capacity finalization must finish before handing off to upstream owner
setup; retry and factory-reset compatibility remain qualification requirements.

The ISO is the upstream source artifact; this plan does not publish a custom
ISO or a separately maintained downloadable Omarchy root image. It does require
a maintained local construction recipe, a packaged Linux runtime, and tested
deployment/finalization logic. This has not yet been demonstrated.

The intended benefit is avoiding a dedicated partition used to boot the ISO
on the physical machine. Temporary ISO, virtual-disk, and exported-image files
still require host storage, with space estimation and cleanup. Separate boot
partitions required by the installed OS are not temporary ISO staging partitions.

### What the official ISO does

The [ISO builder](https://github.com/omacom/omarchy-iso/blob/quattro/builder/build-iso.sh)
creates a live installation environment and an offline package repository. It
does not package a finished desktop filesystem that can simply be extracted.
The [installer](https://github.com/omacom/omarchy-iso/blob/quattro/configs/airootfs/usr/share/omarchy-iso/orchestrator/phases_impl.py)
creates filesystems/encryption, installs packages and their hooks, configures
hardware and users, and generates boot files and snapshots. Disk-space
preparation in Windows does not already perform those steps.

The [unattended configuration](https://github.com/omacom/omarchy/blob/quattro/manual/51-unattended-installs.md)
and deferred provisioning are reuse candidates for construction in the VM.
Qualify a pinned released ISO; support in the moving upstream source is not
proof that every downloadable version has the same contract.

Existing [Try x86 artifacts](https://github.com/omacom/try-omarchy-windows)
and builders remain useful references. Their VM configuration and ext4 payload
must not be treated as already equivalent to the bare-metal Btrfs/encryption
installation. No new distro or package collection is being proposed.

### Feasibility gates before enabling direct installation

- Produce and independently boot a system from a pinned official ISO using
  only file-backed disks; pin and verify all executable build dependencies.
- Separate generic setup from actual hardware setup. Driver detection in the
  build VM must not leave a physical machine with VM-only boot assumptions.
- Define how target identifiers, boot files, filesystem growth, and snapshots
  survive deployment onto a different partition layout.
- Preserve per-install encryption keys and identities. Never distribute or
  reuse an encrypted template with a shared volume key. Define secure secret
  handoff and disposal, including any unattended-configuration files.
- Establish the minimum CPU virtualization, RAM, free space, and host-runtime
  requirements. Account for construction time as part of the installation UX.
- Define how the helper verifies the local output and its provenance, rather
  than accepting arbitrary caller-supplied image paths or hashes.
- First qualify Windows x64 UEFI/GPT alongside deployment into unallocated
  space; qualify native NTFS shrinking separately, then add supported layouts
  on Linux and Intel Macs. Full replacement remains a separate later mode.
- Test interrupted construction, interrupted writes, boot failure, independent
  recovery to the preserved OS, and cleanup ownership.

After investigating staging, the user now favors direct construction/deployment
again. The [latest September 6 decision](direct-install-priority-2026-09-06.md)
preserves staging's temporary partition, installer integration and cleanup costs.
The construction gates above remain open for the preferred approach; execution
testing remains deferred.

## USB creation: reuse the upstream-recommended writing engine

Omarchy's [getting-started guide](https://github.com/omacom/omarchy/blob/quattro/manual/02-getting-started.md)
recommends **balenaEtcher on Windows/macOS** and **Caligula on Linux**. The
Etcher SDK recommendation refers to the engine used by that same Etcher app.
Etcher also supports Linux. Those upstream usage recommendations do not require
us to integrate two engines: the planned backend is **Etcher SDK on all three
hosts**, with platform-specific privilege adapters. Caligula is a contingency
only if a demonstrated Etcher limitation warrants reconsidering this decision.

Preferred integration: keep our Tauri UI, downloader, target policy, and operation
records; call a pinned upstream writing engine through a narrow packaged adapter.
Prefer an ordinary dependency over copying source or forking a complete GUI.
If changes are necessary, keep a small patch with recorded provenance and seek
upstream inclusion when communication is authorized. Upstream maintenance does
not automatically update a fork or qualify our packaging/integration.

| Candidate | Evidence and fit | Planned position |
| --- | --- | --- |
| [Etcher SDK](https://github.com/balena-io-modules/etcher-sdk) | Separate Apache-2.0 SDK used by cross-platform Etcher, with device scanning, block-device writing, progress, and optional verification. Node/native modules need packaging. | Planned shared engine for Windows, macOS, and Linux, subject to qualification. |
| [Caligula](https://github.com/ifd3f/caligula) | Omarchy's Linux recommendation; Rust CLI with noninteractive options and read-back verification. Windows is not supported; macOS support is limited. GPL-3.0. | Contingency only; no second production backend or Windows port is planned. |
| [Fedora Media Writer](https://github.com/FedoraQt/MediaWriter) | Cross-platform Qt/C++ writer with separate helper and raw image workflow; GPL/LGPL components. | Alternative if the preferred integrations fail their packaging/API gates. |
| [Rufus](https://github.com/pbatard/rufus) | Windows-focused full application with broader ISO transformation features; GPL-3.0. | Reference or external fallback; no full fork planned. |

The SDK repository was not archived and showed activity through September 3,
2026 when inspected. Its package metadata reported version 10.2.14. Etcher's
own app currently pins an older SDK, so inspect and test the chosen version
rather than treating app behavior as proof about every SDK release.

Etcher already [packages a helper process](https://github.com/balena-io/etcher/blob/master/forge.sidecar.ts),
which supports the feasibility of a Tauri-side adapter. This is packaging
evidence, not a stable public CLI contract. Do not adopt the example/default
control interface as our privileged protocol without adapting it. The SDK's
sample writer leaves verification optional; our integration must require it.

### USB workflow and responsibilities

The user additionally requested **Keep files and add installer** alongside the
existing erase mode. See [the preservation design](usb-preserve-files-2026-09-06.md)
for boot/filesystem constraints and the candidate implementation sequence.
Preservation is not implemented by the current raw writer; a separate backend
must establish compatibility and retain existing data and supported boot choices.
The following workflow describes the existing erase mode only.

```text
resolve official ISO -> download/resume -> verify source
-> select eligible whole USB device -> confirm erase -> elevate
-> reidentify and lock/unmount target -> raw write official ISO
-> flush -> read back and verify full image length -> eject -> receipt
```

USB creation writes the official ISO as a disk image. It does not use the local
installed-system construction process. No separate USB formatting wizard or
custom bootloader transformation is required for the qualified official image.

Our adapter must enforce source/system-disk exclusion, stable device identity,
capacity and sector alignment, exact write bounds, mandatory verification,
cancellation semantics, and honest failure reporting. The privileged API accepts
only local verified artifacts and the approved plan, with no general shell,
network download, or arbitrary write endpoint. A selected SDK's helpers/native
modules must satisfy this boundary on each host.

Before selecting the production engine, prove packaged execution, progress and
error reporting, target revalidation, mandatory read-back, and ejection on each
supported host. If a later decision switches to Caligula, assess how to expose
progress through our UI without relying on fragile terminal-output scraping.
Record component-specific licenses, exact versions and modifications in notices;
do not assume all engine source can be relicensed under the app's own license.

The existing Rust media-writer and fake-block-device crates are useful test
assets. They are not a reason to build and maintain a second physical writer
when a suitable maintained engine can be integrated. The initial implementation
now exercises the pinned Etcher SDK against regular files; physical-device
integration remains unavailable.
