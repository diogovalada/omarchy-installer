# Synthetic disk inventories

Deterministic, non-secret disk inventories for device-policy and plan tests.
Each fixture is a complete enumeration snapshot, not an instruction to open a
real device. Paths, serials, GUIDs, and WWNs are reserved synthetic values.

## Interpretation

- `candidate_id` identifies a record within one enumeration.
- `stable_id` is the identity the policy would use across enumeration passes.
  Duplicate or absent stable identities must fail closed.
- `system_disk` is independently observed; it is never inferred only from a
  mount point.
- `media` distinguishes removable, internal, and virtual storage. USB transport
  alone does not prove removable/safe.
- `size_bytes` and LBA values are decimal integers. Tests must use checked
  arithmetic and must not derive byte lengths through floating-point values.
- `partitions` describes the current topology for classification. A media write
  plan replaces the whole selected device; it never targets a listed partition.
- `expect` is the golden result for `synthetic-device-policy-v1`.

Eligibility requires a unique stable identity, whole removable media, adequate
size, a non-system device, and no identity change between selection and helper
re-enumeration. Every device not explicitly eligible has one or more refusal
codes.

## Refusal codes used here

| Code | Meaning |
| --- | --- |
| `system-disk` | Hosts the running operating system |
| `internal-media` | Not classified as removable media |
| `virtual-device` | File/VM-backed device is not a physical USB target |
| `duplicate-stable-identity` | Identity selects more than one device |
| `missing-stable-identity` | Device cannot be safely re-identified |
| `target-too-small` | Capacity is below the synthetic 8 GiB image requirement |
| `identity-changed` | Re-enumeration did not match the selection snapshot |

The fixtures deliberately include GPT, MBR, 512-byte, 512e, and 4Kn examples,
plus Windows BitLocker, APFS/T1/T2-era layouts, Apple-Silicon firmware/APFS,
LUKS/LVM, Btrfs, and ZFS.

Run `./validate.ps1` for dependency-free parse, uniqueness, arithmetic, and
expectation-reference checks.
