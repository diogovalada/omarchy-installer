# Direct installation priority and staging findings

Decision: 2026-09-06, after reviewing the staged-ISO restrictions.

The user's current preference is to return to local system construction followed
by native Windows deployment as the leading no-USB approach. It appears easier
to coordinate from Windows than an internal installer partition and a second
installation environment. This is a working architecture preference, not a
completed feasibility proof. The earlier staging-first sequencing is superseded;
retain its findings and the prepared-image alternative for future decisions.

This decision originally recorded discussion only. The subsequent
[completion pass](evidence/windows-direct-readiness-2026-09-06.md) implements
the remaining Windows work and resumes automated validation. Host disk,
BitLocker, firmware, scheduled-task and reboot operations remain unperformed.

## Preferred journey

1. Download and verify the official ISO in Windows.
2. Use its installation engine in a disposable Linux environment with only
   file-backed virtual disks to construct a fresh installed system per operation.
3. Use upstream's preparing-for-another-owner/deferred provisioning mode. Keep
   personal configuration at first boot, including keyboard, account/password,
   optional Git identity, hostname and timezone. Storage allocation and boot-menu
   preferences still belong to the host installation plan.
4. Through the native Windows helper, inspect and approve the exact destination,
   coordinate required BitLocker suspension, prepare space, deploy the verified
   system and configure its permanent boot entries.
5. Complete hardware and capacity finalization before upstream owner setup on
   the first Omarchy boot. Retain a working Windows return path and verify the
   BitLocker restoration lifecycle, including subsequent Windows boots.
6. Clean operation-owned temporary files after their dependencies are finished.

The existing builder already passes the deferred-provisioning marker to the
upstream installer and arranges hardware finalization before owner provisioning.
This is source implementation, not proof of a successful first boot. The ISO
remains the verified source; preparing an installed system does not require
publishing or modifying the official ISO. Local Docker/QEMU construction still
has significant runtime, disk-space and memory costs. There is no temporary
physical ISO boot partition, but installed-system boot/root partitions and
temporary host files remain necessary.

## Preserve the upstream setup workflow

**Historical source-audit finding, now corrected in source:** the menu pre-hook rejected the
entry-free template upstream deliberately restores during encrypted owner setup
and factory reset. This is a known integration defect, not just an untested
possibility. Read the [compatibility audit](evidence/deferred-setup-compatibility-2026-09-06.md).
The completion pass corrects both hook phases while preserving upstream's
workflow, with five pure regression tests. Actual first boot remains unqualified.

The user's explicit follow-up requires preserving Omarchy's own setup workflow
and any state or partitions it needs. Do not replace its owner setup with a
second onboarding implementation, discard provisioning files during deployment,
or remove a partition merely because it appears to contain configuration.

For the inspected x86 4.0.2 contract, deferred owner state resides inside the
installed root at `/var/lib/omarchy/provisioning/`; the pending marker activates
`omarchy-provision-owner.service`, and the owner program uses the shared upstream
setup form. Temporary encryption-unlock material also participates in boot.
The factory baseline is a Btrfs `@factory` subvolume. These are not a separate
temporary ISO staging partition. Deploy the complete required filesystem/boot
state, and let upstream owner provisioning manage its completion and key cleanup.
Do not generalize this layout to Mac or a later release without inspection.

Existing integration adds a prerequisite for physical-hardware and capacity
finalization, plus boot-menu maintenance and factory-baseline refresh. These
adaptations must preserve upstream setup, retry and reset contracts; they are
not evidence that we can promise no interference before testing. Keep changes
limited to deployment necessities and retain upstream's owner binary and form.
Qualification must cover first-boot completion, interrupted setup/retry and the
resulting factory-reset baseline, not merely reaching a desktop.

## Findings from the staging investigation

- The pinned 4.0.2 configurator excludes the entire parent disk of its boot
  source before the user selects an installation mode. Its stated purpose is
  preventing installation media from becoming a whole-disk wipe target. This
  is not an inherent prohibition on same-disk installation. A bounded path
  could protect the staging partition and all retained partitions, but must
  also preserve access to the live environment and package source throughout
  partition changes; merely hiding the source from detection is insufficient.
- The stock free-space path rejects any detected BitLocker signature on the
  selected disk, including suspended BitLocker. Suspension leaves encryption
  and that signature intact. Using the stock path unchanged therefore requires
  more than our Windows helper suspending protection.
- The historical upstream rationale explicitly avoids unexpected Windows
  recovery-key prompts for users who lack their key, and disallows an override
  in that design's v1. It is a conservative product policy, not evidence that
  writing separate unused sectors is inherently incompatible with BitLocker.
- Staging adds source-partition capacity, boot/source discovery, cross-reboot
  state, protected destination handoff, and later cleanup. Removing staging
  does not automatically extend Omarchy into the reclaimed space.
- The ordinary wizard asks personal questions before installation. Its
  interactive deferred mode uses full-disk installation, so that mode must not
  be forwarded unchanged to an alongside target. The local builder instead
  invokes the reusable engine against a disposable virtual disk.

## What BitLocker does and does not establish

**Subsequent specification research:** an extra Windows boot is not inherently
required for an already-compatible UEFI/default-profile configuration using our
restart-to-original-Windows-entry design. Actual protector/boot-route inspection
and Secure Boot transition handling were concrete missing preflight work; the
completion pass now implements them, pending machine qualification. Read
[the research and its exact scope](evidence/bitlocker-direct-boot-research-2026-09-06.md).

The Windows boot environment can change in either architecture. Direct
deployment does not eliminate recovery risk simply because it runs in Windows.
The relevant advantage is control over preparation, mutations, error handling
and restoration, without handing the physical disk to a wizard that refuses
encrypted-Windows layouts.

Microsoft documents boot-manager and boot-configuration changes as potential
recovery triggers. Its default native-UEFI profiles omit the GPT partition-table
measurement (PCR 5), so adding a partition alone is not a universal trigger.
The actual protector profile, firmware and Secure Boot changes matter. The
current implementation requires Secure Boot off; any transition from enabled
to disabled must be accounted for before promising a smooth Windows return.

Use **suspend protection**, not full drive decryption, for the proposed temporary
maintenance window. Data remains encrypted but a clear key temporarily permits
access. Restoration must be bounded and recoverable; leaving Windows suspended
while a user remains in Linux is not an acceptable silent success state.

The native provider has conditional suspension/restoration and recovery records
in source. That does not yet establish that its restoration timing accommodates
every later boot change. Qualification must determine when to resume/reseal,
exercise failure and cancellation, and verify Windows boots after protection is
active again. Checking only that protection reports active before reboot is
insufficient evidence. Accessible recovery information remains the fallback;
do not promise users they can never need their recovery key.

## Retained alternatives

- Internal staged ISO: secondary option, not implemented. Preserve the
  [earlier plan](staged-iso-first-plan.md) and its source findings.
- Published prepared-system image: later research option to avoid per-user
  construction. Publication, maintenance, per-install encryption and hardware
  finalization require their own design; copying a shared encrypted image and
  changing its password does not create a unique volume key.
- Apple Silicon: continue the native Asahi backend. Its pinned prepared payload
  is unencrypted, as recorded in the [Mac inspection](evidence/apple-direct-encryption-2026-09-06.md).
- Complete Windows removal remains post-v1.

## Evidence

- Pinned ISO source inspection: [configurator](../artifacts/iso-installer-choices/iso-configurator),
  SHA-256 `0dcf67931d71c2c45f1f56fa9bf7d1b6a583600fad26441c2e5042a0631c461e`.
- [Historical upstream policy and restore design](https://github.com/omacom/omarchy-iso/blob/8b3dcc68b23dc00cf2d3e75cfe9e7afeff5501e4/plans/protected-partition-install.md).
  This design is rationale evidence, not a claim that all its proposed features shipped.
- [Microsoft native-UEFI validation profiles](https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/configure#configure-tpm-platform-validation-profile-for-native-uefi-firmware-configurations).
- [Microsoft suspension and decryption explanation](https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/faq#what-is-the-difference-between-suspending-and-decrypting-bitlocker).
- [Microsoft recovery scenarios](https://learn.microsoft.com/en-us/windows/security/information-protection/bitlocker/bitlocker-recovery-guide-plan).
- Existing source: [builder](../providers/image-builder-x86/product-guest.sh)
  and [encryption implementation evidence](evidence/encryption-allocation-2026-09-06.md).
