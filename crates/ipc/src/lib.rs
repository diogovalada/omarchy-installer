//! Strict protocol types for communication across the Omarchy Setup privilege
//! boundary.
//!
//! This crate deliberately contains no transport, operating-system IPC, disk,
//! network, or privilege code. A transport must authenticate its peer and then
//! pass complete, length-delimited frames to [`decode_frame`].

#![forbid(unsafe_code)]

use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use thiserror::Error;

/// Only this protocol version is accepted by this crate.
pub const PROTOCOL_VERSION: u16 = 1;
/// Hard upper bound for one encoded message, including its envelope.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
/// Hard upper bound for human-readable identifiers.
pub const MAX_IDENTIFIER_BYTES: usize = 128;
/// Hard upper bound for the immutable steps in a plan.
pub const MAX_PLAN_STEPS: usize = 64;
/// Hard upper bound for an export request.
pub const MAX_EXPORT_BYTES: u64 = 16 * 1024 * 1024;

/// An identifier with a small, transport-independent representation.
///
/// Identifiers are ASCII and limited to letters, digits, `.`, `_`, `-`, `:`,
/// and `/`. They are identifiers rather than user-facing text.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct Identifier(String);

impl Identifier {
    pub fn new(value: impl Into<String>) -> Result<Self, ProtocolError> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_identifier(value: &str) -> Result<(), ProtocolError> {
    if value.is_empty() {
        return Err(ProtocolError::InvalidIdentifier("identifier is empty"));
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err(ProtocolError::InvalidIdentifier("identifier is too long"));
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
    }) {
        return Err(ProtocolError::InvalidIdentifier(
            "identifier contains a disallowed character",
        ));
    }
    Ok(())
}

impl<'de> Deserialize<'de> for Identifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Identifier::new(value).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

macro_rules! fixed_bytes_type {
    ($name:ident, $length:expr) => {
        #[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Hash)]
        pub struct $name([u8; $length]);

        impl $name {
            pub const LENGTH: usize = $length;

            pub const fn from_bytes(bytes: [u8; $length]) -> Self {
                Self(bytes)
            }

            pub const fn as_bytes(&self) -> &[u8; $length] {
                &self.0
            }

            pub fn to_hex(&self) -> String {
                encode_hex(&self.0)
            }

            pub fn from_hex(value: &str) -> Result<Self, ProtocolError> {
                let bytes = decode_hex::<$length>(value)?;
                Ok(Self(bytes))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.to_hex())
                    .finish()
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(&self.to_hex())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::from_hex(&value).map_err(serde::de::Error::custom)
            }
        }
    };
}

fixed_bytes_type!(SessionNonce, 32);
fixed_bytes_type!(RequestId, 16);
fixed_bytes_type!(OperationId, 16);
fixed_bytes_type!(Sha256Digest, 32);

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], ProtocolError> {
    if value.len() != N * 2 {
        return Err(ProtocolError::InvalidFixedBytes {
            expected_hex_chars: N * 2,
            actual_hex_chars: value.len(),
        });
    }
    let mut output = [0; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = decode_nibble(pair[0])?;
        let low = decode_nibble(pair[1])?;
        output[index] = (high << 4) | low;
    }
    Ok(output)
}

fn decode_nibble(value: u8) -> Result<u8, ProtocolError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(ProtocolError::InvalidHex),
    }
}

/// Identity of the exact verified artifact a plan may consume.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdentity {
    pub artifact_id: Identifier,
    pub catalog_generation: u64,
    pub byte_length: u64,
    pub sha256: Sha256Digest,
}

/// Identity of a whole target device at selection time.
///
/// `fingerprint` is produced from immutable or slow-changing device properties
/// selected by the platform adapter. The helper must still re-probe the target
/// immediately before mutation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetIdentity {
    pub stable_id: Identifier,
    pub device_kind: DeviceKind,
    pub capacity_bytes: u64,
    pub fingerprint: Sha256Digest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    RemovableUsb,
    InternalDisk,
    VirtualDisk,
    Simulation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    WriteUsb,
    DownloadOnly,
    TryVirtualMachine,
    DirectInstallAlongside,
    DirectInstallReplace,
}

/// A closed set of operations. It intentionally offers no arbitrary command,
/// path, byte range, or generic file-write primitive.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanStep {
    VerifyArtifact,
    ReidentifyTarget,
    AcquireExclusiveTarget,
    WriteArtifact,
    FlushTarget,
    VerifyWrittenArtifact,
    EjectTarget,
    LaunchQualifiedProvider { provider_id: Identifier },
}

/// The plan authorized by the user and later revalidated by a privileged helper.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImmutablePlan {
    pub operation_id: OperationId,
    pub action: PlanAction,
    pub artifact: ArtifactIdentity,
    pub target: TargetIdentity,
    pub steps: Vec<PlanStep>,
    pub plan_hash: Sha256Digest,
}

impl ImmutablePlan {
    pub fn new(
        operation_id: OperationId,
        action: PlanAction,
        artifact: ArtifactIdentity,
        target: TargetIdentity,
        steps: Vec<PlanStep>,
    ) -> Result<Self, ProtocolError> {
        validate_steps(&steps)?;
        let mut plan = Self {
            operation_id,
            action,
            artifact,
            target,
            steps,
            plan_hash: Sha256Digest::from_bytes([0; 32]),
        };
        plan.plan_hash = plan.computed_hash();
        Ok(plan)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_steps(&self.steps)?;
        if self.plan_hash != self.computed_hash() {
            return Err(ProtocolError::PlanHashMismatch);
        }
        if self.artifact.byte_length == 0 {
            return Err(ProtocolError::InvalidPlan("artifact length is zero"));
        }
        if self.target.capacity_bytes == 0 {
            return Err(ProtocolError::InvalidPlan("target capacity is zero"));
        }
        if self.action == PlanAction::WriteUsb
            && self.target.device_kind != DeviceKind::RemovableUsb
            && self.target.device_kind != DeviceKind::Simulation
        {
            return Err(ProtocolError::InvalidPlan(
                "write_usb requires a removable or simulation target",
            ));
        }
        if self.action == PlanAction::WriteUsb
            && self.artifact.byte_length > self.target.capacity_bytes
        {
            return Err(ProtocolError::InvalidPlan(
                "artifact does not fit on target",
            ));
        }
        Ok(())
    }

    pub fn binding(&self) -> PlanBinding {
        PlanBinding {
            operation_id: self.operation_id,
            plan_hash: self.plan_hash,
            artifact: self.artifact.clone(),
            target: self.target.clone(),
        }
    }

    fn computed_hash(&self) -> Sha256Digest {
        let mut hasher = Sha256::new();
        hasher.update(b"omarchy-setup-plan\0v1\0");
        hasher.update(self.operation_id.as_bytes());
        hasher.update([self.action as u8]);
        hash_artifact(&mut hasher, &self.artifact);
        hash_target(&mut hasher, &self.target);
        hasher.update((self.steps.len() as u64).to_be_bytes());
        for step in &self.steps {
            match step {
                PlanStep::VerifyArtifact => hasher.update([0]),
                PlanStep::ReidentifyTarget => hasher.update([1]),
                PlanStep::AcquireExclusiveTarget => hasher.update([2]),
                PlanStep::WriteArtifact => hasher.update([3]),
                PlanStep::FlushTarget => hasher.update([4]),
                PlanStep::VerifyWrittenArtifact => hasher.update([5]),
                PlanStep::EjectTarget => hasher.update([6]),
                PlanStep::LaunchQualifiedProvider { provider_id } => {
                    hasher.update([7]);
                    hash_bytes(&mut hasher, provider_id.as_str().as_bytes());
                }
            }
        }
        Sha256Digest::from_bytes(hasher.finalize().into())
    }
}

fn validate_steps(steps: &[PlanStep]) -> Result<(), ProtocolError> {
    if steps.is_empty() {
        return Err(ProtocolError::InvalidPlan("plan contains no steps"));
    }
    if steps.len() > MAX_PLAN_STEPS {
        return Err(ProtocolError::InvalidPlan("plan contains too many steps"));
    }
    Ok(())
}

fn hash_artifact(hasher: &mut Sha256, artifact: &ArtifactIdentity) {
    hash_bytes(hasher, artifact.artifact_id.as_str().as_bytes());
    hasher.update(artifact.catalog_generation.to_be_bytes());
    hasher.update(artifact.byte_length.to_be_bytes());
    hasher.update(artifact.sha256.as_bytes());
}

fn hash_target(hasher: &mut Sha256, target: &TargetIdentity) {
    hash_bytes(hasher, target.stable_id.as_str().as_bytes());
    hasher.update([target.device_kind as u8]);
    hasher.update(target.capacity_bytes.to_be_bytes());
    hasher.update(target.fingerprint.as_bytes());
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

/// Exact identities that must match between prepare and execute.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanBinding {
    pub operation_id: OperationId,
    pub plan_hash: Sha256Digest,
    pub artifact: ArtifactIdentity,
    pub target: TargetIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    pub protocol_version: u16,
    pub session_nonce: SessionNonce,
    /// Strictly increasing, gap-free sequence number, starting at zero.
    pub sequence: u64,
    pub request_id: RequestId,
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn new(
        session_nonce: SessionNonce,
        sequence: u64,
        request_id: RequestId,
        payload: T,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            session_nonce,
            sequence,
            request_id,
            payload,
        }
    }

    pub fn validate_version(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion {
                received: self.protocol_version,
                supported: PROTOCOL_VERSION,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "body", rename_all = "snake_case")]
pub enum ClientMessage {
    Prepare(PrepareRequest),
    Execute(ExecuteRequest),
    Cancel(CancelRequest),
    Query(QueryRequest),
    Export(ExportRequest),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareRequest {
    pub plan: ImmutablePlan,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteRequest {
    pub binding: PlanBinding,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelRequest {
    pub operation_id: OperationId,
    pub plan_hash: Sha256Digest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryRequest {
    pub operation_id: OperationId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionLevel {
    Standard,
    Strict,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportRequest {
    pub operation_id: OperationId,
    pub redaction: RedactionLevel,
    pub max_bytes: u64,
}

impl ExportRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.max_bytes == 0 || self.max_bytes > MAX_EXPORT_BYTES {
            return Err(ProtocolError::InvalidExportLimit {
                received: self.max_bytes,
                maximum: MAX_EXPORT_BYTES,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "body", rename_all = "snake_case")]
pub enum ServerMessage {
    Prepared(PreparedResponse),
    Accepted(AcceptedResponse),
    Progress(ProgressEvent),
    State(StateResponse),
    Exported(ExportedResponse),
    Error(ErrorResponse),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedResponse {
    pub binding: PlanBinding,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedResponse {
    pub operation_id: OperationId,
    pub state: OperationState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressPhase {
    VerifyingArtifact,
    ReidentifyingTarget,
    AcquiringTarget,
    WritingArtifact,
    FlushingTarget,
    VerifyingWrite,
    EjectingTarget,
    RunningProvider,
    Finalizing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressDisposition {
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgressEvent {
    pub operation_id: OperationId,
    /// Per-operation, gap-free event number, starting at zero.
    pub event_sequence: u64,
    pub phase: ProgressPhase,
    pub completed_units: u64,
    pub total_units: u64,
    pub disposition: ProgressDisposition,
}

impl ProgressEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.total_units == 0 {
            return Err(ProtocolError::InvalidProgress("total units is zero"));
        }
        if self.completed_units > self.total_units {
            return Err(ProtocolError::InvalidProgress(
                "completed units exceeds total units",
            ));
        }
        if self.disposition == ProgressDisposition::Completed
            && self.completed_units != self.total_units
        {
            return Err(ProtocolError::InvalidProgress(
                "completed event is not at total units",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateResponse {
    pub operation_id: OperationId,
    pub state: OperationState,
    pub binding: PlanBinding,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedResponse {
    pub operation_id: OperationId,
    pub byte_length: u64,
    pub sha256: Sha256Digest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidMessage,
    InvalidState,
    IdentityChanged,
    ReplayRejected,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorResponse {
    pub code: ErrorCode,
    /// Stable, bounded identifier suitable for mapping to localized UI text.
    pub message_id: Identifier,
    pub operation_id: Option<OperationId>,
}

/// Decode one already-delimited frame. The size check happens before JSON
/// allocation or parsing, and trailing non-whitespace input is rejected.
pub fn decode_frame<T: DeserializeOwned>(bytes: &[u8]) -> Result<Envelope<T>, ProtocolError> {
    decode_frame_with_limit(bytes, MAX_FRAME_BYTES)
}

pub fn decode_frame_with_limit<T: DeserializeOwned>(
    bytes: &[u8],
    maximum_bytes: usize,
) -> Result<Envelope<T>, ProtocolError> {
    if bytes.is_empty() {
        return Err(ProtocolError::EmptyFrame);
    }
    if bytes.len() > maximum_bytes {
        return Err(ProtocolError::FrameTooLarge {
            actual: bytes.len(),
            maximum: maximum_bytes,
        });
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let envelope = Envelope::<T>::deserialize(&mut deserializer)
        .map_err(|error| ProtocolError::MalformedFrame(error.to_string()))?;
    deserializer
        .end()
        .map_err(|error| ProtocolError::MalformedFrame(error.to_string()))?;
    envelope.validate_version()?;
    Ok(envelope)
}

pub fn encode_frame<T: Serialize>(envelope: &Envelope<T>) -> Result<Vec<u8>, ProtocolError> {
    envelope.validate_version()?;
    let encoded = serde_json::to_vec(envelope)
        .map_err(|error| ProtocolError::MalformedFrame(error.to_string()))?;
    if encoded.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge {
            actual: encoded.len(),
            maximum: MAX_FRAME_BYTES,
        });
    }
    Ok(encoded)
}

/// Session-scoped replay protection. Sequence numbers are intentionally
/// gap-free: a missing message terminates or resynchronizes the session rather
/// than allowing an attacker to suppress an operation silently.
#[derive(Clone, Debug)]
pub struct ReplayGuard {
    session_nonce: SessionNonce,
    next_sequence: u64,
}

impl ReplayGuard {
    pub fn new(session_nonce: SessionNonce) -> Self {
        Self {
            session_nonce,
            next_sequence: 0,
        }
    }

    pub fn accept<T>(&mut self, envelope: &Envelope<T>) -> Result<(), ProtocolError> {
        envelope.validate_version()?;
        if envelope.session_nonce != self.session_nonce {
            return Err(ProtocolError::SessionNonceMismatch);
        }
        if envelope.sequence != self.next_sequence {
            return Err(ProtocolError::UnexpectedSequence {
                received: envelope.sequence,
                expected: self.next_sequence,
            });
        }
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(ProtocolError::SequenceExhausted)?;
        Ok(())
    }

    pub fn next_sequence(&self) -> u64 {
        self.next_sequence
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Prepared,
    Executing,
    Completed,
    Cancelled,
    Failed,
}

impl OperationState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Clone, Debug)]
struct TrackedOperation {
    binding: PlanBinding,
    state: OperationState,
    next_progress_sequence: u64,
    completed_units: u64,
}

/// Pure state validator shared by both ends of the protocol. It performs no
/// action; it only rejects confused-deputy bindings and invalid transitions.
#[derive(Clone, Debug, Default)]
pub struct OperationTracker {
    operations: BTreeMap<OperationId, TrackedOperation>,
}

impl OperationTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn prepare(&mut self, request: &PrepareRequest) -> Result<PlanBinding, ProtocolError> {
        request.plan.validate()?;
        let operation_id = request.plan.operation_id;
        if self.operations.contains_key(&operation_id) {
            return Err(ProtocolError::OperationAlreadyExists(operation_id));
        }
        let binding = request.plan.binding();
        self.operations.insert(
            operation_id,
            TrackedOperation {
                binding: binding.clone(),
                state: OperationState::Prepared,
                next_progress_sequence: 0,
                completed_units: 0,
            },
        );
        Ok(binding)
    }

    pub fn execute(&mut self, request: &ExecuteRequest) -> Result<(), ProtocolError> {
        let tracked = self
            .operations
            .get_mut(&request.binding.operation_id)
            .ok_or(ProtocolError::UnknownOperation(
                request.binding.operation_id,
            ))?;
        if tracked.state != OperationState::Prepared {
            return Err(ProtocolError::InvalidTransition {
                from: tracked.state,
                requested: "execute",
            });
        }
        if tracked.binding != request.binding {
            return Err(ProtocolError::BindingMismatch);
        }
        tracked.state = OperationState::Executing;
        Ok(())
    }

    pub fn progress(&mut self, event: &ProgressEvent) -> Result<OperationState, ProtocolError> {
        event.validate()?;
        let tracked = self
            .operations
            .get_mut(&event.operation_id)
            .ok_or(ProtocolError::UnknownOperation(event.operation_id))?;
        if tracked.state != OperationState::Executing {
            return Err(ProtocolError::InvalidTransition {
                from: tracked.state,
                requested: "progress",
            });
        }
        if event.event_sequence != tracked.next_progress_sequence {
            return Err(ProtocolError::UnexpectedProgressSequence {
                received: event.event_sequence,
                expected: tracked.next_progress_sequence,
            });
        }
        if event.completed_units < tracked.completed_units {
            return Err(ProtocolError::ProgressRegressed);
        }
        tracked.next_progress_sequence = tracked
            .next_progress_sequence
            .checked_add(1)
            .ok_or(ProtocolError::SequenceExhausted)?;
        tracked.completed_units = event.completed_units;
        tracked.state = match event.disposition {
            ProgressDisposition::Running => OperationState::Executing,
            ProgressDisposition::Completed => OperationState::Completed,
            ProgressDisposition::Cancelled => OperationState::Cancelled,
            ProgressDisposition::Failed => OperationState::Failed,
        };
        Ok(tracked.state)
    }

    pub fn cancel(&mut self, request: &CancelRequest) -> Result<(), ProtocolError> {
        let tracked = self
            .operations
            .get_mut(&request.operation_id)
            .ok_or(ProtocolError::UnknownOperation(request.operation_id))?;
        if tracked.binding.plan_hash != request.plan_hash {
            return Err(ProtocolError::BindingMismatch);
        }
        if !matches!(
            tracked.state,
            OperationState::Prepared | OperationState::Executing
        ) {
            return Err(ProtocolError::InvalidTransition {
                from: tracked.state,
                requested: "cancel",
            });
        }
        tracked.state = OperationState::Cancelled;
        Ok(())
    }

    pub fn query(&self, request: &QueryRequest) -> Result<OperationState, ProtocolError> {
        self.operations
            .get(&request.operation_id)
            .map(|tracked| tracked.state)
            .ok_or(ProtocolError::UnknownOperation(request.operation_id))
    }

    /// Diagnostics export is allowed only after mutation has stopped.
    pub fn export(&self, request: &ExportRequest) -> Result<(), ProtocolError> {
        request.validate()?;
        let tracked = self
            .operations
            .get(&request.operation_id)
            .ok_or(ProtocolError::UnknownOperation(request.operation_id))?;
        if !tracked.state.is_terminal() {
            return Err(ProtocolError::InvalidTransition {
                from: tracked.state,
                requested: "export",
            });
        }
        Ok(())
    }

    pub fn binding(&self, operation_id: OperationId) -> Option<&PlanBinding> {
        self.operations.get(&operation_id).map(|item| &item.binding)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ProtocolError {
    #[error("empty frame")]
    EmptyFrame,
    #[error("frame is {actual} bytes, exceeding the {maximum}-byte limit")]
    FrameTooLarge { actual: usize, maximum: usize },
    #[error("malformed frame: {0}")]
    MalformedFrame(String),
    #[error("unsupported protocol version {received}; supported version is {supported}")]
    UnsupportedVersion { received: u16, supported: u16 },
    #[error("invalid identifier: {0}")]
    InvalidIdentifier(&'static str),
    #[error(
        "fixed byte value has {actual_hex_chars} hex characters; expected {expected_hex_chars}"
    )]
    InvalidFixedBytes {
        expected_hex_chars: usize,
        actual_hex_chars: usize,
    },
    #[error("hex values must use lowercase ASCII characters")]
    InvalidHex,
    #[error("invalid plan: {0}")]
    InvalidPlan(&'static str),
    #[error("plan hash does not match immutable plan fields")]
    PlanHashMismatch,
    #[error("session nonce does not match")]
    SessionNonceMismatch,
    #[error("received message sequence {received}; expected {expected}")]
    UnexpectedSequence { received: u64, expected: u64 },
    #[error("received progress sequence {received}; expected {expected}")]
    UnexpectedProgressSequence { received: u64, expected: u64 },
    #[error("sequence space exhausted")]
    SequenceExhausted,
    #[error("operation {0:?} already exists")]
    OperationAlreadyExists(OperationId),
    #[error("unknown operation {0:?}")]
    UnknownOperation(OperationId),
    #[error("plan, artifact, or target binding does not match prepared operation")]
    BindingMismatch,
    #[error("cannot request {requested} while operation is {from:?}")]
    InvalidTransition {
        from: OperationState,
        requested: &'static str,
    },
    #[error("invalid progress event: {0}")]
    InvalidProgress(&'static str),
    #[error("progress regressed")]
    ProgressRegressed,
    #[error("export limit {received} is invalid; maximum is {maximum}")]
    InvalidExportLimit { received: u64, maximum: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bytes<const N: usize>(value: u8) -> [u8; N] {
        [value; N]
    }

    fn sample_plan() -> ImmutablePlan {
        ImmutablePlan::new(
            OperationId::from_bytes(bytes(1)),
            PlanAction::WriteUsb,
            ArtifactIdentity {
                artifact_id: Identifier::new("omarchy-x86_64:4.0").unwrap(),
                catalog_generation: 42,
                byte_length: 1_000,
                sha256: Sha256Digest::from_bytes(bytes(2)),
            },
            TargetIdentity {
                stable_id: Identifier::new("usb:serial-123").unwrap(),
                device_kind: DeviceKind::RemovableUsb,
                capacity_bytes: 2_000,
                fingerprint: Sha256Digest::from_bytes(bytes(3)),
            },
            vec![
                PlanStep::VerifyArtifact,
                PlanStep::ReidentifyTarget,
                PlanStep::AcquireExclusiveTarget,
                PlanStep::WriteArtifact,
                PlanStep::FlushTarget,
                PlanStep::VerifyWrittenArtifact,
                PlanStep::EjectTarget,
            ],
        )
        .unwrap()
    }

    fn envelope(payload: ClientMessage, sequence: u64) -> Envelope<ClientMessage> {
        Envelope::new(
            SessionNonce::from_bytes(bytes(9)),
            sequence,
            RequestId::from_bytes(bytes(sequence as u8)),
            payload,
        )
    }

    #[test]
    fn client_message_round_trips() {
        let original = envelope(
            ClientMessage::Prepare(PrepareRequest {
                plan: sample_plan(),
            }),
            0,
        );
        let encoded = encode_frame(&original).unwrap();
        let decoded: Envelope<ClientMessage> = decode_frame(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn rejects_unknown_envelope_field() {
        let mut value = serde_json::to_value(envelope(
            ClientMessage::Prepare(PrepareRequest {
                plan: sample_plan(),
            }),
            0,
        ))
        .unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("future_privilege".into(), json!(true));

        let error =
            decode_frame::<ClientMessage>(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(matches!(error, ProtocolError::MalformedFrame(_)));
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_unknown_payload_field() {
        let mut value = serde_json::to_value(envelope(
            ClientMessage::Execute(ExecuteRequest {
                binding: sample_plan().binding(),
            }),
            0,
        ))
        .unwrap();
        value["payload"]["body"]
            .as_object_mut()
            .unwrap()
            .insert("command".into(), json!("format-everything"));

        let error =
            decode_frame::<ClientMessage>(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(matches!(error, ProtocolError::MalformedFrame(_)));
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_oversized_frame_before_parsing() {
        let bytes = vec![b'x'; 257];
        assert_eq!(
            decode_frame_with_limit::<ClientMessage>(&bytes, 256),
            Err(ProtocolError::FrameTooLarge {
                actual: 257,
                maximum: 256
            })
        );
    }

    #[test]
    fn rejects_wrong_version_and_trailing_data() {
        let mut wrong = envelope(
            ClientMessage::Query(QueryRequest {
                operation_id: OperationId::from_bytes(bytes(1)),
            }),
            0,
        );
        wrong.protocol_version = 2;
        let encoded = serde_json::to_vec(&wrong).unwrap();
        assert_eq!(
            decode_frame::<ClientMessage>(&encoded),
            Err(ProtocolError::UnsupportedVersion {
                received: 2,
                supported: 1
            })
        );

        let mut trailing = serde_json::to_vec(&envelope(
            ClientMessage::Query(QueryRequest {
                operation_id: OperationId::from_bytes(bytes(1)),
            }),
            0,
        ))
        .unwrap();
        trailing.extend_from_slice(b" true");
        assert!(matches!(
            decode_frame::<ClientMessage>(&trailing),
            Err(ProtocolError::MalformedFrame(_))
        ));
    }

    #[test]
    fn identifier_and_fixed_fields_are_bounded() {
        assert!(Identifier::new("a".repeat(MAX_IDENTIFIER_BYTES)).is_ok());
        assert!(matches!(
            Identifier::new("a".repeat(MAX_IDENTIFIER_BYTES + 1)),
            Err(ProtocolError::InvalidIdentifier(_))
        ));
        assert!(SessionNonce::from_hex(&"00".repeat(31)).is_err());
        assert!(SessionNonce::from_hex(&"AA".repeat(32)).is_err());
    }

    #[test]
    fn replay_guard_rejects_duplicate_gap_and_wrong_session() {
        let nonce = SessionNonce::from_bytes(bytes(9));
        let mut guard = ReplayGuard::new(nonce);
        let first = envelope(
            ClientMessage::Query(QueryRequest {
                operation_id: OperationId::from_bytes(bytes(1)),
            }),
            0,
        );
        guard.accept(&first).unwrap();
        assert_eq!(
            guard.accept(&first),
            Err(ProtocolError::UnexpectedSequence {
                received: 0,
                expected: 1
            })
        );

        let gap = envelope(
            ClientMessage::Query(QueryRequest {
                operation_id: OperationId::from_bytes(bytes(1)),
            }),
            2,
        );
        assert_eq!(
            guard.accept(&gap),
            Err(ProtocolError::UnexpectedSequence {
                received: 2,
                expected: 1
            })
        );

        let mut wrong_nonce = envelope(
            ClientMessage::Query(QueryRequest {
                operation_id: OperationId::from_bytes(bytes(1)),
            }),
            1,
        );
        wrong_nonce.session_nonce = SessionNonce::from_bytes(bytes(8));
        assert_eq!(
            guard.accept(&wrong_nonce),
            Err(ProtocolError::SessionNonceMismatch)
        );
    }

    #[test]
    fn plan_hash_binds_artifact_and_target_identity() {
        let mut plan = sample_plan();
        plan.target.capacity_bytes += 1;
        assert_eq!(plan.validate(), Err(ProtocolError::PlanHashMismatch));

        let mut plan = sample_plan();
        plan.artifact.sha256 = Sha256Digest::from_bytes(bytes(99));
        assert_eq!(plan.validate(), Err(ProtocolError::PlanHashMismatch));
    }

    #[test]
    fn execute_requires_prepare_and_exact_binding() {
        let plan = sample_plan();
        let mut tracker = OperationTracker::new();
        let request = ExecuteRequest {
            binding: plan.binding(),
        };
        assert_eq!(
            tracker.execute(&request),
            Err(ProtocolError::UnknownOperation(plan.operation_id))
        );

        tracker
            .prepare(&PrepareRequest { plan: plan.clone() })
            .unwrap();
        let mut changed = request.clone();
        changed.binding.target.fingerprint = Sha256Digest::from_bytes(bytes(7));
        assert_eq!(
            tracker.execute(&changed),
            Err(ProtocolError::BindingMismatch)
        );
        tracker.execute(&request).unwrap();
        assert_eq!(
            tracker.execute(&request),
            Err(ProtocolError::InvalidTransition {
                from: OperationState::Executing,
                requested: "execute"
            })
        );
    }

    #[test]
    fn rejects_invalid_progress_transitions_and_replay() {
        let plan = sample_plan();
        let mut tracker = OperationTracker::new();
        tracker
            .prepare(&PrepareRequest { plan: plan.clone() })
            .unwrap();
        let event = ProgressEvent {
            operation_id: plan.operation_id,
            event_sequence: 0,
            phase: ProgressPhase::WritingArtifact,
            completed_units: 10,
            total_units: 100,
            disposition: ProgressDisposition::Running,
        };
        assert_eq!(
            tracker.progress(&event),
            Err(ProtocolError::InvalidTransition {
                from: OperationState::Prepared,
                requested: "progress"
            })
        );
        tracker
            .execute(&ExecuteRequest {
                binding: plan.binding(),
            })
            .unwrap();
        tracker.progress(&event).unwrap();
        assert_eq!(
            tracker.progress(&event),
            Err(ProtocolError::UnexpectedProgressSequence {
                received: 0,
                expected: 1
            })
        );
    }

    #[test]
    fn cancel_and_export_follow_terminal_rules() {
        let plan = sample_plan();
        let mut tracker = OperationTracker::new();
        tracker
            .prepare(&PrepareRequest { plan: plan.clone() })
            .unwrap();
        let export = ExportRequest {
            operation_id: plan.operation_id,
            redaction: RedactionLevel::Strict,
            max_bytes: 1024,
        };
        assert_eq!(
            tracker.export(&export),
            Err(ProtocolError::InvalidTransition {
                from: OperationState::Prepared,
                requested: "export"
            })
        );
        tracker
            .cancel(&CancelRequest {
                operation_id: plan.operation_id,
                plan_hash: plan.plan_hash,
            })
            .unwrap();
        tracker.export(&export).unwrap();
        assert_eq!(
            tracker.cancel(&CancelRequest {
                operation_id: plan.operation_id,
                plan_hash: plan.plan_hash,
            }),
            Err(ProtocolError::InvalidTransition {
                from: OperationState::Cancelled,
                requested: "cancel"
            })
        );
    }
}
