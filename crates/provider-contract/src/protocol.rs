use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AuthorizationProof, CancelResult, CapabilityDescriptor, DiagnosticBundle, HostFacts,
    ImmutablePlan, MessageId, OperationId, OperationReceipt, OperationRequest, OperationSnapshot,
    PolicyReference, PreflightFinding, ProgressEvent, ProviderDescriptor, ProviderError,
    RecoveryResult, SchemaVersion,
};

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    pub schema_version: SchemaVersion,
    pub message_id: MessageId,
    pub body: T,
}

impl<T> Envelope<T> {
    pub fn v1(message_id: MessageId, body: T) -> Self {
        Self {
            schema_version: SchemaVersion::V1,
            message_id,
            body,
        }
    }
}

pub type ProviderRequestEnvelope = Envelope<ProviderRequest>;
pub type ProviderResponseEnvelope = Envelope<ProviderResponse>;

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ProviderRequest {
    Describe,
    Probe(ProbeRequest),
    Capabilities(CapabilitiesRequest),
    Preflight(PreflightRequest),
    Plan(PlanRequest),
    Execute(ExecuteRequest),
    Resume(ResumeRequest),
    Recover(RecoverRequest),
    Cancel(CancelRequest),
    Diagnostics(DiagnosticsRequest),
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ProviderResponse {
    Descriptor(ProviderDescriptor),
    Probe(ProbeResponse),
    Capabilities(CapabilitiesResponse),
    Preflight(PreflightResponse),
    Plan(PlanResponse),
    Progress(ProgressEvent),
    Completed(OperationReceipt),
    Resumed(ResumeResponse),
    Recovered(RecoveryResult),
    Cancelled(CancelResult),
    Diagnostics(DiagnosticBundle),
    Error(ProviderError),
}

#[derive(Clone, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeRequest {}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeResponse {
    pub host: HostFacts,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesRequest {
    pub host: HostFacts,
    pub policy: PolicyReference,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesResponse {
    pub capabilities: Vec<CapabilityDescriptor>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightRequest {
    pub operation: OperationRequest,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreflightResponse {
    pub findings: Vec<PreflightFinding>,
    pub may_plan: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub operation: OperationRequest,
    pub policy: PolicyReference,
    pub requested_at_unix_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanResponse {
    pub plan: ImmutablePlan,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteRequest {
    pub plan: ImmutablePlan,
    pub authorization: AuthorizationProof,
    pub requested_at_unix_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeRequest {
    pub operation_id: OperationId,
    pub after_sequence: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeResponse {
    pub operation: OperationSnapshot,
    pub events: Vec<ProgressEvent>,
    pub receipt: Option<OperationReceipt>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverRequest {
    pub operation_id: OperationId,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancelRequest {
    pub operation_id: OperationId,
    pub requested_at_unix_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsRequest {
    pub operation_id: OperationId,
    pub generated_at_unix_seconds: u64,
}
