# Support matrix

Status: draft; no real-device feature is qualified yet

Last reviewed: 2026-09-08

The [platform behavior guide](platform-behavior.md) records startup/defaults,
storage, encryption, first-boot and Recovery differences. The Windows menu and
Mac startup picker are separate platform behaviors; neither is hardware-qualified.

Omarchy Installer is an **unofficial community project**. This matrix describes
implementation scope and current support. Development cells can be exercised by
maintainers but have no supported release channel or hardware qualification.

## Status vocabulary

- **Planned:** architecture exists only on paper or in simulation.
- **Development:** implemented for maintainers; not offered to general users.
- **Experimental:** separately enabled, hardware-tested, with known exclusions.
- **Beta:** hardware-qualified for the exact cell, but still collecting evidence.
- **Stable:** signed, maintained, recoverable, and qualified for the exact cell.
- **Unavailable:** intentionally disabled or technically inapplicable.

Passing CI or a VM test cannot promote a destructive feature beyond
Development.

## Planned host capabilities

| Host | Download and verify | Create x86 USB | Try | Direct alongside | Replace host OS |
| --- | --- | --- | --- | --- | --- |
| Windows x64, UEFI | Development, native verification tested | Development, physical path wired; unqualified | Deferred beyond v1 | Development, local construction + GPT deployment; unqualified | Unavailable |
| Intel Mac | Source implemented, host build unverified | Development, erase/write implemented; native host and hardware unverified | Deferred beyond v1 | Planned x86 path, not implemented | Unavailable |
| Apple Silicon Mac | Source implemented, host build unverified | Development, erase/write for another x86 machine; native host and hardware unverified | Deferred beyond v1 | Development source, retained Asahi bridge; signed packaging required | Unavailable |
| Linux host | Source implemented, host build unverified | Development, erase/write implemented; native Linux file-backed tests passed; hardware unqualified | Deferred beyond v1 | Planned per-layout deployment, not implemented | Unavailable |

An x86 USB cannot install Omarchy on Apple Silicon. Apple Silicon requires an
Asahi-aware path. USB media created there is for a compatible x86 machine.

Keeping existing USB files remains Windows x64 only. Linux and macOS expose
erase-and-write after review and administrator approval. Their implementation,
prerequisites and software test evidence are recorded in the
[cross-platform USB update](evidence/cross-platform-usb-2026-09-08.md).

Windows and macOS share the storage selection, allocation and typed deletion UI.
On Mac, development source exposes the native backend's detected-Omarchy
replacement and alongside choices. Replacement covers the whole detected
installation, requires its typed identifier and exact-plan approval, and does not
replace running macOS. Arbitrary foreign partition deletion and repair are not
exposed. This remains uncompiled on macOS and unqualified on hardware; see the
[shared storage update](evidence/shared-storage-2026-09-06.md).

## Public integrations under evaluation

These are external projects, not bundled or endorsed merely by appearing here:

| Purpose | Project | Current integration position |
| --- | --- | --- |
| Omarchy distribution | <https://github.com/omacom/omarchy> | Upstream; do not fork distribution logic |
| x86 installation image | <https://github.com/omacom/omarchy-iso> | Verified USB source and source for local installed-image construction |
| Apple-Silicon installation | <https://github.com/maralcbr/omarchy-mx-mac> | Retain native backend behind shared Tauri frontend |
| USB engine, all hosts | <https://github.com/balena-io-modules/etcher-sdk> | Pinned dependency with Windows and Linux file-backed writer tests; macOS native execution and Linux/macOS hardware unqualified |
| Try on macOS | <https://github.com/omacom/try-omarchy> | Deferred beyond v1 |
| Try on Windows | <https://github.com/omacom/try-omarchy-windows> | Deferred beyond v1 |

Announced work is not treated as a dependency until there is a stable public
artifact or an explicit maintainer agreement.

## A support cell is specific

Support applies to a complete tuple, not merely “Windows” or “Mac”:

```text
application version
+ host OS version and architecture
+ exact mode
+ boot and firmware class
+ storage controller and sector geometry
+ encryption/layout class
+ provider and artifact versions
+ Mac model identifier when applicable
```

Evidence from one cell does not qualify a neighboring cell. For example,
Windows on a plain GPT/NVMe disk does not qualify BitLocker, Storage Spaces,
dynamic disks, Intel RST/VMD, legacy BIOS, or Windows ARM.

## Initial USB exclusions

USB writing must refuse:

- the running system disk and any disk that backs it;
- internal or ambiguously classified disks;
- a partition path when a whole removable disk is required;
- devices without stable enough identity to survive re-enumeration;
- media smaller than the signed artifact's declared write length;
- a target whose identity, geometry, removability, or attachment changes;
- concurrent mounts or locks that cannot be safely released;
- unsupported logical-sector geometry;
- any operation using an unverified, expired, revoked, or rolled-back catalog.

There is no expert override for identity or system-disk refusals.

## Direct-install exclusions

No direct-install cell is currently qualified. The first Windows qualification
cell is UEFI/GPT alongside deployment into pre-existing unallocated space, using
images constructed locally from the official ISO. Selectable allocation, native
NTFS shrinking, per-install LUKS2 and conditional BitLocker suspension/restoration
are now implemented in source, with separate qualification still required.
Windows replacement of an eligible unused partition is also implemented with
typed confirmation and a final native warning. It does not permit replacing the
running host OS. Encrypted/unknown/multi-device layouts and protected partitions
are excluded; deletion and locking are not hardware-qualified.
Arbitrary BitLocker changes, Storage
Spaces, dynamic disks, RST/VMD ambiguity, legacy BIOS, ARM Windows, and full
replacement remain outside the first cell. See [the plan](installation-plan.md)
for encryption, runtime, provenance, and first-boot feasibility gates.

Apple-Silicon support is model-specific and upstream-defined. A successful
installation on one M1 Pro model does not establish M1–M5 coverage.
The currently pinned published catalog admits only `apple,j314s` and expires
2026-11-30. Source integration is not a signed or hardware-qualified Mac release.

See [the integration record](evidence/native-options-2026-09-06.md) for exact
implementation boundaries and the explicitly deferred execution checks.
The [encryption/allocation update](evidence/encryption-allocation-2026-09-06.md)
supersedes its earlier fixed-size and unencrypted implementation limits.

Linux dual boot and replacement require separate qualification across LUKS,
LVM, Btrfs, ZFS, RAID, bootloader, and distribution combinations.

## Promotion evidence

Each Experimental or higher cell must link to:

- immutable build, catalog, provider, and artifact identifiers;
- at least two independent host reports where practical;
- exact host/storage/encryption facts with sensitive fields redacted;
- successful plan, write/install, verification, reboot/boot, and cleanup steps;
- interruption and recovery exercises appropriate to the feature;
- known exclusions and recovery instructions;
- reviewer approval and the release channel in which it is enabled.

Stable additionally requires signed packages/catalogs, a maintained recovery
path, an independent security review for privileged paths, and no unresolved
data-loss or high-severity security issue.

See the [tester policy](testing/tester-policy.md) and
[evidence playbook](testing/hardware-evidence-playbook.md).
