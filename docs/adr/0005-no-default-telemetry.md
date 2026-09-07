# ADR 0005: No default telemetry; manual previewable diagnostics

- Status: accepted; release-blocking invariant
- Date: 2026-09-02

## Context

Installation diagnostics can reveal usernames, paths, disk layouts, serials,
encryption status, firmware details and recovery information. The project does
need structured evidence from opt-in hardware testing, particularly while local
hardware is unavailable, but background collection would create privacy,
security, consent and retention obligations.

## Decision

Ship no telemetry, analytics SDK, hidden identifier, automatic crash upload or
automatic support-bundle upload. Generate diagnostics locally from an explicit
field allowlist. Redact them, show the exact result to the user, and require a
separate manual upload action.

Never collect passwords, recovery keys, Wi-Fi details, disk contents, full
serial numbers or persistent hardware identifiers. When a hardware attribute is
needed for one report, prefer a per-bundle salted digest that cannot correlate
reports by default.

## Consequences

- Ordinary use produces no background reporting.
- Testers retain control and can inspect every submitted field.
- Maintainers receive less population-level data and must recruit structured
  opt-in testers.
- Redaction and preview code becomes security-sensitive and requires adversarial
  tests.
- Any future telemetry proposal requires a new ADR and explicit consent design;
  it cannot be introduced as a minor implementation change.

## Alternatives rejected

- **Opt-out analytics:** inappropriate for an unofficial disk-management tool and
  conflicts with data minimization.
- **Automatic crash dumps:** native dumps can contain secrets and raw memory.
- **Anonymous persistent installation ID:** still enables cross-event linkage and
  is unnecessary for the initial evidence model.
