//! Pure, side-effect-free safety model for Omarchy Setup.
//!
//! This crate never probes hardware, opens files, or touches a block device. It
//! accepts observations made by less-trusted adapters, evaluates explicit
//! policy grants, derives immutable plans, and guards operation transitions.

#![forbid(unsafe_code)]

mod capability;
mod host;
mod plan;
mod policy;
mod state;

pub use capability::{Capability, CapabilityKind, CapabilityRequest, SupportLevel};
pub use host::{Architecture, ElevationKind, Fact, HostBinding, HostFacts, HostOs};
pub use plan::{
    ArtifactRef, CanonicalPlan, DirectInstallMode, ImmutablePlan, ManageAction, OperationId,
    OperationIntent, PlanDigest, PlanError, PlanRequest, PlanStep, ProviderBinding, TargetIdentity,
    plan,
};
pub use policy::{
    CapabilityGrant, PolicyDecision, PolicyViolation, SafetyPolicy, evaluate_capability,
};
pub use state::{
    Failure, OperationEvent, OperationMachine, OperationState, ReceiptDigest, TransitionError,
};
