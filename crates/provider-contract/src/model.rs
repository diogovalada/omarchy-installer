use std::{collections::BTreeMap, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::PROVIDER_PROTOCOL_V1;

const MAX_ID_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 4_096;

macro_rules! bounded_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, JsonSchema, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
                let value = value.into();
                validate_id(stringify!($name), &value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(de::Error::custom)
            }
        }
    };
}

bounded_id!(MessageId);
bounded_id!(ProviderId);
bounded_id!(CapabilityId);
bounded_id!(OperationId);
bounded_id!(PlanId);
bounded_id!(ArtifactId);
bounded_id!(DeviceId);
bounded_id!(FindingCode);
bounded_id!(ResourceId);
bounded_id!(DiagnosticCode);

fn validate_id(field: &'static str, value: &str) -> Result<(), ValidationError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-/".contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(ValidationError::InvalidIdentifier { field })
    }
}

#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SchemaVersion(u16);

impl SchemaVersion {
    pub const V1: Self = Self(PROVIDER_PROTOCOL_V1);

    pub const fn get(self) -> u16 {
        self.0
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u16::deserialize(deserializer)?;
        if value == PROVIDER_PROTOCOL_V1 {
            Ok(Self(value))
        } else {
            Err(de::Error::custom(format_args!(
                "unsupported provider schema version {value}; expected {PROVIDER_PROTOCOL_V1}"
            )))
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, JsonSchema, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn parse(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into();
        let valid = value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if valid {
            Ok(Self(value))
        } else {
            Err(ValidationError::InvalidSha256)
        }
    }

    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ValidationError {
    #[error("{field} is not a bounded portable identifier")]
    InvalidIdentifier { field: &'static str },
    #[error("SHA-256 must be exactly 64 lowercase hexadecimal characters")]
    InvalidSha256,
    #[error("plan digest does not match its immutable contents")]
    PlanDigestMismatch,
    #[error("{field} exceeds the maximum text length")]
    TextTooLong { field: &'static str },
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderDescriptor {
    pub provider_id: ProviderId,
    pub display_name: String,
    pub provider_version: String,
    pub protocol_versions: Vec<u16>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostOs {
    Windows,
    Macos,
    Linux,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X86_64,
    Aarch64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareKind {
    Uefi,
    Apple,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostFacts {
    pub os: HostOs,
    pub architecture: Architecture,
    pub os_version: String,
    pub device_model: Option<String>,
    pub firmware: FirmwareKind,
    pub memory_bytes: u64,
    pub secure_boot_enabled: Option<bool>,
    pub virtualization_available: Option<bool>,
    pub on_ac_power: Option<bool>,
    pub snapshot_digest: Sha256Digest,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    TryVirtualMachine,
    DownloadArtifact,
    CreateUsb,
    DirectInstall,
    ManageOwnedResources,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Unavailable,
    SimulationOnly,
    Experimental,
    HardwareQualified,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDescriptor {
    pub capability_id: CapabilityId,
    pub operation_kind: OperationKind,
    pub support_level: SupportLevel,
    pub requires_elevation: bool,
    pub reason_code: Option<FindingCode>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyReference {
    pub policy_version: u64,
    pub policy_digest: Sha256Digest,
    pub expires_at_unix_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    pub artifact_id: ArtifactId,
    pub sha256: Sha256Digest,
    pub length_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceIdentity {
    pub stable_id: DeviceId,
    pub capacity_bytes: u64,
    pub removable: bool,
    pub internal: bool,
    pub system_disk: bool,
    pub model: Option<String>,
    pub serial_suffix: Option<String>,
    pub inventory_digest: Sha256Digest,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistenceMode {
    Disposable,
    Persistent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMode {
    Alongside,
    SeparateDisk,
    Replace,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperationIntent {
    TryVirtualMachine {
        artifact: ArtifactReference,
        persistence: PersistenceMode,
        memory_bytes: u64,
        storage_bytes: u64,
    },
    DownloadArtifact {
        artifact: ArtifactReference,
    },
    CreateUsb {
        artifact: ArtifactReference,
        target: DeviceIdentity,
        verify_after_write: bool,
    },
    DirectInstall {
        artifact: ArtifactReference,
        target: DeviceIdentity,
        mode: InstallMode,
    },
    ManageOwnedResources {
        resource_id: ResourceId,
    },
}

impl OperationIntent {
    pub const fn kind(&self) -> OperationKind {
        match self {
            Self::TryVirtualMachine { .. } => OperationKind::TryVirtualMachine,
            Self::DownloadArtifact { .. } => OperationKind::DownloadArtifact,
            Self::CreateUsb { .. } => OperationKind::CreateUsb,
            Self::DirectInstall { .. } => OperationKind::DirectInstall,
            Self::ManageOwnedResources { .. } => OperationKind::ManageOwnedResources,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationRequest {
    pub operation_id: OperationId,
    pub capability_id: CapabilityId,
    pub host_snapshot_digest: Sha256Digest,
    pub intent: OperationIntent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Information,
    Warning,
    Blocking,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightFinding {
    pub code: FindingCode,
    pub severity: FindingSeverity,
    pub summary: String,
    pub remediation: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanAction {
    ResolveVerifiedArtifact {
        artifact: ArtifactReference,
    },
    CreateVirtualMachine {
        persistence: PersistenceMode,
        memory_bytes: u64,
        storage_bytes: u64,
    },
    WriteRemovableMedia {
        artifact: ArtifactReference,
        target: DeviceIdentity,
        verify_after_write: bool,
    },
    PrepareDirectInstall {
        artifact: ArtifactReference,
        target: DeviceIdentity,
        mode: InstallMode,
    },
    RemoveOwnedResource {
        resource_id: ResourceId,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStep {
    pub sequence: u32,
    pub action: PlanAction,
    pub requires_elevation: bool,
    pub destructive_effect: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnsignedPlan {
    pub plan_id: PlanId,
    pub operation_id: OperationId,
    pub provider_id: ProviderId,
    pub provider_version: String,
    pub host_snapshot_digest: Sha256Digest,
    pub policy: PolicyReference,
    pub created_at_unix_seconds: u64,
    pub expires_at_unix_seconds: u64,
    pub steps: Vec<PlanStep>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImmutablePlan {
    #[serde(flatten)]
    pub contents: UnsignedPlan,
    pub plan_digest: Sha256Digest,
}

impl ImmutablePlan {
    pub fn seal(contents: UnsignedPlan) -> Result<Self, serde_json::Error> {
        let encoded = serde_json::to_vec(&contents)?;
        Ok(Self {
            contents,
            plan_digest: Sha256Digest::of_bytes(&encoded),
        })
    }

    pub fn validate_digest(&self) -> Result<(), ValidationError> {
        let encoded =
            serde_json::to_vec(&self.contents).map_err(|_| ValidationError::TextTooLong {
                field: "serialized plan",
            })?;
        if Sha256Digest::of_bytes(&encoded) == self.plan_digest {
            Ok(())
        } else {
            Err(ValidationError::PlanDigestMismatch)
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationProof {
    pub plan_digest: Sha256Digest,
    pub approval_nonce: Sha256Digest,
    pub approved_at_unix_seconds: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStage {
    Queued,
    Revalidating,
    Downloading,
    Preparing,
    Mutating,
    Verifying,
    Finalizing,
    Complete,
    Interrupted,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressEvent {
    pub operation_id: OperationId,
    pub sequence: u64,
    pub occurred_at_unix_seconds: u64,
    pub stage: ProgressStage,
    pub completed_units: u64,
    pub total_units: Option<u64>,
    pub message_code: DiagnosticCode,
    pub cancellable: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    Succeeded,
    Cancelled,
    Interrupted,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationReceipt {
    pub operation_id: OperationId,
    pub plan_digest: Sha256Digest,
    pub outcome: OperationOutcome,
    pub completed_at_unix_seconds: u64,
    pub final_sequence: u64,
    pub result_codes: Vec<DiagnosticCode>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationSnapshot {
    pub operation_id: OperationId,
    pub plan_digest: Sha256Digest,
    pub stage: ProgressStage,
    pub last_sequence: u64,
    pub terminal: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDisposition {
    NothingToRecover,
    ResumeFromCheckpoint,
    RestartFromBeginning,
    ManualActionRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryResult {
    pub operation: OperationSnapshot,
    pub disposition: RecoveryDisposition,
    pub explanation_code: DiagnosticCode,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelDisposition {
    Accepted,
    AlreadyTerminal,
    UnsafeAtCurrentStage,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancelResult {
    pub operation: OperationSnapshot,
    pub disposition: CancelDisposition,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticEntry {
    pub code: DiagnosticCode,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticBundle {
    pub operation_id: OperationId,
    pub generated_at_unix_seconds: u64,
    pub entries: Vec<DiagnosticEntry>,
    pub redaction_version: u32,
    pub contains_sensitive_data: bool,
}

impl DiagnosticBundle {
    pub fn validate(&self) -> Result<(), ValidationError> {
        for entry in &self.entries {
            if entry.value.len() > MAX_TEXT_BYTES {
                return Err(ValidationError::TextTooLong {
                    field: "diagnostic value",
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorCode {
    InvalidRequest,
    UnsupportedProtocol,
    UnsupportedCapability,
    PreflightBlocked,
    StaleHostFacts,
    InvalidPlan,
    AuthorizationMismatch,
    OperationNotFound,
    InvalidTransition,
    ProviderUnavailable,
    Internal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryAdvice {
    Never,
    AfterReprobe,
    AfterUserAction,
    Later,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, Error)]
#[error("{code:?}: {message}")]
#[serde(deny_unknown_fields)]
pub struct ProviderError {
    pub code: ProviderErrorCode,
    pub message: String,
    pub retry: RetryAdvice,
    pub details: BTreeMap<String, String>,
}

impl ProviderError {
    pub fn new(code: ProviderErrorCode, message: impl Into<String>, retry: RetryAdvice) -> Self {
        Self {
            code,
            message: message.into(),
            retry,
            details: BTreeMap::new(),
        }
    }
}
