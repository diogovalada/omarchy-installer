use serde::{Deserialize, Serialize};

/// A probed fact that may be unavailable or unsafe to infer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status", content = "value")]
pub enum Fact<T> {
    /// The probe established the value.
    Known(T),
    /// The probe could not establish the value.
    Unknown,
}

impl<T> Fact<T> {
    /// Returns a reference only when the fact is known.
    #[must_use]
    pub const fn known(&self) -> Option<&T> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown => None,
        }
    }
}

/// Host operating-system family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostOs {
    Windows,
    MacOs,
    Linux,
}

/// Host processor architecture.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X86_64,
    Aarch64,
}

/// Native elevation mechanism available to the ordinary-user GUI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElevationKind {
    WindowsUac,
    MacOsAuthorization,
    LinuxPolkit,
    Unavailable,
}

/// Read-only observations collected by a host adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostFacts {
    pub os: Fact<HostOs>,
    pub architecture: Fact<Architecture>,
    pub elevation: Fact<ElevationKind>,
    pub virtualization_available: Fact<bool>,
}

/// Exact, non-secret host identity bound into a plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostBinding {
    pub os: HostOs,
    pub architecture: Architecture,
    /// A stable SHA-256 identifier produced by the host adapter.
    pub fingerprint_sha256: String,
}

impl HostBinding {
    pub(crate) fn validate(&self) -> bool {
        is_lower_hex_sha256(&self.fingerprint_sha256)
    }
}

pub(crate) fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
