# Installation options: retained learnings

2026-09-08. Condensed from the architecture discussion, including its corrections.
These are findings and options, not a decision to replace the current design.

## Current approach and performance

- The discussed build chain is Windows → Docker → QEMU → official Omarchy
  installer. Removing Docker and removing the inner VM are separate changes.
- Hardware acceleration is the main performance opportunity. The current recipe
  uses software emulation (TCG); one build timed out after 30 minutes. That is a
  failed attempt, not a normal installation time or an accelerated benchmark.
- Image export, deployment and verification still require substantial disk I/O.
  Installing directly after reboot can avoid intermediate-image passes, but adds
  boot, deployment and recovery integration.

## Virtualization alternatives

| Route | Retained tradeoff |
| --- | --- |
| Native Windows QEMU + WHPX | One VM; preserves the official installer. Candidate to benchmark first. Windows Home supports WHPX without Docker or WSL. |
| Dedicated WSL2 + QEMU/KVM | Removes Docker; accelerated QEMU is nested virtualization. KVM must be tested, not merely detected. Keep build files in WSL's Linux filesystem. |
| Build directly inside WSL2 | Removes the inner VM, but requires adapting the installation process to WSL. |

- Both WHPX and WSL2 require firmware virtualization and Windows features that
  cannot be assumed enabled. Initial setup may require administrator access and
  a restart. An already-working WSL2 environment normally needs no restart to
  import our distribution. Firmware changes may require a BIOS/UEFI visit.
- No measured ranking exists between native WHPX and nested KVM. A successful
  KVM capability query and fast WSL command launches do not prove build speed.
- Discussed builder baseline: 10 GiB free RAM (6 GiB guest), 85 GiB temporary
  space after ISO staging, plus roughly 40 GiB minimum destination space. These
  are recipe constraints, not WSL minimums; remeasure before changing them.

## Reusable images and encryption

- A generic signed image could move repeated installation work into our release
  pipeline. Personal setup, drivers, growth, snapshots and factory reset can be
  retained with deliberate integration. A raw Btrfs image fixes the filesystem;
  Ext4 needs a separate image or file-based deployment.
- **Never treat a password change as a fresh encryption key.** Cloned LUKS images
  share their volume key until actual re-encryption. Each installation needs a
  unique key and removal of template unlock credentials.
- The inspected Omarchy first-boot flow changes credentials for existing
  encryption; it does not encrypt plaintext installations. The official installer
  creates encryption earlier.
- Two custom deployment options: create fresh LUKS and write through its mapping;
  or deploy plaintext and encrypt offline from a separate Linux setup environment
  after reboot. The latter could remove Windows virtualization requirements.
- Plaintext images compress better than encrypted templates. Cryptsetup supplies
  conversion/re-encryption machinery, but we own packaging, boot changes,
  power-loss recovery, completion checks and release signing/testing. Feasibility
  is not qualification; earlier effort estimates were provisional.

## Native filesystem construction

`mkfs.btrfs --rootdir` and experimental Rust `btrfsutils` merit isolated prototypes;
Windows suitability remains unproved. WinBtrfs adds a kernel-driver dependency;
the discussed btrfs-fuse implementation is read-only. `libbtrfsutil` relies on
Linux filesystem operations. Filesystem creation alone does not solve package
hooks, Linux metadata, boot configuration or encryption.

## Next decision evidence

Benchmark file-only native WHPX and WSL2/KVM builds, separating construction from
export. Compare those results with a recovery-tested prebuilt-image prototype
before choosing an architecture. Preserve upstream owner setup where possible.

Related: [current priority](direct-install-priority-2026-09-06.md) and
[recorded build attempts](evidence/image-builder/README.md).
