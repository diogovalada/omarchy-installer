# Encryption and Windows allocation implementation — September 6, 2026

This records source implementation after the user's explicit request to add all
four missing Windows features. It supersedes the fixed-size, unencrypted,
unallocated-space-only boundaries in `native-options-2026-09-06.md`. That report
remains historical evidence. No installation or recovery execution is claimed.

## Implemented behavior

- The desktop selects an allocation in GiB, bounded by freshly inspected space.
  Choices include existing free extents and eligible Windows NTFS shrink targets.
  Native code retains disk/partition identity, validates the selected byte count,
  and rechecks it after elevation. Final confirmation includes exact Windows
  before/after sizes and both new partition extents. An NTFS partition with an
  unaligned end releases less than 1 MiB extra to align the new partitions; the
  allocation stays exactly as selected and the Windows size includes that tail.
- The pinned official ISO creates a fresh LUKS2 root for each operation, using
  its existing deferred owner flow. First boot grows the LUKS mapping and Btrfs
  to the selected target size, applies upstream hardware setup, then runs owner
  provisioning. A local image is never shared between installations.
- Stock owner provisioning changes passphrase slots; it does not rotate the
  LUKS volume key. Therefore the construction QCOW2 and both exported partition
  images are additionally encrypted under an ephemeral operation key. The helper
  sends the key only through private child stdin. It is absent from requests on
  disk, manifests, journals, process arguments and environment entries. Encrypted
  output files cannot be reused after that operation key is discarded.
- The deferred bootstrap key permits the initial boot. Personal disk protection
  requires successful first-boot owner setup and key retirement. This follows
  upstream's deferred contract and is stated in the desktop.
- BitLocker inspection admits fully encrypted, unlocked supported volumes and
  rejects incomplete/unknown/locked affected states. Only required active
  protection is suspended. Data stays encrypted, and pre-existing suspension is
  preserved. A durable intent record and protected SYSTEM recovery task are
  established before suspension; normal completion restores and verifies
  protection. Failure to restore produces an error and retains recovery state.
- Native NTFS resize occurs only after the final plan is approved. The provider
  revalidates Windows size limits, disk and partition identities, and existing
  GPT entries before creating the exact new partition extents. A larger root
  receives the verified base image; the complete source span is read back.

## Validation boundary

The user explicitly deferred tests, VM construction/boot, native installer
launch, elevation, physical disk writes, BitLocker changes, firmware changes,
scheduled recovery execution and reboot. Only source review, syntax/type checks,
provider staging and compilation were performed for this update. No host disk
inspection was run to exercise the new shrink or encryption code.

Frontend type checking passed with zero errors and four pre-existing warnings
in unused prototype components. The production frontend build and native Rust
library compilation passed. Builder Python, embedded Python and shell syntax
checks passed, and the artifact decoder compiled under Windows PowerShell 5 /
.NET Framework. None of these checks executed the installation or crypto path.

`pnpm desktop:build:native` and the final locked native rebuild with
`tauri/custom-protocol` completed successfully. The resulting executable is
`apps/desktop/src-tauri/target/debug/omarchy-setup-desktop.exe`, SHA-256
`88e9e852e4fc7111fe4f394d26534334c050e172126b57e13f2481e8d76600dd`.
All **6,143** staged and packaged provider files matched their length/SHA-256
records; direct-provider and image-builder files also matched current source.
The compiled provider manifest SHA-256 is
`0e885737cdbd9cfb5c60e3f16a7e116a21470a5cce961c9e7032e7c8e2f46343`.
The [build record](encryption-allocation-build-2026-09-06.json) contains exact
source hashes and timestamps. The executable was not launched.

## Remaining qualification

Establish encrypted construction, owner provisioning, first boot on larger root
partitions, native NTFS shrinking, BitLocker success/failure/interruption recovery,
cross-language encrypted artifact handling and installation bootability in
disposable environments before physical hardware qualification. Secure Boot
must still be off. Full Windows replacement is not implemented. macOS signed
packaging and native execution still require a Mac.

BitLocker protection restoration is verified before reporting completion. Whether
each firmware/TPM configuration accepts the subsequent Windows boot without a
recovery prompt still requires boot qualification; this update makes no such
guarantee.

Implementation provenance: root integration and the delegated image builder,
Windows provider and artifact codec work used `gpt-6-astra`.

## Sources and contracts

- [Official Omarchy installation guide](https://github.com/omacom/omarchy/blob/quattro/manual/02-getting-started.md)
- [Microsoft BitLocker suspension guidance](https://learn.microsoft.com/en-us/troubleshoot/windows-client/windows-security/suspend-bitlocker-protection-non-microsoft-updates)
- [Microsoft NTFS shrink guidance](https://learn.microsoft.com/en-us/windows-server/storage/disk-management/shrink-a-basic-volume)
- [Pinned product source contract](../../providers/image-builder-x86/product-source-contract.md)
- [Artifact encryption contract](artifact-encryption-contract.md)
