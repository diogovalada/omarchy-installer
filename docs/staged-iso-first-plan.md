# Staged official installer: retained alternative and investigation

Decision date: 2026-09-06.

**Superseded sequencing:** later in the September 6 discussion, the user favored
direct local construction/deployment again. Read the
[current decision and BitLocker findings](direct-install-priority-2026-09-06.md).
The plan below preserves the earlier proposal and unresolved integration work;
its references to a first milestone are historical, not the current priority.

The user requested retaining the prepared-system-image option and proposed
prioritizing a no-USB path that stages the official ISO on an internal partition,
then boots the real installer. This is now the first Windows x64 direct-install
path to establish. It is planned, not implemented or boot-qualified. Existing
local-construction code remains a development alternative; no runtime behavior
was changed by this planning update.

## Intended user journey

1. In Windows, download and verify the official ISO.
2. Review the exact space needed for temporary installer storage and the final
   Omarchy installation. Prepare that space using the native storage helper.
3. Populate and verify a dedicated staging partition. Prepare a recoverable
   boot entry and show the handoff instructions.
4. The user chooses when to restart into the Omarchy installer.
5. Run the upstream installation engine on the actual hardware, limiting its
   destination to the agreed free space and preserving Windows and staging.
6. Complete installation and boot Omarchy independently. Establish the selected
   permanent Omarchy/Windows menu behavior; do not assume the existing
   image-builder menu hooks automatically apply to this different path.
7. Offer removal of only the owned staging partition and temporary boot entry
   after success is confirmed. Explain any unallocated space left behind.

This moves package installation, unique LUKS encryption, filesystem creation,
hardware detection and system configuration back into the upstream Linux
installer. No Docker-based OS construction is part of the intended Windows
user journey. It still needs temporary disk capacity and an installer reboot.
Its exact RAM/space requirements must be measured; the current builder's
85 GiB/10 GiB thresholds do not transfer to this path.

The intended product choice is **Install without USB → Restart into installer**.
Other no-USB methods may be offered after their own implementation and
qualification. Do not expose several apparently working options merely because
their architectures have been recorded.

## First feasibility gates

Source inspection of the actual pinned 4.0.2 ISO found two material constraints:

- `root/configurator`, `disk_form`, resolves `/run/archiso/bootmnt` to its parent
  disk and excludes that whole disk from the target picker. A staging partition
  on the same physical disk as Windows and the destination conflicts with that
  normal picker policy. A boot arrangement that happens to hide the source disk
  identity is not an adequate safety solution. Same-disk source/target support
  needs an explicit bounded handoff that protects staging and every retained
  partition while restricting writes to the agreed destination.
- `run_partition_decide` scans the selected disk for BitLocker signatures and
  refuses the alongside path when found. Suspending Windows protectors does not
  remove those signatures. The existing native deployment provider's bounded
  suspension/restoration design cannot simply be claimed for this new path.
  First isolate an unencrypted Windows test cell, then decide the supported
  encrypted-Windows policy and any narrowly scoped installer integration needed.
  Do not decrypt Windows or remove this check as an implicit fallback.

Further work:

- Prove the exact UEFI boot route and ArchISO source discovery from the staging
  filesystem. Decide between verified extraction and an ISO-file boot path;
  copying an ISO file alone does not establish bootability. Do not assume a FAT
  volume can hold the complete ISO as one file, or that raw-writing a hybrid ISO
  inside a partition is equivalent to writing a whole USB disk.
- Account for staging and destination as separate regions. The source cannot
  be deleted or overwritten while installation still depends on it. Cleanup
  does not automatically grow a neighboring filesystem.
- Bind cross-reboot state to the verified artifact and exact disk/partition
  identities. Revalidate before Linux mutation; do not trust drive letters or
  a stale Windows plan as sufficient authorization.
- Define where upstream questions are shown. Normal interactive installation
  asks personal questions before installation. Deferred owner setup can retain
  them at first boot, but this ISO's interactive deferred mode selects full-disk
  installation and must not be used unchanged for alongside installation.
- Preserve a working Windows boot route if preparation or installation fails.
  Separate 'installer staged', 'installer completed' and 'Omarchy booted' states.

The next execution proof, once testing is resumed, should model staging and final
destination on the **same disposable disk**, not only use a separate virtual CD.
First prove staging boot and protected target selection; then installation,
independent boot, Windows return and staging cleanup. Hardware tests follow.
The prior instruction deferring execution tests remains in force: this planning
change authorizes no disk/firmware mutation, elevation, installation or reboot.

## Retained alternatives and rationale

| Method | Position | Reason / unresolved cost |
| --- | --- | --- |
| Staged official ISO, restart into installer | First Windows no-USB milestone | Reuses real installation on actual hardware; needs safe same-disk boot/source handling and cleanup |
| Build a system locally from the ISO, then deploy | Existing development alternative; lower priority | Avoids installer staging partition but currently needs Docker, substantial temporary storage and memory; construction/boot remains unproven |
| Download a prepared installed-system image | Retained research option for later | Removes per-user package construction; requires maintained image publication, unique encryption and destination finalization |

A prepared image can preserve upstream owner setup: keyboard, account/password,
optional Git identity, hostname and timezone can remain first-boot questions.
Our app would replace storage selection and deployment; the image still needs
portable boot, unique identity/encryption, hardware finalization, capacity growth
and a valid factory snapshot. These are not the same as a customized installer
ISO, which continues to execute installation after booting.

Possible deployment tools remain research candidates: native image conversion
(`qemu-img` currently documents standalone LUKS1, not our LUKS2 target), or a
bundled minimal Linux appliance that only deploys a prepared image. Neither is
an approved implementation change. Do not distribute a cloned encrypted image
and assume changing its password creates a unique volume key.

The retained Mac backend uses a prepared image but its pinned direct-install
package is unencrypted; see the [Mac encryption inspection](evidence/apple-direct-encryption-2026-09-06.md).
Apple Silicon continues using its native Asahi integration. This staging plan
does not apply the x86 ISO to Apple Silicon; Intel Mac direct staging would need
separate platform work.

Full Windows removal remains post-v1. A management dashboard, automatic resume,
general repair and polished diagnostics UI may also follow later; basic failure
guidance, protection restoration where applicable, retained recovery records and
safe owned-file/staging cleanup belong to the first working path.

## Sources

- Actual ISO `root/configurator` SHA-256:
  `0dcf67931d71c2c45f1f56fa9bf7d1b6a583600fad26441c2e5042a0631c461e`.
  Read without running it; source copy under
  `artifacts/iso-installer-choices/iso-configurator`.
- Actual shared setup form SHA-256:
  `0fbb8c3c8b151e5ff884c16d381e12fda53cc16adfd08a684d41e7327250665d`.
- [Upstream configurator](https://github.com/omacom/omarchy-iso/blob/quattro/configs/airootfs/root/configurator)
- [Official dual-boot flow](https://github.com/omacom/omarchy/blob/quattro/manual/50-dual-boot-install.md)
- [Deferred owner setup](https://github.com/omacom/omarchy/blob/quattro/manual/02-getting-started.md#installing-for-another-owner)

This decision supersedes the earlier local-construction-first sequencing in
older evidence and architecture sections. It preserves their implementation
history and validation limits.
