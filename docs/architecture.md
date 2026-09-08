# Architecture overview

Status: native download slice implemented; installation and physical USB integration planned

Last reviewed: 2026-09-05

Omarchy Installer is an **unofficial community project**. It is not an OmaCom or
Omarchy release and must not imply endorsement without written permission.

## Purpose and boundaries

The project presents one setup experience while delegating platform-specific
work to replaceable providers and narrowly privileged helpers. It is an
orchestration and safety layer, not a universal disk installer.

v1 covers download-only, USB creation, and qualified direct installation. Try is
deferred beyond v1. The desktop now executes native downloads; install and
physical USB controls remain unavailable. The [current strategy](installation-plan.md)
defines local x86 image construction from the official ISO and reuse of the
native Apple backend. One Etcher SDK backend is planned for USB on all hosts.

## Current download implementation

The shared Svelte download panel calls four Tauri commands: status, resolve,
start and cancel. Native state retains an immutable `Release`; no source URL,
key, release metadata or destination path is accepted from the webview.
`omarchy-release-client` resolves the official ISO and uses the existing
`omarchy-downloader` cache for resumable transfer and size/SHA-256 validation.
The upstream detached OpenPGP signature must verify against the embedded
upstream primary key before promotion. The desktop then exports through a
temporary file, rehashes the copied data and publishes without overwriting.

This development slice uses upstream signing directly, not the separate
project-catalog test key. Catalog freshness/rollback policy and privileged
artifact ownership are still future release requirements. See the
[release client contract](../crates/release-client/README.md) and
[current execution evidence](evidence/implementation-2026-09-05.md).

## Components

```text
Unprivileged desktop application
  |-- capability and policy core
  |-- signed catalog and verified downloader
  |-- operation journal and diagnostics
  |-- provider client
  `-- typed privileged-plan client
         |
         +-- Windows: one-shot elevated helper
         +-- macOS: signed XPC privileged helper
         `-- Linux: packaged Polkit-authorized helper

Providers
  |-- media: adapter + Etcher SDK for discovery/write/verify/eject
  |-- direct x86: local Linux builder + native deployment/finalization
  |-- direct Apple: retained native coordinator/helper/Asahi engine
  `-- try: optional future integration, outside v1
```

The desktop process never needs administrator or root privileges. It may be
compromised and is therefore not trusted by a privileged helper.

## Shared domain model

The shared Rust core owns:

- immutable operation plans and canonical plan digests;
- host facts and support-cell evaluation;
- capability, preflight-finding, and operation-state types;
- catalog validation, artifact verification, and rollback protection;
- operation journaling and redacted diagnostics;
- target-classification policy and bounded write rules;
- versioned provider and helper contracts.

Safety rules stay outside the webview and frontend TypeScript. The UI renders
facts and plans produced by the core; it does not decide whether a device is
safe.

## Provider lifecycle

Every Direct provider, and any future Try provider, implements a versioned subset of:

```text
probe_host -> capabilities -> preflight -> plan -> authorize -> execute
                                                |             |
                                                `-- cancel    +-- resume
                                                              `-- recover
```

Providers must declare their identity, protocol version, artifacts, support
cells, and exact privileges. Unknown fields do not grant capabilities. A
provider is unavailable unless the signed catalog permits its exact version
and host cell.

Providers come in three integration forms:

1. in-process logic that cannot mutate privileged state;
2. a verified, signed handoff to an independently maintained application;
3. an agreed typed protocol to an external engine.

Use upstream dependencies under their applicable licenses and preserve notices.
Coordinate changes to an upstream-owned protocol with its maintainers; ordinary
reuse does not depend on obtaining a reply. A handoff remains visibly attributed
to its actual maintainer. No upstream messages are authorized by this plan update.

## Engine reuse and local construction

The USB adapter wraps a pinned Etcher SDK and its bundled Node/native runtime;
the full Electron GUI is not required. Native elevation, authenticated IPC,
target policy and mandatory verification remain our integration responsibilities.
Do not treat the SDK's sample CLI or Etcher's internal helper protocol as a
drop-in privileged API. Do not maintain a second production writer by default.

For x86 direct installation, the official ISO supplies packages and reusable
installation logic to an isolated local Linux build environment. This environment
constructs operation-specific partition images on virtual disks; it cannot access
the physical target. A native helper deploys the verified output, and the installed
system completes machine-specific setup. Unique encryption, image provenance,
hardware finalization, boot configuration, and filesystem growth must be proved.
This avoids planning an ISO boot partition or publishing a derivative ISO, while
still requiring temporary host files and a maintained construction recipe.

The Mac frontend bridge retains LiveInstallerEnvironment, EngineInspectionRunner,
InstallerSession, TrustCore, helper identity checks, and Recovery completion.
The Swift protocol is an integration boundary to adapt, not a public CLI.

## Privileged operation flow

USB creation uses this sequence:

1. Resolve metadata through a valid signed catalog.
2. Download into an unprivileged cache and verify signature, length, and hash.
3. Enumerate eligible whole removable disks and display immutable target facts.
4. Build and digest a bounded plan; obtain explicit user confirmation.
5. Start the platform helper through the native elevation mechanism.
6. Authenticate both ends and send the typed plan, nonce, and verified artifact.
7. The helper re-verifies the artifact, plan, caller, target identity, and policy.
8. Lock and unmount/dismount the target, then stream a length-bounded write.
9. Flush durable caches and read back the full image length for verification.
10. Eject where supported and issue a journalled operation receipt.

Identity is re-evaluated immediately before mutation. Device paths alone are
never durable identities. Any mismatch, ambiguity, hotplug race, or unsupported
state fails closed.

## Trust and update model

The release catalog has an embedded offline root, expiry, version and rollback
protection, delegated artifact keys, exact sizes and hashes, supported-host
constraints, and provider/helper protocol ranges. Upstream signatures are
verified independently; the project catalog does not replace upstream trust.

Executable updates cannot add privileged behavior during an operation. Helpers
accept only compatible, signed clients and bounded plans. An emergency signed
policy may disable a catalog entry or support cell, but cannot deliver executable
code or broaden a helper's allowlist.

## Operation ownership and recovery

An append-only, crash-safe journal stores the operation ID, state transitions,
plan digest, non-secret device facts, artifact identity, provider version, and
receipts. It stores no passwords, recovery keys, disk content, or persistent
cross-install tracking identifier.

Resume and removal operate only on resources for which ownership can be proven.
An incomplete operation is explained; it is never converted to success merely
because the process restarted.

## Platform adapters

| Platform | Discovery and control | Elevation boundary |
| --- | --- | --- |
| Windows | SetupAPI/storage APIs, volume locking, `\\.\PhysicalDriveN` only after identity validation | One-shot Authenticode-signed helper launched with UAC; authenticated named pipe |
| macOS | Disk Arbitration and whole `disk`/`rdisk` devices | Developer-ID-signed `SMAppService`/XPC helper with reciprocal code-identity checks |
| Linux | udev/sysfs and UDisks2 where suitable | Root-owned packaged service authorized through Polkit; never an AppImage-bundled helper |

Direct-install providers define additional platform-specific boundaries and
must receive their own threat-model and support-cell review before enablement.

## Test seams

All block I/O is behind an interface implemented first by file-backed devices.
The simulator supports stable identities, synthetic GPT/MBR layouts, short
writes, read errors, hotplug, stale identities, crashes, and power-loss
checkpoints. Two sentinel devices accompany every destructive simulation.

Release packages must make the fake and real backends mutually exclusive at
build time. CI proves that developer previews cannot accidentally select a real
disk backend.

## References

- [Threat model](threat-model.md)
- [Support matrix](support-matrix.md)
- [Tester policy](testing/tester-policy.md)
- [Provider architecture ADR](adr/0004-provider-architecture.md)
- [Privilege separation ADR](adr/0003-privilege-separation.md)
