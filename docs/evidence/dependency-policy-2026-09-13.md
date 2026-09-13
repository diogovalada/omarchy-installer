# Dependency-policy review

The [September 12 CI failure](https://github.com/diogovalada/omarchy-installer/actions/runs/34709711505/job/103596120231)
had three causes. Other software tests and all four package builds passed.

## Local version requirements

The release client and fake-block-device test harness used path dependencies
without version requirements. They now declare 0.1.0, matching those internal
packages. Registry wildcard denial remains enabled.

## Certificate-data license

The locked webpki-roots 1.0.9 package declares CDLA-Permissive-2.0. A package- and
version-specific exception permits that license. The agreement permits sharing
the data with its license text; [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)
reproduces the package's LICENSE and accompanies application bundles and release
downloads. Other license requirements remain in force.

Sources: [license terms](https://cdla.dev/permissive-2-0/) and the LICENSE/Cargo.toml
distributed with webpki-roots 1.0.9.

## RUSTSEC-2023-0071

The locked pgp 0.20.0 dependency includes rsa 0.9.10. The
[advisory](https://rustsec.org/advisories/RUSTSEC-2023-0071.html) describes private-key
recovery through timing observations and currently lists no patched version.
This is a real upstream issue; this change does not claim to fix rsa.

The production integration in
[release-client](../../crates/release-client/src/lib.rs) parses the embedded
SignedPublicKey, checks its fingerprint and verifies a detached signature with
the primary public key. It does not load an RSA private key, sign messages or
decrypt messages. Test-only key generation is not a production signing service.
Based on that code-path review, this one advisory is recorded as an explicit
non-applicable exception. All other advisory checks remain enabled.

Reassess the exception before adding private-key operations or another use of
pgp/rsa, and when updating either dependency. Prefer a patched compatible version
when one becomes available. Do not reuse this exception as a general approval
for RSA signing/decryption or claim that the dependency itself is unaffected.
