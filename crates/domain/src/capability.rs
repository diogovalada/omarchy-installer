use serde::{Deserialize, Serialize};

/// An operation family exposed by the product.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Download,
    CreateUsb,
    TryVm,
    DirectInstallAlongside,
    DirectInstallReplace,
    Resume,
    Repair,
    Remove,
    ExportDiagnostics,
}

/// Release qualification attached to an explicit policy grant.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Stable,
    Experimental,
}

/// A capability query. Provider-backed operations include the exact provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityRequest<'a> {
    pub kind: CapabilityKind,
    pub provider_id: Option<&'a str>,
}

/// A capability made available by policy on this host.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub kind: CapabilityKind,
    pub support: SupportLevel,
    pub provider_id: Option<String>,
    pub policy_revision: u64,
}

impl CapabilityKind {
    /// Whether this capability must cross a privilege boundary.
    #[must_use]
    pub const fn requires_elevation(self) -> bool {
        matches!(
            self,
            Self::CreateUsb
                | Self::DirectInstallAlongside
                | Self::DirectInstallReplace
                | Self::Repair
                | Self::Remove
        )
    }
}
