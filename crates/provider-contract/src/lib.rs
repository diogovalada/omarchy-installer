//! Stable, data-only contracts between Omarchy Installer and capability providers.
//!
//! This crate intentionally contains no provider discovery, dynamic loading, shell
//! invocation, or process execution. A host application chooses the transport and
//! translates these DTOs at its trust boundary.

#![forbid(unsafe_code)]

mod lifecycle;
mod model;
mod protocol;
mod simulator;

pub use lifecycle::{ProgressSink, ProviderLifecycle};
pub use model::*;
pub use protocol::*;
pub use simulator::{SimulatedProvider, SimulatorScenario};

/// The only protocol revision understood by this crate.
pub const PROVIDER_PROTOCOL_V1: u16 = 1;
