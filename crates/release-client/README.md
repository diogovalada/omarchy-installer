# Official release client

Read-only release discovery and resumable HTTPS downloads for Omarchy Installer.
This crate has no device discovery, disk-writing, elevation, installer launch,
or downloaded-code execution capability.

## API and ownership

Call the synchronous API in a desktop blocking worker:

```rust,no_run
use omarchy_release_client::{download, resolve_current};
use std::sync::atomic::AtomicBool;

let release = resolve_current()?;
let image = download(&release, "private-cache", &AtomicBool::new(false), |event| {
    // Forward event.phase / received_bytes / total_bytes to the desktop.
})?;
assert!(image.signature_verified);
# Ok::<(), omarchy_release_client::Error>(())
```

`Release` is immutable outside the crate and serializable for display, but is
not deserializable. Keep the authoritative object in native application state;
the frontend must not select URLs, metadata, or signing keys. `DownloadedImage`
returns its content-addressed cache path only after all checks pass. The desktop
owns the final user-facing export and should avoid overwriting existing files.

Progress phases serialize as `preparing`, `downloading`, `verifying_signature`,
and `complete`. The `preparing` phase includes checksum validation and partial
rehashing. The signature phase reports bytes verified, independently of network
progress. Cancellation returns `Error::Cancelled`. Network operations check the
flag at most 100 ms apart, with 15-second connect/read timeouts; metadata also
has a 30-second total deadline. Signature and checksum readers check cancellation
between blocks. Cache lookup and partial rehash also report progress and honor
cancellation between 64 KiB blocks through fallible progress callbacks.

## Authentication and source pins

Current metadata is obtained from the ISO link on
<https://omarchy.org/>, with ISO, checksum, and detached signature restricted to
<https://iso.omarchy.org/>. Homepage HTML is bounded to 1 MiB, checksum to 1 KiB,
and signature to 16 KiB. Releases require an exact `omarchy-X.Y.Z.iso` filename,
a matching single checksum record, and an explicit length between 1 byte and
64 GiB. No redirect is followed. Encoded responses, ambiguous HTTP framing,
incorrect range totals/offsets, unexpected statuses, and length changes fail.

The embedded public key comes from the upstream builder at immutable commit
`2673c613d9a71e23920e43fbb951238145e0f1e8`:

- [Pinned upstream public key](https://raw.githubusercontent.com/omacom/omarchy-iso/2673c613d9a71e23920e43fbb951238145e0f1e8/builder/omarchy.gpg)
- [Upstream ISO signing script](https://github.com/omacom/omarchy-iso/blob/2673c613d9a71e23920e43fbb951238145e0f1e8/bin/omarchy-iso-sign)
- [Upstream builder source](https://github.com/omacom/omarchy-iso/tree/2673c613d9a71e23920e43fbb951238145e0f1e8)

Primary signing fingerprint:
`40DFB630FF42BCFFB047046CF0134EE680CAC571`.
Embedded armored-file SHA-256:
`15d6aac44df688165b2ea35fe0b23af239bbc66a6909c10a5c219e8d94b707de`.
The key identifies `Omarchy <pkgs@omarchy.org>` and is attributed to upstream
Omarchy/Omacom. Its identity is a deliberate application trust pin. Updating it
requires a reviewed source change; the UI, cache, and network cannot replace it.

Native verification uses [rPGP 0.20](https://docs.rs/pgp/0.20.0/pgp/) and its
streaming `packet::Signature::verify` API, with no GPG installation requirement.
The crate verifies the embedded certificate bindings and primary fingerprint,
accepts exactly one binary signature packet with SHA-256/384/512, rejects unknown
critical semantics and expired/future signatures, and verifies the whole ISO
against the pinned signing primary key. Encryption subkeys cannot authorize an
image. SHA-256 sidecars alone never establish publisher authentication.

This is a direct upstream signing-key integration, **not a project catalog
signature or a project production-key ceremony**. It authenticates ISO bytes to
that pinned key. HTTPS homepage discovery does not provide cryptographic
freshness or rollback protection, and this slice does not fetch revocations or
automatically rotate keys. The embedded upstream key currently has no expiry or
revocation. No public test-catalog root is used.

## Resume and cache behavior

`omarchy-downloader::VerifiedCache` and `DownloadSession` provide unique partial
files, incremental length/SHA-256 enforcement, and atomic content-addressed
promotion. A digest-scoped OS file lock prevents concurrent client processes
from opening the same persisted release session. The token is persisted beside
the private cache so app restart can resume from actual partial bytes. State and
token reads are bounded, and resumed specs must match the discovered release.
A torn, stale, or malformed pointer/state removes only the trusted digest-scoped
pointer and starts a new unique partial. Orphan files remain intact; untrusted
tokens never cause deletion of other files.

A resumed request must return a valid `206` matching the exact requested start,
final byte, full length, and body length. A valid full `200` causes an explicit
restart with a fresh partial; it is never appended to existing bytes. Network
failure, truncation, and cancellation preserve partials. Complete bytes with a
bad checksum are discarded, so retry can fetch them again. Valid-checksum bytes
with a failing signature remain partial and never get promoted. A bad cached
artifact is removed under its artifact lock and fetched again. A cache hit is
rechecked for checksum, length, and pinned-key signature before return.

The cache is private application data, not a defense against a malicious process
already able to alter the same user's files concurrently. The path-return API
does not guarantee immutability after verification; any future privileged writer
must establish its own protected handle and verification boundary.

## Verification and provenance

Implementation agent model: **gpt-6-astra**. Implemented and checked on Windows
with repository Rust 1.93.0. This crate requires Rust 1.88 because rPGP 0.20 does.

```powershell
$env:CARGO_TARGET_DIR = Join-Path $PWD 'crates/release-client/target-local'
cargo test -p omarchy-release-client
cargo clippy -p omarchy-release-client --all-targets --no-deps -- -D warnings
cargo fmt -p omarchy-release-client -- --check

# Explicit small network smoke; never downloads an ISO body:
cargo test -p omarchy-release-client official_metadata_smoke -- --ignored --nocapture

# Optional: native authentication of an existing local ISO, with fresh metadata:
cargo run -p omarchy-release-client --example official_release -- 'C:/path/omarchy-4.0.2.iso'
```

Local TCP fixtures are compiled only under `cfg(test)`. They generate unrelated
test-only Ed25519 keys in memory and cover cancellation/resume across calls,
cache signature revalidation, corrupt cache repair, bad/ignored ranges,
truncated responses, content encodings, redirects, checksum mismatches, bad
signers, malformed filenames/metadata/signature packets, and metadata bounds.
Production has no HTTP fixture mode or caller-provided trust key.

Live discovery on 2026-09-05 resolved version `4.0.2`, length `6227752960`, SHA-256
`2ef8e624aa1bec7e277e28056b8535a6c9373ba48d7ede3f1a01cb6d2373cfb8`.
The detached signature is 119 bytes. Full native verification of that existing
6,227,752,960-byte ISO passed: exact length, SHA-256, and detached signature
against the embedded primary key. The command used local Cargo config overrides
`profile.dev.package.sha2.opt-level=3` and `profile.dev.package.pgp.opt-level=3`;
unoptimized crypto is slow on multi-gigabyte files. Release builds optimize by
default. No ISO copy or second ISO download was needed.

Final local checks: 12 release-client unit/fixture tests and 11 downloader tests
passed; strict Clippy for both crates and all targets passed. The one ignored
live metadata test is separately opt-in. Cargo-deny is not installed locally;
the repository dependency-policy CI remains the authority for that check.
