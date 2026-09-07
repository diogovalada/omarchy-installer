use serde::{Deserialize, Serialize};

use crate::{
    Architecture, Capability, CapabilityKind, CapabilityRequest, ElevationKind, HostFacts, HostOs,
    SupportLevel,
};

/// One exact allow rule from authenticated policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityGrant {
    pub kind: CapabilityKind,
    pub os: HostOs,
    pub architecture: Architecture,
    /// Required for provider operations; must be absent for built-in operations.
    pub provider_id: Option<String>,
    pub support: SupportLevel,
}

/// Authenticated policy input. Absence of a grant means denial.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyPolicy {
    pub revision: u64,
    pub grants: Vec<CapabilityGrant>,
    /// Emergency kill switches override every grant.
    pub disabled: Vec<CapabilityKind>,
    /// Whether this build is allowed to expose experimental grants.
    pub allow_experimental: bool,
}

/// A machine-readable failure-closed denial reason.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyViolation {
    UnknownHostOs,
    UnknownArchitecture,
    UnknownElevation,
    ElevationUnavailable,
    UnknownVirtualization,
    VirtualizationUnavailable,
    EmergencyDisabled,
    ExperimentalDisabled,
    MissingGrant,
    AmbiguousGrant,
    InvalidProviderIdentity,
}

/// Result of applying authenticated policy to observed host facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyDecision {
    Allowed(Capability),
    Denied(PolicyViolation),
}

/// Evaluates one capability. Every unknown or ambiguous safety input is denied.
#[must_use]
pub fn evaluate_capability(
    facts: &HostFacts,
    policy: &SafetyPolicy,
    request: CapabilityRequest<'_>,
) -> PolicyDecision {
    let Some(os) = facts.os.known().copied() else {
        return PolicyDecision::Denied(PolicyViolation::UnknownHostOs);
    };
    let Some(architecture) = facts.architecture.known().copied() else {
        return PolicyDecision::Denied(PolicyViolation::UnknownArchitecture);
    };

    if policy.disabled.contains(&request.kind) {
        return PolicyDecision::Denied(PolicyViolation::EmergencyDisabled);
    }

    if request.kind.requires_elevation() {
        let Some(elevation) = facts.elevation.known().copied() else {
            return PolicyDecision::Denied(PolicyViolation::UnknownElevation);
        };
        if elevation == ElevationKind::Unavailable {
            return PolicyDecision::Denied(PolicyViolation::ElevationUnavailable);
        }
    }

    if request.kind == CapabilityKind::TryVm {
        let Some(available) = facts.virtualization_available.known().copied() else {
            return PolicyDecision::Denied(PolicyViolation::UnknownVirtualization);
        };
        if !available {
            return PolicyDecision::Denied(PolicyViolation::VirtualizationUnavailable);
        }
    }

    if request.provider_id.is_some_and(|id| !valid_identifier(id)) {
        return PolicyDecision::Denied(PolicyViolation::InvalidProviderIdentity);
    }

    let matching: Vec<_> = policy
        .grants
        .iter()
        .filter(|grant| {
            grant.kind == request.kind
                && grant.os == os
                && grant.architecture == architecture
                && grant.provider_id.as_deref() == request.provider_id
        })
        .collect();

    let [grant] = matching.as_slice() else {
        return PolicyDecision::Denied(if matching.is_empty() {
            PolicyViolation::MissingGrant
        } else {
            PolicyViolation::AmbiguousGrant
        });
    };

    if grant.support == SupportLevel::Experimental && !policy.allow_experimental {
        return PolicyDecision::Denied(PolicyViolation::ExperimentalDisabled);
    }

    PolicyDecision::Allowed(Capability {
        kind: grant.kind,
        support: grant.support,
        provider_id: grant.provider_id.clone(),
        policy_revision: policy.revision,
    })
}

pub(crate) fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
}
