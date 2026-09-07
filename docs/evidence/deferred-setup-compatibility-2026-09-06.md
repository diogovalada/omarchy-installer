# Deferred setup compatibility: source audit

**Follow-up:** the [completion pass](windows-direct-readiness-2026-09-06.md)
corrects the exact-template handling in both hook phases and adds five pure
regression tests. The findings below describe the earlier source revision.
Installed-system owner setup and factory reset still need boot qualification.

Date: 2026-09-06. Scope: the existing Windows x86 construction/deployment
additions versus the actual pinned Omarchy 4.0.2 ISO. This is source inspection,
not installation, boot or failure-injection execution. No runtime code changed.

## Result

The architecture can retain upstream deferred owner setup, but the current
boot-menu pre-hook has a concrete incompatibility with upstream's entry-free
Limine template. It blocks the boot rebuild used by encrypted owner setup and
factory reset. Do not describe those paths as merely awaiting qualification
with no known issues. This finding supersedes that earlier assessment.

## Confirmed integration defect: template reset versus mandatory menu group

1. Our guest recipe installs `15-omarchy-boot-identity`, which always calls
   `omarchy-boot-menu --prepare-update` before the upstream boot tools work.
2. `boot_menu.py`, `rebind_machine` (lines 117–136), requires exactly one
   `/Advanced Omarchy options` group. Absence raises an exception and the CLI
   returns 101, intentionally a fatal boot-hook result.
3. The pinned `omarchy-provision-owner`, `rekey_luks` (line 862), calls
   `reset_limine_config` (line 890) before `limine-update`. That function
   (line 957) copies the shipped Limine template onto the ESP. The template
   from `omarchy-settings-4.0.2-1-any.pkg.tar.zst` has appearance/global settings
   and **no boot-entry groups**. Our guest does not modify this template.
4. `limine-update` runs the boot-install and kernel-rebuild scripts. The pinned
   `limine-mkinitcpio-install`, lines 258–262, runs the pre-hooks before it
   generates entries and exits 2 if they fail. Therefore the missing group
   cannot be repaired by normal upstream entry generation: our pre-hook stops
   that generation first.
5. Owner setup catches the failed rebuild, restores temporary-unlock input
   files, tries another rebuild and returns failure. That rebuild encounters
   the same absent-group condition. The owner retry UI remains present, but
   retrying cannot resolve this deterministic integration conflict. Repeated
   attempts can also add further password slots before reaching the failure;
   this is not a safe loop to treat as successful provisioning.
6. The pinned `omarchy-system-factory-reset`, `rebuild_next_boot` (line 235),
   also restores the entry-free template (line 285) and calls `limine-update`.
   It therefore encounters the same hook failure while preparing reset, before
   its staged root is activated. This is a second affected workflow from the
   same root cause, not a separate upstream limitation.

The scope-correct correction belongs in our hook: accommodate the verified
upstream template-reset state, let upstream generate current kernel entries,
then apply the requested OS menu in the post-hook. Retain strict ESP identity
checks and rejection of ambiguous or unmanaged entries. Do not bypass upstream
password finalization, replace its owner program, disable its retry mechanism,
or patch factory reset to preserve stale boot entries. This audit records the
needed correction; it does not implement it.

## What the source supports

- Deferred provisioning is an explicit upstream engine contract. It strips
  personal account credentials during construction and stages owner setup.
- The owner program and service in our source lock match the bundled runtime
  byte for byte. The installed service is also checked against that hash by
  our validator. The shared setup form is the same file used by the ISO wizard.
- Required owner state, group grants and offline Node payload are stored under
  `/var/lib/omarchy/provisioning/`. Temporary LUKS boot-unlock files span root and
  boot artifacts. Whole-partition deployment retains this state; it is not a
  separate configuration partition that the proposed approach removes.
- Upstream `omarchy-apply-hardware --defer-provisioning` explicitly permits
  hardware setup before an account exists and records groups for the future
  owner. Our prerequisite uses this interface rather than creating a user.
- Our first-boot prerequisite is ordered before owner provisioning. Once its
  pending marker is removed, a later owner retry does not repeat construction
  or deliberately discard the existing upstream `setup-user` retry state.
- Upstream retains pending state and offers retries or a console on owner-setup
  failure. Its reset-finish service gates setup while a wipe is incomplete.
- The factory baseline is a Btrfs `@factory` subvolume. Our snapshot refresh uses
  upstream's credential scrub paths and removes its own first-boot pending
  marker from the baseline, so the intention is to preserve upstream reset
  rather than restart VM-to-hardware finalization during every future reset.

These facts support the integration design. They do not establish all runtime
outcomes. In particular, after correcting the known hook defect, boot/password
transition, interruption/retry, physical hardware setup, factory baseline
refresh and reset need execution evidence. The user's deferred-testing boundary
remains in force.

## Inspection provenance

Read the installed scripts directly from the ISO's runtime/settings packages;
no installer or extracted upstream script was executed. A network-disabled
container read the ISO mount read-only and wrote only extracted source/evidence
under `artifacts/setup-compatibility-inspection`. Known runtime, orchestrator,
owner-service and Limine package hashes matched `product-source-lock.json`.
Additional extracted source hashes are in the
[inspection record](../../artifacts/setup-compatibility-inspection/source-record.json).
The source-record's paths identify the exact files used above.

Public tagged sources were used for discovery, but the released package is the
authority for this audit: the extracted owner program differs from the public
tag's current file length. Do not substitute moving source for the inspected
release when implementing the correction.

- [Public owner program](https://github.com/omacom/omarchy/blob/v4.0.2/bin/omarchy-provision-owner)
- [Public factory-reset program](https://github.com/omacom/omarchy/blob/v4.0.2/bin/omarchy-system-factory-reset)
- [Local hook](../../providers/image-builder-x86/boot_menu.py)
- [Local first-boot prerequisite](../../providers/image-builder-x86/product-firstboot.sh)

Reviewed by `gpt-6-astra`; no delegated agents.
