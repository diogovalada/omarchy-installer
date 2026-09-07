# Installer strategy learnings

Recorded: 2026-09-05 (Europe/Lisbon).

September 6 follow-up: [direct-install priority and staging findings](direct-install-priority-2026-09-06.md)
records the latest preference for native deployment with deferred first-boot
owner setup, stock same-disk/BitLocker restrictions, and the shared boot-recovery
risk. It supersedes the intervening staging-first proposal.

This records the user discussion following the independent review of HANDOFF.md.
Distinguish the user's intended product, recommendations, verified source facts,
and decisions still open. Recording these notes does not authorize implementation,
publication, or disk operations. Earlier roadmap and handoff recommendations are
historical where they conflict with this document.

## Product direction from the discussion

- The product is a desktop setup application for Windows, macOS, and Linux. It
  offers downloading an image, downloading and writing installation USB media,
  and direct installation on the current computer where supported.
- The user proposed leaving Try Omarchy integration out of v1 and revisiting it
  later. This is the working v1 scope direction. Do not keep developing a Try
  mockup as though it were required for v1.
- Keep an explicit download-only option. The user pointed out that the other
  features already require downloading and verification, making this a small
  additional workflow. Downloading should also happen automatically within USB
  creation; users should not need to find an ISO URL or install a separate flasher.
- The user suggested replacing the Mac installer's native frontend with one
  shared GUI, such as Tauri. The assessment supports this approach while retaining
  useful native backend code. It does not require rewriting everything in Rust.
- Cross-platform host support does not imply every direct-install operation works
  on every host. Distinguish the computer running the app from the destination
  computer for which it creates media, and distinguish Apple Silicon from Intel Macs.
- In the subsequent discussion, the user accepted local construction of an x86
  installed image from the official ISO, followed by native deployment, as the
  direction to put in the plan. The exact builder, hardware/support matrix,
  encryption/finalization design, and implementation remain unqualified.
  See [the updated implementation strategy](installation-plan.md).

## Terminology that caused confusion

There are two relevant installers:

1. Our proposed desktop setup application, launched from the existing OS.
2. The Linux installer that runs after booting the official Omarchy ISO.

When the user asked whether to fork the Mac installer, they meant the macOS
desktop application that prepares a native installation, not the installer inside
the ISO. The initial answer addressed the wrong component and must not be treated
as a reason to reject reusing or forking the Mac desktop installer.

For this discussion, direct installation means installing on the current computer
without requiring a USB stick. It does not necessarily mean no reboot or that every
installation operation occurs inside the running host OS.

## What exists locally

The local project is a standalone prototype, not an Omacom fork. At review time it
had no commits or remote. Useful Rust libraries exist, but the desktop workflows
are not connected to real downloading, physical disk writing, or installation.

In particular, the Try screen changes local RAM/persistence controls and opens a
generic dialog. Its Run simulation button only sets a completion Boolean; it does
not run the Rust provider simulator, validate a real plan, or launch a VM. Calling
it a simulated workflow overstated its functionality. The user challenged its
value; do not count it as implemented Try support or meaningful validation.

Evidence: apps/desktop/src/views/TryFlow.svelte,
apps/desktop/src/lib/PlanModal.svelte, and
apps/desktop/src-tauri/src/lib.rs. No test suite or physical installation was run
during this discussion.

## The public Mac installer is a concrete reuse candidate

The inspected project is
[maralcbr/omarchy-mx-mac](https://github.com/maralcbr/omarchy-mx-mac/tree/main/apps/omarchy-apple-installer).
Its
[September 1 release](https://github.com/maralcbr/omarchy-mx-mac/releases/tag/v4.0.1-mac.2.9.090126)
includes a downloadable Omarchy-MX-Mac-Installer-4.0.1-mac.2.9.090126.pkg.

Public Omacom repository enumeration did not identify a separate native Omarchy M
installer repository. Do not equate this community implementation with the exact
announced Omacom application without further evidence. The two official Try apps
are separate VM applications.

Source inspection and read-only inspection of the
[released .7 engine archive](https://github.com/maralcbr/omarchy-mx-mac/releases/download/v4.0.1-mac.2.9.090126/installer-v0.9.0-omarchy.7.tar.gz)
established that the Mac engine does real installation work:

- Select eligible space on the system disk and, when necessary, shrink an APFS
  container using macOS storage tools.
- Create an APFS boot/recovery stub and the Linux installation partitions.
- Deploy EFI/firmware content and prepared boot.img and root.img system images,
  then read back installed content for verification.
- Configure the boot target with Apple's bless tool and prepare the Recovery
  handoff. A human Recovery authorization step is still required.

This is image deployment, not merely placing an ISO on disk for the next boot.
Current UI code lets the user adjust the allocation, automatically ranks free
space ahead of resize candidates, and refuses an existing Omarchy installation.
Engine-level replacement support must not be confused with a UI-exposed option.

Important provenance limitation: the release tag contains older UEFI-only source,
while its attached .7 engine implements full-system image deployment and boot
authorization. Current main also implements the full path. Inspect and pin the
actual release components; the tag alone does not describe the downloadable
engine. The .pkg binary was not audited or installed, and no physical compatibility
claim follows from this source review.

## Replacing the Mac frontend

Recommended boundary:

Shared Tauri screens -> narrow native adapter -> retained Swift coordinator/state
machine -> authenticated privileged helper -> pinned Asahi engine.

The existing
[InstallerEnvironment protocol](https://github.com/maralcbr/omarchy-mx-mac/blob/main/apps/omarchy-apple-installer/Sources/OmarchyInstallerUXCore/InstallerEnvironment.swift)
is a useful screen/backend boundary. It is a Swift protocol, not an already
available external JSON or CLI integration API.

Preserve the nonvisual logic in LiveInstallerEnvironment, EngineInspectionRunner,
InstallerSession, TrustCore, and the helper. Some of it lives in the application
target despite being essential backend behavior. Replacing the views must not
discard retained plans, approval/state checks, host reinspection, or recovery logic.

A native bridge or a signed companion executable is feasible. A companion adds an
IPC boundary. Neither option can simply bypass the existing app/helper signing
requirements; packaging, caller identity, and authorization must be adapted.
Whether to maintain a narrow fork or contribute an integration boundary upstream
remains open. The existing local prototype is not a reason to reject useful
upstream code or to preserve its own abstractions unnecessarily.

## Direct installation: corrected comparison

The earlier recommendation to use native image deployment on Macs and the ISO
installer on Windows/Linux was too rigid. Reusing the ISO is an implementation
and maintenance choice, not a general Windows/Linux requirement.

### Prepared-image deployment

The Mac-style strategy is feasible in principle for Windows and suitable Linux
layouts: allocate space, write prepared Linux partition images, configure boot,
then finish machine-specific setup on first boot.

Windows does not need to understand Btrfs or another Linux filesystem merely to
copy its raw image. Microsoft's
[WriteFile documentation](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-writefile)
explicitly allows appropriate raw writes outside volume extents or into eligible
unmounted/RAW volumes. Windows also provides
[native NTFS shrinking](https://learn.microsoft.com/en-us/windows-hardware/drivers/storage/msft-partition-resize).
The fact that Windows runs elsewhere on the same disk is not a categorical blocker
to installing alongside it in unused space.

The additional work is a suitable x86 image and deployment/first-boot pipeline:
machine identities, user provisioning, filesystem growth, boot configuration,
hardware setup, and encryption. Some upstream components can be reused; this does
not imply every function must be invented here.

Later clarification: "maintaining an image" does not imply publishing a custom
ISO. The accepted plan first constructs an installed filesystem locally from
the downloaded official ISO, likely in a disposable Linux VM, then deploys it.
We maintain the construction/deployment recipe and runtime integration, not a
separately hosted OS image. The ISO contains a live environment and package
repository; copying a section does not produce a ready installed desktop.
Existing x86 Try rootfs artifacts also exist but have VM-specific assumptions.

This can avoid a dedicated partition used to boot the ISO. Temporary host cache,
virtual disks, and images still require storage and cleanup. The ISO-based boot
alternative cannot format its own source partition while it still needs source
data; entering the live environment does not establish that all data is in RAM.

Encryption needs unique state per installation. Cloning one pre-encrypted LUKS
image and merely changing its password retains the same underlying volume key.
A proper design needs per-install encryption initialization or re-encryption;
this may use a small Linux initialization stage rather than the whole ISO installer.
See the [Cryptsetup FAQ](https://gitlab.com/cryptsetup/cryptsetup/-/blob/main/FAQ.md).

### Rebooting into the official ISO installer

This reuses upstream's package installation, filesystem/encryption setup, hardware
configuration, boot setup, and provisioning. It avoids maintaining an additional
deployed-image pipeline, but adds temporary boot/staging/cleanup work. It is not
automatically simpler overall.

There is concrete source support for booting a stored ISO: Omarchy's
[GRUB loopback configuration](https://github.com/omacom/omarchy-iso/blob/quattro/configs/grub/loopback.cfg)
passes img_dev and img_loop, and its initramfs includes archiso_loop_mnt. A complete
Windows no-USB launch path was not built or tested. Merely saving an ISO and adding
an ordinary Windows boot entry is insufficient.

The GUI can gather choices before reboot even when execution occurs afterward.
Upstream's
[cidata autoinstall support](https://github.com/omacom/omarchy/blob/quattro/manual/51-unattended-installs.md)
offers a possible configuration handoff, but does not itself implement a secure,
identity-revalidated plan transfer from our app.

### Constraints that apply to either design

- Installing into unused space differs from erasing the running OS. Do not use
  restrictions on the latter as proof that the former must happen offline.
- Whether resizing requires going offline depends on the existing filesystem and
  layout, not merely the host OS name. Rebooting does not make every filesystem
  shrinkable.
- A staging volume on the target disk is not independent installation media. A
  whole-disk wipe must not destroy files the installer still needs. Reaching a
  live environment alone does not prove all source dependencies are in RAM.
- Suspending BitLocker does not decrypt its data. An ISO boot design must establish
  that both the bootloader and Linux environment can read the chosen staging area.
- Hardware eligibility and actual disk identities must be checked at the point
  of mutation; user choices collected earlier do not replace those checks.

## Current plan and remaining feasibility work

Use a common Tauri frontend and reuse the Mac native backend. Keep download-only
and USB creation in scope; leave Try for a later discussion.

For x86 direct installation, first prove local image construction using the
official ISO and upstream scripts, native deployment, and first-boot finalization.
This direction has been accepted for the plan, not implemented or qualified.
Staged-ISO boot remains an alternative if the preferred design fails its gates.
Do not describe VM-specific setup as proof of bare-metal hardware readiness.

For USB creation, the user asked about maintained reuse and specifically
Omarchy's recommended tools. Upstream recommends balenaEtcher on Windows/macOS
and Caligula on Linux. Etcher SDK is the engine used by the recommended Etcher
app, and Etcher also supports Linux. The user prefers one shared engine, so the
plan selects a pinned Etcher SDK plus a narrow adapter for all three hosts,
subject to packaging/API qualification. Caligula lacks Windows support and has
limited macOS support; it is a contingency, not a second planned backend.
Prefer a dependency over a full UI fork, and retain our own target, verification,
privilege, and operation-state policy. See [the plan](installation-plan.md) for
primary sources and the comparison.

Prefer one qualified installation method per supported scenario. The recommended
consumer choices are target, allocation, and preserve/replace where supported,
rather than asking users to choose installation internals. This is a recommendation,
not a user-mandated prohibition on advanced options.

Research assistance used model ID gpt-6-astra. Findings are based on local source,
upstream source/documentation, and read-only released-engine inspection. No
installer, privileged helper, firmware mutation, physical disk write, publication,
or upstream contact was performed.
