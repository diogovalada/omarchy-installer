# Hardware evidence playbook

Status: template; not authorization to test a real device

Last reviewed: 2026-09-02

Use this only with an immutable candidate that explicitly requests your test
ring. Read the [tester policy](tester-policy.md) first. Omarchy Setup is an
**unofficial community project**.

## 1. Candidate record

Copy the values from the canonical candidate announcement; do not infer them.

```text
Application version/build ID:
Download URL:
Package hash and signature result:
Catalog version:
Provider version (if any):
Artifact version/hash:
Playbook revision:
Authorized ring:
Declared support cell:
Known exclusions:
```

If any value is absent or signature verification is unexpected, stop.

## 2. Non-sensitive host record

Record only what the candidate asks for. Prefer locally generated diagnostics.

```text
Host OS/version/architecture:
Firmware/boot class:
Computer model class (exact Mac model identifier if required):
Storage-controller class:
Logical-sector size:
Encryption/layout class:
USB controller/connection class:
Disposable USB vendor/model and approximate capacity:
```

Do not include usernames, hostnames, full serials, recovery keys, disk contents,
Wi-Fi details or persistent hardware identifiers.

## 3. Preparation checklist

- [ ] This is not my only or a managed computer.
- [ ] The target is disposable and contains no valuable data.
- [ ] Backup and host recovery have been tested.
- [ ] Unrelated external/removable storage is disconnected.
- [ ] The candidate matches the declared support cell and ring.
- [ ] Package identity, publisher/signature and hash are correct.
- [ ] Reliable power is connected.
- [ ] I know the incident stop conditions.

## 4. Ring 1: download and verification

1. Resolve the image through the candidate's signed catalog.
2. Interrupt and resume one download if requested.
3. Confirm the displayed size, architecture, source and version.
4. Confirm both project-catalog and available upstream verification.
5. Confirm corrupted/truncated fixture tests fail without offering Write/Install.

Record the result and exact error category, not secrets or raw paths.

## 5. Ring 2: disposable USB

1. Before launch, record which disks are attached using an OS-native read-only
   view or the candidate's inventory export.
2. Start the GUI as an ordinary user. Confirm it does not require initial
   elevation.
3. Select Create USB and confirm internal/system disks are not eligible.
4. Select the disposable target and compare model, capacity and connection.
5. Read the complete immutable plan. Stop if any detail is ambiguous.
6. Confirm once. Verify native elevation names the expected signed publisher.
7. Observe write, durable-flush and complete read-back verification stages.
8. If assigned a cancellation case, cancel only at the named stage and record
   the resulting journal/recovery state.
9. Confirm successful ejection where supported.
10. Compare the post-test device inventory. No non-target device may change.

Do not retry a wrong-target, unexpected elevation or non-target mutation.

## 6. Ring 3: boot media

1. Keep the host's normal system disk protected; use only the spare host named
   in the support cell.
2. Select the USB from firmware boot selection without changing unrelated
   firmware/security settings.
3. Confirm the image reaches its expected installer/live environment.
4. Do not install unless the candidate explicitly authorizes Ring 4.
5. Shut down normally and confirm the original host still boots.

Apple Silicon cannot boot/install the x86 Omarchy USB and is not a Ring 3 target
for that artifact.

## 7. Ring 4: spare-disk installation

Use only the provider-specific supplement. It must define exact supported disk,
boot, encryption and recovery states, expected partition/boot changes, safe
interruption stages and recovery exercises. If no supplement exists, Ring 4 is
not authorized.

After installation, record boot of Omarchy, boot/recovery of any promised host
OS, resulting layout, and removal/cleanup behavior. Do not claim support for a
neighboring hardware cell.

## 8. Failure evidence

1. Stop at the first safety invariant violation.
2. Photograph screens only if they contain no personal information; otherwise
   transcribe the exact error.
3. Generate diagnostics locally, preview every field and remove optional data.
4. Record operation ID/build/catalog/provider/artifact identifiers.
5. State the last completed stage and whether retry/reboot occurred.
6. Submit privately if the failure concerns privilege, verification, wrong disk,
   non-target mutation, data loss, boot loss or sensitive-data exposure.

Never attach raw disk images, memory dumps, recovery keys or unreviewed logs.

## 9. Maintainer disposition

Maintainers classify the result as reproduced, needs-safe-information, fixed,
excluded, duplicate or inconclusive. Useful failures become synthetic fixtures
and regression tests. Promotion evidence links the sanitized report and states
which exact support cell it qualifies.
