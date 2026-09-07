# Omarchy diagnostics

This crate creates strict, local-only diagnostic bundles. It contains no
filesystem, network, telemetry, or upload API.

Privacy properties:

- all free-form strings are sanitized before entering the serializable model;
- known usernames and home paths can be registered as per-run sensitive
  literals;
- common absolute paths, labelled disk identifiers, recovery keys, email
  addresses, IP addresses, and MAC addresses are redacted;
- arbitrary metadata keys are rejected and sensitive key names are explicitly
  forbidden;
- devices receive random per-bundle UUIDs and exact capacities are coarsened;
- JSON is returned only as caller-owned memory for preview and explicit action.

Pattern redaction is defense in depth, not a proof that arbitrary prose cannot
contain an unrecognizable identifier. Callers should prefer typed event codes,
avoid free-form text, and register host-known identities with `Sanitizer`.
