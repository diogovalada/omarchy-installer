# Contributing to Omarchy Installer

Thank you for helping build a safer cross-platform Omarchy setup experience.
This is an **unofficial community project** and is not an OmaCom or Omarchy
release. Contributions must not imply upstream endorsement.

## Before contributing

Read:

- `README.md` for product scope and planned improvements;
- `docs/architecture.md` and `docs/threat-model.md` for trust boundaries;
- `docs/support-matrix.md` before changing a capability claim;
- the relevant files in `docs/adr/` before changing architecture;
- `docs/testing/tester-policy.md` before any hardware test.

Open a design discussion before implementing a new privileged verb, direct
installer, trust root, update path, provider protocol, real-device backend,
telemetry/data collection, or change to a release-blocking invariant.

## Scope and provenance

Prefer small, independently useful contributions. Integrate upstream projects
through documented providers or verified handoffs; do not copy their code or
assets without compatible licensing, attribution and maintainer agreement.
Preserve notices and record new third-party sources.

Do not use the Omarchy name, artwork or maintainer identity in a way that implies
official status. Public development packages must say “community preview — not
an official Omarchy release” until branding permission changes.

## Safety-critical changes

The following areas are safety-critical:

- device classification, target identity and raw writes;
- privileged helpers, IPC and authorization;
- catalog/update trust, signing and rollback protection;
- partition, boot, recovery and direct-install behavior;
- journaling, ownership/removal and diagnostics redaction;
- release packaging that selects real versus simulated backends.

They require two independent approvals, no self-merge, focused tests and an
updated threat model/ADR when assumptions change. Reviewers should evaluate both
the requested operation and how a malicious UI, provider, cache or local process
could abuse it.

No test may touch a real disk by default. Unit/integration tests use file-backed
devices and sentinel non-target disks. A real-device test must be separately
enabled, name the exact disposable target through a stable identity, follow the
tester policy and never run in ordinary CI.

## Development expectations

- Keep safety policy in the shared Rust core, not frontend code.
- Use typed, bounded protocols; unknown input must not expand privilege.
- Fail closed on unsupported or ambiguous hardware and state.
- Add unit/property/model/fuzz or fault-injection coverage proportional to risk.
- Convert hardware failures into sanitized synthetic regression fixtures.
- Avoid telemetry and automatic diagnostic upload.
- Never commit credentials, signing material, user logs or real hardware IDs.
- Update documentation and the exact support cell with behavior changes.

Run the repository's formatting, linting, unit, integration and package checks
before requesting review. Until those commands are scaffolded, record the exact
commands and platform used in the pull request.

## Commit and pull-request notes

Use focused commits with an imperative summary. A pull request should state:

1. user-visible outcome and non-goals;
2. threat model and privilege impact;
3. tests and simulation/fault cases run;
4. platforms and exact support cells affected;
5. recovery or rollback behavior;
6. third-party provenance/licensing impact;
7. documentation/ADR changes;
8. whether physical evidence exists (never imply it when only CI passed).

By contributing, certify the Developer Certificate of Origin 1.1 with a
`Signed-off-by:` trailer. A new CLA is not required unless future upstream
adoption requires one.

## Security and conduct

Do not open a public issue for a vulnerability or suspected wrong-target/data-
loss defect. Follow `SECURITY.md`. Be respectful, do not pressure people into
destructive testing, and never solicit secrets or remote access from testers.
