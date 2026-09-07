use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

use crate::{ImmutablePlan, PlanDigest, PlanError};

/// Digest of a successful operation receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReceiptDigest(pub String);

/// Sanitized failure recorded in the operation journal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    pub code: String,
    pub recoverable: bool,
}

/// Durable operation state. Terminal states reject all later events.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum OperationState {
    Planned,
    AwaitingAuthorization,
    Ready,
    Running,
    Verifying,
    Cancelling,
    RecoveryRequired { failure: Failure },
    Succeeded { receipt_digest: ReceiptDigest },
    Cancelled,
    Failed { failure: Failure },
}

/// Append-only event accepted by [`OperationMachine`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationEvent {
    PlanConfirmed,
    AuthorizationGranted { plan_digest: PlanDigest },
    AuthorizationDenied,
    ExecutionStarted { plan_digest: PlanDigest },
    VerificationStarted,
    Completed { receipt_digest: ReceiptDigest },
    CancelRequested,
    CancellationFinished,
    ExecutionFailed { failure: Failure },
    MarkRecoveryRequired { failure: Failure },
}

/// Pure state machine bound to exactly one immutable plan digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationMachine {
    state: OperationState,
    plan_digest: PlanDigest,
    authorization_required: bool,
    verification_required: bool,
}

impl OperationMachine {
    /// Starts in `Planned`; no authorization or execution has happened.
    ///
    /// # Errors
    /// Returns the plan serialization error if its digest cannot be computed.
    pub fn new(plan: &ImmutablePlan) -> Result<Self, PlanError> {
        Ok(Self {
            state: OperationState::Planned,
            plan_digest: plan.digest()?,
            authorization_required: plan.authorization_required(),
            verification_required: plan.verification_required(),
        })
    }

    #[must_use]
    pub const fn state(&self) -> &OperationState {
        &self.state
    }

    #[must_use]
    pub const fn plan_digest(&self) -> &PlanDigest {
        &self.plan_digest
    }

    /// Applies one event or leaves state unchanged on rejection.
    ///
    /// # Errors
    /// Rejects events unavailable in the current state or bound to another plan digest.
    pub fn apply(&mut self, event: OperationEvent) -> Result<(), TransitionError> {
        let next = match (&self.state, event) {
            (OperationState::Planned, OperationEvent::PlanConfirmed) => {
                if self.authorization_required {
                    OperationState::AwaitingAuthorization
                } else {
                    OperationState::Ready
                }
            }
            (
                OperationState::AwaitingAuthorization,
                OperationEvent::AuthorizationGranted { plan_digest },
            ) => {
                self.verify_digest(&plan_digest)?;
                OperationState::Ready
            }
            (OperationState::AwaitingAuthorization, OperationEvent::AuthorizationDenied)
            | (OperationState::Cancelling, OperationEvent::CancellationFinished) => {
                OperationState::Cancelled
            }
            (OperationState::Ready, OperationEvent::ExecutionStarted { plan_digest }) => {
                self.verify_digest(&plan_digest)?;
                OperationState::Running
            }
            (OperationState::Running, OperationEvent::VerificationStarted)
                if self.verification_required =>
            {
                OperationState::Verifying
            }
            (OperationState::Running, OperationEvent::Completed { receipt_digest })
                if !self.verification_required =>
            {
                OperationState::Succeeded { receipt_digest }
            }
            (OperationState::Verifying, OperationEvent::Completed { receipt_digest }) => {
                OperationState::Succeeded { receipt_digest }
            }
            (
                OperationState::Ready | OperationState::Running | OperationState::Verifying,
                OperationEvent::CancelRequested,
            ) => OperationState::Cancelling,
            (
                OperationState::Ready
                | OperationState::Running
                | OperationState::Verifying
                | OperationState::Cancelling,
                OperationEvent::ExecutionFailed { failure },
            ) => OperationState::Failed { failure },
            (
                OperationState::Running | OperationState::Verifying | OperationState::Cancelling,
                OperationEvent::MarkRecoveryRequired { failure },
            ) => OperationState::RecoveryRequired { failure },
            _ => return Err(TransitionError::IllegalTransition),
        };
        self.state = next;
        Ok(())
    }

    fn verify_digest(&self, supplied: &PlanDigest) -> Result<(), TransitionError> {
        if supplied == &self.plan_digest {
            Ok(())
        } else {
            Err(TransitionError::PlanDigestMismatch)
        }
    }
}

/// Rejected state transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionError {
    IllegalTransition,
    PlanDigestMismatch,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IllegalTransition => formatter.write_str("illegal operation-state transition"),
            Self::PlanDigestMismatch => formatter.write_str("operation plan digest mismatch"),
        }
    }
}

impl Error for TransitionError {}
