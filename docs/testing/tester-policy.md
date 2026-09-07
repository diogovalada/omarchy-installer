# Community tester policy

Status: draft

Last reviewed: 2026-09-02

Omarchy Setup is an **unofficial community project**. Tester participation is
voluntary and does not make a feature safe for general use.

## Non-negotiable eligibility rules

Do not perform a destructive test on:

- your only computer or only bootable/recovery device;
- a computer or drive containing data you cannot afford to lose;
- employer-, school-, customer-, or otherwise managed equipment;
- a drive whose owner has not explicitly authorized erasure;
- a machine without a tested backup and recovery route;
- a device outside the exact candidate's published support cell.

USB-writing tests require a disposable USB drive containing no valuable data.
Direct-install tests require a sacrificial machine or dedicated spare disk.

The application and maintainers will never ask for passwords, BitLocker/FileVault
or other recovery keys, disk contents, Wi-Fi credentials, full serial numbers,
private signing keys, or remote-control access. Treat any such request as a
security incident and report it using `SECURITY.md`.

## Test rings

| Ring | Activity | Minimum prerequisites |
| --- | --- | --- |
| 0 | UI, simulator, catalog and dry-run | No physical disk mutation |
| 1 | Download and cryptographic verification | Adequate temporary space |
| 2 | Write/verify/eject a disposable USB | Empty disposable media; backup/recovery prepared |
| 3 | Boot the USB on a spare computer | Ring 2 success; spare compatible host |
| 4 | Install to a spare disk/test machine | Dedicated hardware; recovery media tested |
| 5 | Direct/dual-boot provider | Exact qualified cell; provider-specific playbook and approval |

Testers must not skip rings specified by the candidate. A result from a higher
ring does not excuse missing evidence from lower rings.

## Candidate identity

Use only a candidate announced in a canonical project location. Record:

- application version and immutable build ID;
- package hash/signature status;
- signed catalog version;
- provider and artifact versions;
- playbook revision and requested ring;
- exact support-cell declaration and known exclusions.

Never test a binary received only through a direct message or an unverified file
host. Do not disable OS security controls merely to make an unexplained binary
run.

## Required preparation

Before any Ring 2+ activity:

1. Read the entire candidate-specific playbook and exclusions.
2. Update or complete a backup, then verify that recovery material works.
3. Disconnect unrelated removable and external storage.
4. Record non-sensitive host facts requested by the evidence template.
5. Confirm the target contains no valuable data and identify it by at least
   model, approximate capacity and connection—not drive letter alone.
6. Ensure reliable power; do not test through a questionable hub.
7. Know how to stop: before confirmation, cancel normally; after writing starts,
   follow the playbook instead of killing power unless explicitly testing that
   failure.

## Stop and escalate

Stop immediately and preserve the local journal/support bundle if:

- the displayed target differs from the disposable drive;
- any internal/system/ambiguous disk is offered as eligible;
- the plan changes after confirmation;
- a non-target disk, partition, boot entry or mount changes;
- the tool reports success without full read-back verification;
- the host becomes unbootable or data appears missing;
- elevation exposes a shell, arbitrary path or unexpected publisher;
- diagnostics contain a prohibited secret or identifier;
- instructions conflict with this policy.

Do not retry a suspected wrong-target or out-of-plan mutation. Use the private
security route for signature/privilege/privacy flaws and the candidate's issue
form for ordinary failures. A credible data-loss, wrong-target or unrecoverable
boot report freezes the affected feature.

## Evidence and privacy

Submit only the locally generated, previewed and manually approved fields. Before
upload, inspect attachments for names, paths, serials, account IDs, recovery keys
and screenshots showing personal data. You may remove optional fields.

Public reports should use broad capacity and redacted identifiers. Maintainers
must not attempt to correlate per-bundle identifiers across reports. Evidence is
used only to reproduce failures and qualify the declared support cell; retention
and deletion expectations must be stated when public testing opens.

## Maintainer responsibilities

Maintainers must:

- distinguish simulation, Development, Experimental, Beta and Stable clearly;
- publish immutable candidate and support-cell information;
- provide expected behavior, cancellation points and recovery instructions;
- never pressure a tester to expand risk or waive an invariant;
- acknowledge safety reports promptly and disable affected cells where possible;
- translate useful failures into sanitized fixtures and regression tests;
- publish qualification evidence without overstating nearby hardware coverage.

See the [hardware evidence playbook](hardware-evidence-playbook.md) for the
standard Ring 2–4 procedure.
