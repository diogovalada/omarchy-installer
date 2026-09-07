//! Local-only, privacy-preserving diagnostics.
//!
//! This crate deliberately has no transport, telemetry, or filesystem API.
//! Callers can obtain a JSON byte vector, preview it, and explicitly decide
//! what to do with it. All free-form text passes through [`Sanitizer`], while
//! the rest of the bundle is represented by narrow enums and privacy-coarsened
//! values.

mod model;
mod sanitizer;

pub use model::{
    Architecture, AttributeName, BundleBuilder, CapacityBucket, Component, DeviceKind,
    DiagnosticBundle, DiagnosticError, EventCode, HostOs, OperationKind, OperationOutcome,
    Severity,
};
pub use sanitizer::{RedactionKind, SanitizeError, SanitizedText, Sanitizer};
