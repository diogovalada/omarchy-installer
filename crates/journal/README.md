# Omarchy operation journal

This crate records the minimum non-sensitive state needed to resume or audit an
installer operation. Entries are append-only JSON lines with a SHA-256 hash
chain. Storage backends publish a separately flushed head anchor after each log
write; filesystem storage rotates between two anchor slots so one remains valid
if a write is interrupted.

The journal accepts only operation/plan UUIDs, typed lifecycle events, and short
restricted identifiers. Do not add paths, commands, logs, error prose,
credentials, network tokens, partition labels supplied by users, or other
sensitive/free-form values.

## Crash behavior

- An incomplete JSON line is reported as `TruncatedEntry`.
- A valid log ahead of its anchor is treated as uncommitted and rejected.
- A log behind its anchor is treated as truncated and rejected.
- Invalid/reordered/edited entries fail chain validation.
- A valid active journal is resumable; a terminal journal is immutable.

The hashes are not signatures. An attacker who can replace both the journal and
the anchor files can construct a different valid chain. A higher-assurance
deployment should copy the final head hash to separately protected telemetry or
platform storage.
