# Synthetic host fixtures

Deterministic host inventories used by capability, preflight, and UI tests. The
fixtures describe facts reported by an unprivileged probe; they do not encode
commands and must never be treated as authorization to mutate a machine.

All identifiers are invented. No serial number, recovery key, or persistent
identifier was copied from real hardware.

## Contract

- `fixture_version` changes only when the fixture envelope changes.
- `id` is stable and unique within this directory.
- `host` contains observed facts. `unknown` is preferred to guessing.
- `disk_fixture` names the companion inventory in `../synthetic-disks/`.
- `expect` is the golden policy result for the synthetic policy named by
  `policy_id`; changing it is a deliberate test-policy change.
- `direct_install_preflight` describes technical preflight only. A product
  capability may still be unavailable because no qualified provider exists.
- USB eligibility is kept separate from direct-install eligibility. Encryption
  on the internal system disk does not prevent writing a distinct removable
  drive.

The JSON Schema is documentation and a validation aid, not a privileged wire
format. Production schemas must be versioned independently and reject unknown
fields at trust boundaries.

## Coverage index

| Area | Fixtures |
| --- | --- |
| Windows x64 | BitLocker off, protected with recovery, protected without recovery, legacy BIOS |
| Intel Mac | no security chip, T1, T2 with external boot blocked |
| Apple Silicon | allowlisted model, non-allowlisted model |
| Linux | LUKS-on-LVM, native Btrfs, ZFS root |

Parse checks can be run with `./validate.ps1` from PowerShell. Full JSON Schema
validation is intentionally left to the repository test runner.
