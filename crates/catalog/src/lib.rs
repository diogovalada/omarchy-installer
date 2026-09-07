//! Strict, fail-closed verification for Omarchy Setup artifact catalogs.
//!
//! A signed envelope contains the exact catalog JSON bytes as base64. Signing
//! those bytes, rather than a re-serialized JSON value, avoids JSON
//! canonicalization ambiguity. The signed message is domain-separated.
//!
//! This crate deliberately performs no network or filesystem I/O. The caller
//! is responsible for storing [`CatalogState`] atomically only after a catalog
//! has been accepted.

use std::collections::{BTreeSet, HashSet};
use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

/// Maximum decoded catalog size. Catalogs are metadata and should stay small.
pub const MAX_CATALOG_BYTES: usize = 1024 * 1024;
pub const ENVELOPE_VERSION: u32 = 1;
pub const CATALOG_SCHEMA_VERSION: u32 = 1;

const SIGNING_DOMAIN: &[u8] = b"omarchy-setup-catalog-v1\0";

/// An offline trust root embedded by the application.
///
/// Production roots must be provisioned through a documented key ceremony.
/// The repository test root is public test material and has no production use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustRoot {
    pub key_id: String,
    pub public_key: [u8; 32],
}

impl TrustRoot {
    pub fn new(key_id: impl Into<String>, public_key: [u8; 32]) -> Self {
        Self {
            key_id: key_id.into(),
            public_key,
        }
    }
}

/// The wire envelope. Unknown fields are rejected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedCatalogEnvelope {
    pub envelope_version: u32,
    pub key_id: String,
    /// Exact UTF-8 JSON bytes of [`Catalog`], encoded with standard base64.
    pub catalog_base64: String,
    /// Ed25519 signature over the domain separator followed by catalog bytes.
    pub signature_base64: String,
}

/// Versioned catalog payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema_version: u32,
    /// Strictly monotonic publisher sequence, independent of release versions.
    pub sequence: u64,
    pub issued_at_unix: i64,
    /// Exclusive expiry boundary.
    pub expires_at_unix: i64,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// Stable machine identifier, unique within a catalog.
    pub id: String,
    /// Displayed upstream/release version.
    pub version: String,
    pub channel: ReleaseChannel,
    pub kind: ArtifactKind,
    /// HTTPS location. Redirect policy and TLS are enforced by the downloader.
    pub url: String,
    /// The only accepted byte length. Zero-length artifacts are invalid.
    pub length: u64,
    /// SHA-256 of exactly `length` bytes.
    pub sha256: Sha256Digest,
    /// Architecture of the downloaded/installable payload, not the host app.
    pub payload_architecture: Architecture,
    /// Host/capability combinations for which the publisher enables this item.
    pub constraints: Vec<PlatformConstraint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformConstraint {
    pub host_os: HostOs,
    pub host_architectures: BTreeSet<Architecture>,
    pub capabilities: BTreeSet<Capability>,
    /// Opaque platform-native version bounds interpreted by host policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_host_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_host_version: Option<String>,
    /// Required for capabilities whose behavior comes from a provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderConstraint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConstraint {
    pub id: String,
    pub min_protocol: u32,
    pub max_protocol: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostOs {
    Windows,
    Macos,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Architecture {
    X86_64,
    Aarch64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Download,
    CreateUsb,
    TryVm,
    DirectInstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    Iso,
    RawDiskImage,
    VmBundle,
    ProviderPackage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseChannel {
    Stable,
    Beta,
    Development,
}

/// Exact 32-byte SHA-256 digest, serialized as 64 lowercase hexadecimal chars.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            use std::fmt::Write as _;
            write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
        }
        output
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Sha256Digest")
            .field(&self.to_hex())
            .finish()
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse_lower_hex_digest(&value).map_err(serde::de::Error::custom)
    }
}

fn parse_lower_hex_digest(value: &str) -> Result<Sha256Digest, &'static str> {
    if value.len() != 64 {
        return Err("SHA-256 must contain exactly 64 lowercase hexadecimal characters");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("SHA-256 must contain exactly 64 lowercase hexadecimal characters");
    }

    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
    }
    Ok(Sha256Digest(bytes))
}

fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => unreachable!("input validated before conversion"),
    }
}

/// Persist this state atomically after accepting [`VerifiedCatalog`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogState {
    pub highest_sequence: u64,
    pub catalog_sha256: Sha256Digest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCatalog {
    pub catalog: Catalog,
    pub catalog_sha256: Sha256Digest,
    pub next_state: CatalogState,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifyError {
    #[error("invalid signed-catalog envelope: {0}")]
    InvalidEnvelope(String),
    #[error("unsupported envelope version {found}; expected {expected}")]
    UnsupportedEnvelopeVersion { found: u32, expected: u32 },
    #[error("catalog was signed by unknown key {0:?}")]
    UnknownKey(String),
    #[error("catalog payload exceeds the {MAX_CATALOG_BYTES}-byte limit")]
    CatalogTooLarge,
    #[error("catalog payload is not valid base64")]
    InvalidCatalogEncoding,
    #[error("catalog signature is not a 64-byte base64 Ed25519 signature")]
    InvalidSignatureEncoding,
    #[error("trust root contains an invalid Ed25519 public key")]
    InvalidTrustRoot,
    #[error("catalog signature verification failed")]
    InvalidSignature,
    #[error("catalog payload is not valid UTF-8 JSON matching schema v1: {0}")]
    InvalidCatalog(String),
    #[error("unsupported catalog schema version {found}; expected {expected}")]
    UnsupportedSchemaVersion { found: u32, expected: u32 },
    #[error("catalog is not yet valid: issued at {issued_at}, current time {now}")]
    NotYetValid { issued_at: i64, now: i64 },
    #[error("catalog expired at {expires_at}; current time {now}")]
    Expired { expires_at: i64, now: i64 },
    #[error("catalog validity interval is empty or inverted")]
    InvalidValidityInterval,
    #[error("catalog contains no artifacts")]
    EmptyCatalog,
    #[error("artifact {artifact_id:?} is invalid: {reason}")]
    InvalidArtifact { artifact_id: String, reason: String },
    #[error("catalog contains duplicate artifact id {0:?}")]
    DuplicateArtifact(String),
    #[error("catalog sequence {received} would roll back persisted sequence {highest}")]
    Rollback { received: u64, highest: u64 },
    #[error("catalog sequence {sequence} has different signed content than the persisted catalog")]
    SequenceEquivocation { sequence: u64 },
}

/// Verify a signed envelope and calculate the monotonic state to persist.
///
/// `now_unix` must come from the application's trusted wall clock. An equal
/// sequence with identical signed payload is accepted idempotently. An equal
/// sequence with different content is rejected as publisher equivocation.
pub fn verify_catalog(
    envelope_json: &[u8],
    root: &TrustRoot,
    now_unix: i64,
    previous_state: Option<&CatalogState>,
) -> Result<VerifiedCatalog, VerifyError> {
    let envelope: SignedCatalogEnvelope = serde_json::from_slice(envelope_json)
        .map_err(|error| VerifyError::InvalidEnvelope(error.to_string()))?;

    if envelope.envelope_version != ENVELOPE_VERSION {
        return Err(VerifyError::UnsupportedEnvelopeVersion {
            found: envelope.envelope_version,
            expected: ENVELOPE_VERSION,
        });
    }
    if envelope.key_id != root.key_id {
        return Err(VerifyError::UnknownKey(envelope.key_id));
    }

    // Bound allocation using the encoded length before decoding.
    let maximum_encoded_length = MAX_CATALOG_BYTES.div_ceil(3) * 4;
    if envelope.catalog_base64.len() > maximum_encoded_length {
        return Err(VerifyError::CatalogTooLarge);
    }
    let catalog_bytes = BASE64
        .decode(envelope.catalog_base64)
        .map_err(|_| VerifyError::InvalidCatalogEncoding)?;
    if catalog_bytes.len() > MAX_CATALOG_BYTES {
        return Err(VerifyError::CatalogTooLarge);
    }

    let signature_bytes = BASE64
        .decode(envelope.signature_base64)
        .map_err(|_| VerifyError::InvalidSignatureEncoding)?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| VerifyError::InvalidSignatureEncoding)?;
    let verifying_key =
        VerifyingKey::from_bytes(&root.public_key).map_err(|_| VerifyError::InvalidTrustRoot)?;

    let mut signed_message = Vec::with_capacity(SIGNING_DOMAIN.len() + catalog_bytes.len());
    signed_message.extend_from_slice(SIGNING_DOMAIN);
    signed_message.extend_from_slice(&catalog_bytes);
    verifying_key
        .verify_strict(&signed_message, &signature)
        .map_err(|_| VerifyError::InvalidSignature)?;

    let catalog: Catalog = serde_json::from_slice(&catalog_bytes)
        .map_err(|error| VerifyError::InvalidCatalog(error.to_string()))?;
    validate_catalog(&catalog, now_unix)?;

    let catalog_sha256 = Sha256Digest::of(&catalog_bytes);
    if let Some(previous) = previous_state {
        if catalog.sequence < previous.highest_sequence {
            return Err(VerifyError::Rollback {
                received: catalog.sequence,
                highest: previous.highest_sequence,
            });
        }
        if catalog.sequence == previous.highest_sequence
            && catalog_sha256 != previous.catalog_sha256
        {
            return Err(VerifyError::SequenceEquivocation {
                sequence: catalog.sequence,
            });
        }
    }

    let next_state = CatalogState {
        highest_sequence: catalog.sequence,
        catalog_sha256,
    };
    Ok(VerifiedCatalog {
        catalog,
        catalog_sha256,
        next_state,
    })
}

fn validate_catalog(catalog: &Catalog, now_unix: i64) -> Result<(), VerifyError> {
    if catalog.schema_version != CATALOG_SCHEMA_VERSION {
        return Err(VerifyError::UnsupportedSchemaVersion {
            found: catalog.schema_version,
            expected: CATALOG_SCHEMA_VERSION,
        });
    }
    if catalog.issued_at_unix >= catalog.expires_at_unix {
        return Err(VerifyError::InvalidValidityInterval);
    }
    if now_unix < catalog.issued_at_unix {
        return Err(VerifyError::NotYetValid {
            issued_at: catalog.issued_at_unix,
            now: now_unix,
        });
    }
    if now_unix >= catalog.expires_at_unix {
        return Err(VerifyError::Expired {
            expires_at: catalog.expires_at_unix,
            now: now_unix,
        });
    }
    if catalog.artifacts.is_empty() {
        return Err(VerifyError::EmptyCatalog);
    }

    let mut ids = HashSet::with_capacity(catalog.artifacts.len());
    for artifact in &catalog.artifacts {
        if !ids.insert(artifact.id.as_str()) {
            return Err(VerifyError::DuplicateArtifact(artifact.id.clone()));
        }
        validate_artifact(artifact)?;
    }
    Ok(())
}

fn validate_artifact(artifact: &Artifact) -> Result<(), VerifyError> {
    let invalid = |reason: &str| VerifyError::InvalidArtifact {
        artifact_id: artifact.id.clone(),
        reason: reason.to_owned(),
    };

    if !is_safe_identifier(&artifact.id) {
        return Err(invalid(
            "id must be 1-128 ASCII letters, digits, '.', '_' or '-'",
        ));
    }
    if artifact.version.is_empty() || artifact.version.len() > 128 || !artifact.version.is_ascii() {
        return Err(invalid("version must be 1-128 ASCII characters"));
    }
    if !artifact.url.starts_with("https://") || artifact.url.len() > 2048 {
        return Err(invalid(
            "url must be an HTTPS URL no longer than 2048 bytes",
        ));
    }
    if artifact.length == 0 {
        return Err(invalid("length must be greater than zero"));
    }
    if artifact.constraints.is_empty() {
        return Err(invalid("at least one platform constraint is required"));
    }
    for constraint in &artifact.constraints {
        if constraint.host_architectures.is_empty() {
            return Err(invalid("host architecture set cannot be empty"));
        }
        if constraint.capabilities.is_empty() {
            return Err(invalid("capability set cannot be empty"));
        }
        validate_optional_version(constraint.min_host_version.as_deref()).map_err(&invalid)?;
        validate_optional_version(constraint.max_host_version.as_deref()).map_err(&invalid)?;

        let requires_provider = constraint.capabilities.contains(&Capability::TryVm)
            || constraint.capabilities.contains(&Capability::DirectInstall);
        if requires_provider && constraint.provider.is_none() {
            return Err(invalid(
                "try-vm and direct-install require a provider constraint",
            ));
        }
        if let Some(provider) = &constraint.provider {
            if !is_safe_identifier(&provider.id) {
                return Err(invalid("provider id is invalid"));
            }
            if provider.min_protocol == 0 || provider.min_protocol > provider.max_protocol {
                return Err(invalid("provider protocol range is invalid"));
            }
        }
    }
    Ok(())
}

fn validate_optional_version(value: Option<&str>) -> Result<(), &'static str> {
    if let Some(value) = value
        && (value.is_empty() || value.len() > 64 || !value.is_ascii())
    {
        return Err("host version bounds must be 1-64 ASCII characters");
    }
    Ok(())
}

fn is_safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};
    use serde_json::json;

    const TEST_KEY_ID: &str = "omarchy-setup-test-root-1";
    const TEST_SEED: [u8; 32] = [0x42; 32];
    const VALID_AT: i64 = 1_900_000_000;

    fn test_root() -> TrustRoot {
        let signing = SigningKey::from_bytes(&TEST_SEED);
        TrustRoot::new(TEST_KEY_ID, signing.verifying_key().to_bytes())
    }

    fn catalog(sequence: u64, expires_at_unix: i64) -> Catalog {
        Catalog {
            schema_version: CATALOG_SCHEMA_VERSION,
            sequence,
            issued_at_unix: 1_800_000_000,
            expires_at_unix,
            artifacts: vec![Artifact {
                id: "omarchy-x86_64-stable-iso".into(),
                version: "2026.09.1".into(),
                channel: ReleaseChannel::Stable,
                kind: ArtifactKind::Iso,
                url: "https://releases.example.test/omarchy.iso".into(),
                length: 4_294_967_296,
                sha256: Sha256Digest::from_bytes([0xab; 32]),
                payload_architecture: Architecture::X86_64,
                constraints: vec![PlatformConstraint {
                    host_os: HostOs::Windows,
                    host_architectures: BTreeSet::from([
                        Architecture::X86_64,
                        Architecture::Aarch64,
                    ]),
                    capabilities: BTreeSet::from([Capability::Download, Capability::CreateUsb]),
                    min_host_version: Some("10.0.19045".into()),
                    max_host_version: None,
                    provider: None,
                }],
            }],
        }
    }

    fn sign_catalog(catalog: &Catalog) -> Vec<u8> {
        let payload = serde_json::to_vec(catalog).unwrap();
        sign_payload(&payload)
    }

    fn sign_payload(payload: &[u8]) -> Vec<u8> {
        let signing = SigningKey::from_bytes(&TEST_SEED);
        let mut message = SIGNING_DOMAIN.to_vec();
        message.extend_from_slice(payload);
        let signature = signing.sign(&message);
        serde_json::to_vec(&SignedCatalogEnvelope {
            envelope_version: ENVELOPE_VERSION,
            key_id: TEST_KEY_ID.into(),
            catalog_base64: BASE64.encode(payload),
            signature_base64: BASE64.encode(signature.to_bytes()),
        })
        .unwrap()
    }

    #[test]
    fn valid_vector_verifies_and_returns_state() {
        let envelope = sign_catalog(&catalog(7, 2_000_000_000));
        let verified = verify_catalog(&envelope, &test_root(), VALID_AT, None).unwrap();
        assert_eq!(verified.catalog.sequence, 7);
        assert_eq!(verified.next_state.highest_sequence, 7);
        assert_eq!(verified.next_state.catalog_sha256, verified.catalog_sha256);
    }

    #[test]
    fn invalid_signature_vector_is_rejected() {
        let mut envelope: serde_json::Value =
            serde_json::from_slice(&sign_catalog(&catalog(7, 2_000_000_000))).unwrap();
        let signature = envelope["signature_base64"].as_str().unwrap();
        let mut bytes = BASE64.decode(signature).unwrap();
        bytes[0] ^= 0x80;
        envelope["signature_base64"] = json!(BASE64.encode(bytes));

        let error = verify_catalog(
            &serde_json::to_vec(&envelope).unwrap(),
            &test_root(),
            VALID_AT,
            None,
        )
        .unwrap_err();
        assert_eq!(error, VerifyError::InvalidSignature);
    }

    #[test]
    fn expired_vector_is_rejected_at_exclusive_boundary() {
        let envelope = sign_catalog(&catalog(7, VALID_AT));
        let error = verify_catalog(&envelope, &test_root(), VALID_AT, None).unwrap_err();
        assert_eq!(
            error,
            VerifyError::Expired {
                expires_at: VALID_AT,
                now: VALID_AT,
            }
        );
    }

    #[test]
    fn rollback_vector_is_rejected() {
        let previous_catalog = sign_catalog(&catalog(8, 2_000_000_000));
        let state = verify_catalog(&previous_catalog, &test_root(), VALID_AT, None)
            .unwrap()
            .next_state;
        let older = sign_catalog(&catalog(7, 2_000_000_000));
        let error = verify_catalog(&older, &test_root(), VALID_AT, Some(&state)).unwrap_err();
        assert_eq!(
            error,
            VerifyError::Rollback {
                received: 7,
                highest: 8,
            }
        );
    }

    #[test]
    fn equal_sequence_is_idempotent_only_for_identical_signed_payload() {
        let original = sign_catalog(&catalog(8, 2_000_000_000));
        let state = verify_catalog(&original, &test_root(), VALID_AT, None)
            .unwrap()
            .next_state;
        verify_catalog(&original, &test_root(), VALID_AT, Some(&state)).unwrap();

        let mut changed = catalog(8, 2_000_000_000);
        changed.artifacts[0].version = "2026.09.2".into();
        let error = verify_catalog(
            &sign_catalog(&changed),
            &test_root(),
            VALID_AT,
            Some(&state),
        )
        .unwrap_err();
        assert_eq!(error, VerifyError::SequenceEquivocation { sequence: 8 });
    }

    #[test]
    fn unknown_envelope_and_payload_fields_fail_closed() {
        let mut envelope: serde_json::Value =
            serde_json::from_slice(&sign_catalog(&catalog(7, 2_000_000_000))).unwrap();
        envelope["surprise"] = json!(true);
        assert!(matches!(
            verify_catalog(
                &serde_json::to_vec(&envelope).unwrap(),
                &test_root(),
                VALID_AT,
                None
            ),
            Err(VerifyError::InvalidEnvelope(_))
        ));

        let mut payload = serde_json::to_value(catalog(7, 2_000_000_000)).unwrap();
        payload["surprise"] = json!(true);
        let envelope = sign_payload(&serde_json::to_vec(&payload).unwrap());
        assert!(matches!(
            verify_catalog(&envelope, &test_root(), VALID_AT, None),
            Err(VerifyError::InvalidCatalog(_))
        ));
    }

    #[test]
    fn malformed_hash_and_zero_length_are_rejected() {
        let mut payload = serde_json::to_value(catalog(7, 2_000_000_000)).unwrap();
        payload["artifacts"][0]["sha256"] = json!("AB");
        let error = verify_catalog(
            &sign_payload(&serde_json::to_vec(&payload).unwrap()),
            &test_root(),
            VALID_AT,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, VerifyError::InvalidCatalog(_)));

        let mut zero = catalog(7, 2_000_000_000);
        zero.artifacts[0].length = 0;
        let error = verify_catalog(&sign_catalog(&zero), &test_root(), VALID_AT, None).unwrap_err();
        assert!(matches!(error, VerifyError::InvalidArtifact { .. }));
    }

    #[test]
    fn provider_capabilities_require_valid_protocol_constraint() {
        let mut missing = catalog(7, 2_000_000_000);
        missing.artifacts[0].constraints[0]
            .capabilities
            .insert(Capability::DirectInstall);
        let error =
            verify_catalog(&sign_catalog(&missing), &test_root(), VALID_AT, None).unwrap_err();
        assert!(matches!(error, VerifyError::InvalidArtifact { .. }));

        missing.artifacts[0].constraints[0].provider = Some(ProviderConstraint {
            id: "direct-windows".into(),
            min_protocol: 3,
            max_protocol: 2,
        });
        let error =
            verify_catalog(&sign_catalog(&missing), &test_root(), VALID_AT, None).unwrap_err();
        assert!(matches!(error, VerifyError::InvalidArtifact { .. }));
    }

    #[test]
    fn signature_cannot_be_replayed_outside_the_domain() {
        let payload = serde_json::to_vec(&catalog(7, 2_000_000_000)).unwrap();
        let signing = SigningKey::from_bytes(&TEST_SEED);
        let signature = signing.sign(&payload);
        let envelope = serde_json::to_vec(&SignedCatalogEnvelope {
            envelope_version: ENVELOPE_VERSION,
            key_id: TEST_KEY_ID.into(),
            catalog_base64: BASE64.encode(payload),
            signature_base64: BASE64.encode(signature.to_bytes()),
        })
        .unwrap();
        assert_eq!(
            verify_catalog(&envelope, &test_root(), VALID_AT, None).unwrap_err(),
            VerifyError::InvalidSignature
        );
    }

    #[test]
    #[ignore = "developer aid for regenerating checked-in test vectors"]
    fn emit_test_vector_material() {
        println!("PUBLIC={:?}", test_root().public_key);
        for (name, item) in [
            ("valid", catalog(7, 2_000_000_000)),
            ("expired", catalog(8, VALID_AT)),
            ("rollback", catalog(6, 2_000_000_000)),
        ] {
            println!("{name}={}", String::from_utf8(sign_catalog(&item)).unwrap());
        }
    }
}
