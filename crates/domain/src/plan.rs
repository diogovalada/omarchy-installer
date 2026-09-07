use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    CapabilityKind, CapabilityRequest, HostBinding, HostFacts, PolicyDecision, PolicyViolation,
    SafetyPolicy, evaluate_capability, host::is_lower_hex_sha256, policy::valid_identifier,
};

const PLAN_SCHEMA_VERSION: u16 = 1;

/// User-visible operation identifier, also used to correlate the journal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(pub String);

/// Exact artifact metadata obtained from a verified catalog.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub artifact_id: String,
    pub length_bytes: u64,
    pub sha256: String,
}

/// Exact provider build bound into a privileged plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderBinding {
    pub provider_id: String,
    pub protocol_version: u16,
    pub manifest_sha256: String,
}

/// Stable whole-device observations supplied by a platform adapter.
// These are independent observations, including contradictory facts rejected by policy.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetIdentity {
    pub stable_id: String,
    pub display_name: String,
    pub capacity_bytes: u64,
    pub removable: bool,
    pub internal: bool,
    pub system_disk: bool,
    pub identity_ambiguous: bool,
}

/// Destructive direct-install mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectInstallMode {
    Alongside,
    Replace,
}

/// Management action for an operation owned by this application.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManageAction {
    Resume,
    Repair,
    Remove,
}

/// Complete user intent before the planner derives executable steps.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum OperationIntent {
    Download {
        artifact: ArtifactRef,
        destination_token: String,
    },
    CreateUsb {
        artifact: ArtifactRef,
        target: TargetIdentity,
    },
    TryVm {
        provider: ProviderBinding,
    },
    DirectInstall {
        provider: ProviderBinding,
        mode: DirectInstallMode,
    },
    Manage {
        provider: ProviderBinding,
        action: ManageAction,
        owned_operation_id: OperationId,
    },
    ExportDiagnostics {
        owned_operation_id: Option<OperationId>,
    },
}

impl OperationIntent {
    fn capability(&self) -> CapabilityKind {
        match self {
            Self::Download { .. } => CapabilityKind::Download,
            Self::CreateUsb { .. } => CapabilityKind::CreateUsb,
            Self::TryVm { .. } => CapabilityKind::TryVm,
            Self::DirectInstall { mode, .. } => match mode {
                DirectInstallMode::Alongside => CapabilityKind::DirectInstallAlongside,
                DirectInstallMode::Replace => CapabilityKind::DirectInstallReplace,
            },
            Self::Manage { action, .. } => match action {
                ManageAction::Resume => CapabilityKind::Resume,
                ManageAction::Repair => CapabilityKind::Repair,
                ManageAction::Remove => CapabilityKind::Remove,
            },
            Self::ExportDiagnostics { .. } => CapabilityKind::ExportDiagnostics,
        }
    }

    fn provider_id(&self) -> Option<&str> {
        match self {
            Self::TryVm { provider }
            | Self::DirectInstall { provider, .. }
            | Self::Manage { provider, .. } => Some(&provider.provider_id),
            Self::Download { .. } | Self::CreateUsb { .. } | Self::ExportDiagnostics { .. } => None,
        }
    }
}

/// Input to the pure planner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanRequest {
    pub operation_id: OperationId,
    pub host: HostBinding,
    pub intent: OperationIntent,
}

/// Typed allowlisted steps. There is intentionally no shell-command step.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "step")]
pub enum PlanStep {
    DownloadArtifact {
        artifact: ArtifactRef,
    },
    VerifyArtifact {
        artifact: ArtifactRef,
    },
    ReidentifyTarget {
        target: TargetIdentity,
    },
    LockWholeDevice {
        stable_id: String,
    },
    UnmountTargetVolumes {
        stable_id: String,
    },
    WriteArtifact {
        stable_id: String,
        artifact: ArtifactRef,
    },
    FlushDevice {
        stable_id: String,
    },
    ReadBackVerify {
        stable_id: String,
        length_bytes: u64,
        sha256: String,
    },
    EjectDevice {
        stable_id: String,
    },
    InvokeProvider {
        provider: ProviderBinding,
        capability: CapabilityKind,
    },
    ExportDiagnostics {
        owned_operation_id: Option<OperationId>,
    },
}

/// Canonical plan payload. Field order is part of schema version 1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalPlan {
    schema_version: u16,
    operation_id: OperationId,
    policy_revision: u64,
    host: HostBinding,
    intent: OperationIntent,
    authorization_required: bool,
    verification_required: bool,
    steps: Vec<PlanStep>,
}

/// A validated plan whose steps cannot be mutated through the public API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImmutablePlan(CanonicalPlan);

impl ImmutablePlan {
    /// Deterministic UTF-8 JSON. Schema v1 contains no maps or floating values.
    ///
    /// # Errors
    /// Returns `CanonicalSerialization` if the plan cannot be serialized.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, PlanError> {
        serde_json::to_vec(&self.0).map_err(|_| PlanError::CanonicalSerialization)
    }

    /// SHA-256 of [`Self::canonical_bytes`].
    ///
    /// # Errors
    /// Returns `CanonicalSerialization` if the plan cannot be serialized.
    pub fn digest(&self) -> Result<PlanDigest, PlanError> {
        let bytes = self.canonical_bytes()?;
        Ok(PlanDigest(lower_hex(&Sha256::digest(bytes))))
    }

    #[must_use]
    pub const fn payload(&self) -> &CanonicalPlan {
        &self.0
    }

    #[must_use]
    pub const fn authorization_required(&self) -> bool {
        self.0.authorization_required
    }

    #[must_use]
    pub const fn verification_required(&self) -> bool {
        self.0.verification_required
    }

    #[must_use]
    pub fn steps(&self) -> &[PlanStep] {
        &self.0.steps
    }
}

/// Lower-case SHA-256 digest of a canonical plan.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanDigest(pub String);

impl fmt::Display for PlanDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Planner rejection. Invalid or unknown input never produces a partial plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanError {
    Denied(PolicyViolation),
    InvalidOperationId,
    InvalidHostFingerprint,
    InvalidArtifact,
    InvalidDestination,
    InvalidProvider,
    InvalidOwnedOperationId,
    UnsafeTarget(TargetRejection),
    CanonicalSerialization,
}

/// Why media policy rejected a whole device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetRejection {
    InvalidStableIdentity,
    AmbiguousIdentity,
    NotRemovable,
    Internal,
    SystemDisk,
    TooSmall,
}

impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "operation plan rejected: {self:?}")
    }
}

impl Error for PlanError {}

/// Validates the request, applies policy, and derives the complete step list.
///
/// # Errors
/// Rejects invalid identifiers or bindings, denied capabilities, invalid artifacts,
/// and unsafe target devices without returning a partial plan.
pub fn plan(
    facts: &HostFacts,
    policy: &SafetyPolicy,
    request: PlanRequest,
) -> Result<ImmutablePlan, PlanError> {
    validate_identifier(&request.operation_id.0, PlanError::InvalidOperationId)?;
    if !request.host.validate() {
        return Err(PlanError::InvalidHostFingerprint);
    }

    let capability = request.intent.capability();
    match evaluate_capability(
        facts,
        policy,
        CapabilityRequest {
            kind: capability,
            provider_id: request.intent.provider_id(),
        },
    ) {
        PolicyDecision::Allowed(_) => {}
        PolicyDecision::Denied(reason) => return Err(PlanError::Denied(reason)),
    }

    let (steps, verification_required) = derive_steps(&request.intent, capability)?;
    Ok(ImmutablePlan(CanonicalPlan {
        schema_version: PLAN_SCHEMA_VERSION,
        operation_id: request.operation_id,
        policy_revision: policy.revision,
        host: request.host,
        intent: request.intent,
        authorization_required: capability.requires_elevation(),
        verification_required,
        steps,
    }))
}

fn derive_steps(
    intent: &OperationIntent,
    capability: CapabilityKind,
) -> Result<(Vec<PlanStep>, bool), PlanError> {
    match intent {
        OperationIntent::Download {
            artifact,
            destination_token,
        } => {
            validate_artifact(artifact)?;
            validate_identifier(destination_token, PlanError::InvalidDestination)?;
            Ok((
                vec![
                    PlanStep::DownloadArtifact {
                        artifact: artifact.clone(),
                    },
                    PlanStep::VerifyArtifact {
                        artifact: artifact.clone(),
                    },
                ],
                true,
            ))
        }
        OperationIntent::CreateUsb { artifact, target } => {
            validate_artifact(artifact)?;
            validate_target(target, artifact.length_bytes)?;
            let id = target.stable_id.clone();
            Ok((
                vec![
                    PlanStep::DownloadArtifact {
                        artifact: artifact.clone(),
                    },
                    PlanStep::VerifyArtifact {
                        artifact: artifact.clone(),
                    },
                    PlanStep::ReidentifyTarget {
                        target: target.clone(),
                    },
                    PlanStep::LockWholeDevice {
                        stable_id: id.clone(),
                    },
                    PlanStep::UnmountTargetVolumes {
                        stable_id: id.clone(),
                    },
                    PlanStep::WriteArtifact {
                        stable_id: id.clone(),
                        artifact: artifact.clone(),
                    },
                    PlanStep::FlushDevice {
                        stable_id: id.clone(),
                    },
                    PlanStep::ReadBackVerify {
                        stable_id: id.clone(),
                        length_bytes: artifact.length_bytes,
                        sha256: artifact.sha256.clone(),
                    },
                    PlanStep::EjectDevice { stable_id: id },
                ],
                true,
            ))
        }
        OperationIntent::TryVm { provider }
        | OperationIntent::DirectInstall { provider, .. }
        | OperationIntent::Manage { provider, .. } => {
            validate_provider(provider)?;
            if let OperationIntent::Manage {
                owned_operation_id, ..
            } = intent
            {
                validate_identifier(&owned_operation_id.0, PlanError::InvalidOwnedOperationId)?;
            }
            Ok((
                vec![PlanStep::InvokeProvider {
                    provider: provider.clone(),
                    capability,
                }],
                false,
            ))
        }
        OperationIntent::ExportDiagnostics { owned_operation_id } => {
            if let Some(id) = owned_operation_id {
                validate_identifier(&id.0, PlanError::InvalidOwnedOperationId)?;
            }
            Ok((
                vec![PlanStep::ExportDiagnostics {
                    owned_operation_id: owned_operation_id.clone(),
                }],
                false,
            ))
        }
    }
}

fn validate_artifact(artifact: &ArtifactRef) -> Result<(), PlanError> {
    if !valid_identifier(&artifact.artifact_id)
        || artifact.length_bytes == 0
        || !is_lower_hex_sha256(&artifact.sha256)
    {
        return Err(PlanError::InvalidArtifact);
    }
    Ok(())
}

fn validate_provider(provider: &ProviderBinding) -> Result<(), PlanError> {
    if !valid_identifier(&provider.provider_id)
        || provider.protocol_version == 0
        || !is_lower_hex_sha256(&provider.manifest_sha256)
    {
        return Err(PlanError::InvalidProvider);
    }
    Ok(())
}

fn validate_target(target: &TargetIdentity, artifact_length: u64) -> Result<(), PlanError> {
    if !valid_identifier(&target.stable_id) {
        return Err(PlanError::UnsafeTarget(
            TargetRejection::InvalidStableIdentity,
        ));
    }
    if target.identity_ambiguous {
        return Err(PlanError::UnsafeTarget(TargetRejection::AmbiguousIdentity));
    }
    if !target.removable {
        return Err(PlanError::UnsafeTarget(TargetRejection::NotRemovable));
    }
    if target.internal {
        return Err(PlanError::UnsafeTarget(TargetRejection::Internal));
    }
    if target.system_disk {
        return Err(PlanError::UnsafeTarget(TargetRejection::SystemDisk));
    }
    if target.capacity_bytes < artifact_length {
        return Err(PlanError::UnsafeTarget(TargetRejection::TooSmall));
    }
    Ok(())
}

fn validate_identifier(value: &str, error: PlanError) -> Result<(), PlanError> {
    if valid_identifier(value) {
        Ok(())
    } else {
        Err(error)
    }
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}
