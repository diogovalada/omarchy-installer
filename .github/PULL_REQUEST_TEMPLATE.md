## Summary

Describe the user-visible or architectural change and why it is needed.

Omarchy Installer is a **community preview, not an official Omarchy release**. Do not imply endorsement by OmaCom or the Omarchy maintainers.

## Safety classification

- [ ] Non-destructive application, documentation, or test-only change
- [ ] Safety policy, device identity, media writing, privilege, IPC, artifact trust, update, recovery, or direct-install change

For a safety-critical change, describe the affected invariant, failure modes, and rollback or disable path:

<!-- Required for safety-critical changes. -->

## Evidence

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-targets --all-features`
- [ ] Desktop check, test, and build, if affected
- [ ] New or updated simulation/regression fixtures, if affected
- [ ] Documentation or ADR updated, if a contract or trust boundary changed

List the exact commands, fixtures, and results:

<!-- Do not attach unreviewed diagnostics, disk contents, credentials, signing material, or recovery keys. -->

## Review checklist

- [ ] No production signing, release, publishing, or deployment behavior was added.
- [ ] No test can access a real block device by default.
- [ ] Unknown or ambiguous states fail closed.
- [ ] Logs, fixtures, and screenshots contain no secrets or identifying device data.
- [ ] Third-party licensing and notices are preserved.
- [ ] Safety-critical changes have two independent reviewers and will not be self-merged.
