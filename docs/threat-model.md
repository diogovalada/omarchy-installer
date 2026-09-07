# Threat model

Status: draft

Last reviewed: 2026-09-02

This document covers the proposed Omarchy Setup architecture. The project is an
**unofficial community project**, not an OmaCom or Omarchy release.

## Safety objectives

1. Never mutate a disk other than the exact user-confirmed eligible target.
2. Never execute, mount, or write an artifact before verification.
3. Prevent an unprivileged UI compromise from becoming arbitrary administrator
   or root execution.
4. Make interruption, uncertainty, and partial completion visible and
   recoverable where the operation permits.
5. Prevent update/catalog compromise from silently expanding privileged power.
6. Avoid collecting sensitive or linkable user and hardware information.
7. Preserve the existing OS and recovery environment whenever the advertised
   operation promises to do so.

## Assets

- user data on every attached disk, especially non-target disks;
- bootability and recovery of the host OS;
- artifact, catalog, provider, and application authenticity;
- privileged helper integrity and signing identities;
- operation plans, journals, and ownership receipts;
- catalog root keys and release credentials;
- tester privacy and submitted diagnostic bundles;
- the project's reputation and the upstream Omarchy identity.

## Principals and trust assumptions

| Principal | Trust level |
| --- | --- |
| User | Authorizes a displayed plan, but can select the wrong-looking device or misunderstand consequences |
| Desktop UI/webview | Untrusted for privileged decisions; may be compromised |
| Shared Rust policy core | Trusted only after review and tests; parses hostile inputs |
| Privileged helper | High trust, small and allowlisted; distrusts UI and paths |
| Provider | Untrusted until exact identity/version is catalog-authorized; capability-limited |
| Signed catalog | Trusted only through embedded root, expiry, rollback and delegation checks |
| Download/cache/network | Hostile; may truncate, replay, substitute or corrupt data |
| Operating system APIs | Generally trusted, but facts may race due to hotplug/mount/state changes |
| CI/release infrastructure | Sensitive supply-chain boundary; compromise is in scope |
| External tester | Honest but may misunderstand steps; submitted content is untrusted and potentially sensitive |

Physical evil-maid attacks, malicious firmware, a fully compromised kernel, and
hardware that lies consistently about all identity and writes are outside the
first practical boundary. The application must still fail safely on inconsistent
or missing device facts.

## Trust boundaries

```text
Internet and cache
       | signed metadata + verified bytes
       v
Unprivileged application <----> external provider
       | authenticated, typed, immutable plan
       v
Privileged helper ----> OS disk APIs ----> physical target

CI/signing ----> application/helper/catalog releases
Tester machine ----> redacted local bundle ----> issue tracker
```

## Threats and required controls

### Wrong-target disk mutation

Threats include UI confusion, `/dev` or PhysicalDrive number reuse, duplicate or
missing serials, USB bridge quirks, system disks exposed as removable, target
replacement after confirmation, partition/whole-disk confusion, and arithmetic
overflow.

Controls:

- allow whole removable media only in the initial writer;
- derive system-disk ancestry and reject every backing member;
- record multiple identity and geometry facts, not a device path alone;
- show vendor/model/capacity/connection and a destructive summary;
- re-enumerate and reclassify inside the helper immediately before mutation;
- bind artifact, exact byte range, device identity, policy and nonce into the
  immutable plan digest;
- checked arithmetic and exact upper write bound;
- refuse ambiguity or change with no force override;
- sentinel-device tests and physical wrong-target exercises.

### Malicious or corrupted artifact

Threats include mirror substitution, truncated downloads, replayed old images,
cache tampering, malicious catalog entries, decompression bombs, and confusion
between architectures.

Controls:

- embedded offline catalog root, expiry, monotonic version and rollback state;
- delegated signing keys and revocation/rotation procedure;
- verify exact length, cryptographic hash and signature before use;
- helper independently re-verifies the artifact and signed authorization;
- preserve upstream signatures as a separate trust layer;
- bounded streaming/decompression with declared output length;
- match host, architecture and provider constraints explicitly;
- never mount or execute an unverified artifact.

### Privilege escalation through the UI or IPC

Threats include compromised web content, command injection, arbitrary paths,
symlink/race attacks, replay, confused-deputy requests, helper downgrade, and a
malicious local process connecting to IPC.

Controls:

- ordinary-user UI and just-in-time native elevation;
- no shell, arbitrary command, arbitrary file write, network, or update verb;
- versioned typed messages with strict bounds and deny-unknown semantics where
  privilege could expand;
- reciprocal code identity or package/signature checks;
- authenticated local channel, per-operation random nonce and replay rejection;
- helper-generated handles or safe open semantics instead of trusting paths;
- exact client/helper compatibility range;
- one-shot or short-lived helper where the platform permits;
- audit helper code and protocol separately from UI changes.

### Time-of-check/time-of-use and hotplug

Threats include target removal/replacement, volume remount, catalog expiry,
artifact replacement, and host power/state changes between planning and write.

Controls:

- repeat all safety-critical checks after elevation;
- hold exclusive device access through mutation and verification;
- use an already verified immutable artifact identity;
- reject device topology, geometry, or identity changes;
- journal checkpoints and classify disconnect as failure, never success;
- require a fresh plan after any material precondition change.

### Partial writes and false success

Threats include short writes, delayed caches, unplug, helper/UI crash, power
loss, read errors, cancellation at unsafe points, and incorrect progress.

Controls:

- loop on short writes and treat zero progress as an error;
- durable flush using platform-appropriate APIs;
- read back and hash the complete signed image length;
- append-only crash-safe state transitions and plan digest;
- monotonic progress derived from acknowledged bytes;
- explicit cancellation phases and non-cancellable critical sections;
- success receipt only after verification and final state persistence;
- fault injection at every I/O and journal boundary.

### Direct-install boot or data loss

Threats include incorrect partition changes, encryption/bootloader interactions,
firmware differences, staging data erased by the operation, and incomplete
rollback claims.

Controls:

- direct-install disabled until a provider-specific threat model exists;
- exact support cells and fail-closed preflight;
- prefer upstream-qualified engines and signed handoff;
- begin with reversible guidance and pre-existing unallocated space;
- perform same-disk mutation offline where required;
- model the staging source so it survives every intended write;
- test interruption and recovery on sacrificial physical machines;
- never promise rollback where only repair/recovery is possible.

### Supply-chain compromise

Threats include dependency or CI action takeover, forged releases, leaked
signing keys, catalog-key compromise, malicious provider update, and rollback.

Controls:

- pin dependencies and CI actions by immutable identity;
- least-privilege CI and protected release environments;
- signed tags/packages, SBOM, provenance and reproducible checks where possible;
- offline root and separated delegated catalog/release keys;
- two-person approval for release and safety-critical changes;
- expiry, revocation and emergency-disable procedures;
- independently verify public release assets;
- never update during an active destructive operation.

### Privacy and support-data leakage

Threats include recovery keys, usernames, paths, Wi-Fi data, disk contents,
serial numbers, persistent device IDs, and malicious files entering a bundle.

Controls:

- no telemetry by default and no hidden analytics identifier;
- local-only bundle generation with preview before manual upload;
- field allowlist, redaction and adversarial redaction tests;
- truncate or hash hardware identifiers with per-bundle salt when correlation is
  necessary;
- exclude disk contents, credentials and recovery keys categorically;
- document retention and deletion expectations for submitted reports.

### Branding and social engineering

Threats include unofficial binaries presented as official, lookalike downloads,
and instructions that solicit secrets or unsafe testing.

Controls:

- prominent “community preview — not an official Omarchy release” language;
- do not use protected logos or imply endorsement without permission;
- publish hashes/signatures and canonical download locations;
- tester policy forbids credentials, recovery keys and sole/managed machines;
- treat issue comments and external instructions as untrusted.

## Security invariants

The following are release-blocking properties:

- no real-disk backend in simulator-only packages;
- no privileged operation without a confirmed immutable plan;
- no artifact use before both project and applicable upstream verification;
- no generic privileged execution or write API;
- no system/internal/ambiguous target in initial USB support;
- no success before durable flush, full read-back verification and journal commit;
- no capability expansion from unknown protocol fields;
- no automatic diagnostic upload or default telemetry;
- no stable claim based solely on simulation or CI.

## Security-test program

- property tests over arbitrary device topologies and checked arithmetic;
- model-based operation-state and authorization testing;
- fuzz catalog, partition, IPC, journal and diagnostics parsers;
- deterministic I/O, hotplug, crash and power-loss fault injection;
- protocol downgrade, replay and malicious-client tests;
- sentinel non-target disks in every destructive simulation;
- package inspection proving backend feature separation;
- physical USB and direct-install exercises before promotion;
- independent review of helper, IPC, trust/update, and target policy.

## Incident triggers

A credible report of wrong-disk selection, out-of-plan mutation, lost data,
unrecoverable boot, signature bypass, arbitrary privileged behavior, catalog
rollback, or sensitive diagnostic leakage immediately freezes the affected
feature. Maintainers should disable the exact artifact/provider/support cell by
signed policy where possible, preserve evidence, follow `SECURITY.md`, reproduce
the failure safely, and require fresh qualification after a fix.

## Open questions

- Which organization will own branding, release signing and catalog root keys?
- Which stable provider interfaces will upstream Apple, Windows and VM projects
  accept?
- Which device identity facts are sufficiently reliable on each supported OS?
- What independent security-review resources are available before physical beta?
- What retention policy will apply to tester evidence and security reports?
