use crate::{SanitizeError, SanitizedText, Sanitizer};
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

const SCHEMA_VERSION: u32 = 1;
const MAX_EVENTS: usize = 2_000;
const MAX_DEVICES: usize = 64;
const MAX_ATTRIBUTES: usize = 64;

#[derive(Debug, Error)]
pub enum DiagnosticError {
    #[error(transparent)]
    Sanitize(#[from] SanitizeError),
    #[error("invalid event code; use 1-64 ASCII letters, digits, '.', '_' or '-'")]
    InvalidEventCode,
    #[error("unknown diagnostic attribute '{0}'; arbitrary fields are not accepted")]
    UnknownAttribute(String),
    #[error("diagnostic attribute '{0}' is forbidden because it can identify a person or device")]
    ForbiddenAttribute(String),
    #[error("diagnostic collection limit exceeded for {0}")]
    CollectionLimit(&'static str),
    #[error("JSON serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostOs {
    Windows,
    MacOs,
    Linux,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Architecture {
    X86_64,
    Aarch64,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationKind {
    Download,
    Verify,
    WriteUsb,
    TryVirtualMachine,
    DirectInstall,
    Repair,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationOutcome {
    Running,
    Succeeded,
    Cancelled,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Debug,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Component {
    App,
    Catalog,
    Downloader,
    DevicePolicy,
    MediaWriter,
    Provider,
    PrivilegedHelper,
    Journal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceKind {
    Usb,
    SdCard,
    ExternalDrive,
    InternalDrive,
    Virtual,
    Unknown,
}

/// Coarse capacity avoids exporting an unusually exact disk size as a hardware
/// fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapacityBucket {
    Under8GiB,
    From8To15GiB,
    From16To31GiB,
    From32To63GiB,
    From64To127GiB,
    From128To255GiB,
    From256To511GiB,
    From512GiBTo1TiB,
    Over1TiB,
    Unknown,
}

impl CapacityBucket {
    pub fn from_bytes(bytes: u64) -> Self {
        const GIB: u64 = 1024 * 1024 * 1024;
        match bytes / GIB {
            0..=7 => Self::Under8GiB,
            8..=15 => Self::From8To15GiB,
            16..=31 => Self::From16To31GiB,
            32..=63 => Self::From32To63GiB,
            64..=127 => Self::From64To127GiB,
            128..=255 => Self::From128To255GiB,
            256..=511 => Self::From256To511GiB,
            512..=1024 => Self::From512GiBTo1TiB,
            _ => Self::Over1TiB,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EventCode(String);

impl EventCode {
    pub fn new(code: impl Into<String>) -> Result<Self, DiagnosticError> {
        let code = code.into();
        let valid = !code.is_empty()
            && code.len() <= 64
            && code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
        if !valid {
            return Err(DiagnosticError::InvalidEventCode);
        }
        Ok(Self(code))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttributeName {
    Provider,
    FailureStage,
    ImageVariant,
    Transport,
    BootMode,
    Filesystem,
}

impl AttributeName {
    fn parse(input: &str) -> Result<Self, DiagnosticError> {
        let normalized = input.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        match normalized.as_str() {
            "provider" => Ok(Self::Provider),
            "failure_stage" => Ok(Self::FailureStage),
            "image_variant" => Ok(Self::ImageVariant),
            "transport" => Ok(Self::Transport),
            "boot_mode" => Ok(Self::BootMode),
            "filesystem" => Ok(Self::Filesystem),
            "username" | "user" | "account" | "home" | "home_path" | "path" | "disk_serial"
            | "serial" | "wwn" | "recovery_key" | "ip" | "ip_address" | "mac" | "mac_address"
            | "email" | "token" | "password" | "secret" => {
                Err(DiagnosticError::ForbiddenAttribute(input.to_owned()))
            }
            _ => Err(DiagnosticError::UnknownAttribute(input.to_owned())),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticBundle {
    schema_version: u32,
    run_id: Uuid,
    application: ApplicationSummary,
    host: HostSummary,
    operation: OperationSummary,
    devices: Vec<DeviceSummary>,
    attributes: Vec<Attribute>,
    events: Vec<DiagnosticEvent>,
    privacy: PrivacySummary,
}

impl DiagnosticBundle {
    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    /// Serialize only to caller-owned memory. This crate intentionally exposes
    /// no save, upload, telemetry, HTTP, or background-reporting capability.
    pub fn to_json_vec(&self) -> Result<Vec<u8>, DiagnosticError> {
        Ok(serde_json::to_vec_pretty(self)?)
    }

    pub fn to_json_string(&self) -> Result<String, DiagnosticError> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationSummary {
    version: SanitizedText,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostSummary {
    os: HostOs,
    os_version: SanitizedText,
    architecture: Architecture,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationSummary {
    kind: OperationKind,
    outcome: OperationOutcome,
    elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceSummary {
    /// Random for this bundle; never derived from a disk serial, OS path, GUID,
    /// WWN, or other stable hardware identifier.
    diagnostic_id: Uuid,
    kind: DeviceKind,
    removable: bool,
    capacity: CapacityBucket,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Attribute {
    name: AttributeName,
    value: SanitizedText,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticEvent {
    elapsed_ms: u64,
    severity: Severity,
    component: Component,
    code: EventCode,
    message: SanitizedText,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrivacySummary {
    local_only: bool,
    redacted_value_count: usize,
    exact_device_identifiers_included: bool,
}

pub struct BundleBuilder {
    run_id: Uuid,
    sanitizer: Sanitizer,
    application: ApplicationSummary,
    host: HostSummary,
    operation: OperationSummary,
    devices: Vec<DeviceSummary>,
    attributes: Vec<Attribute>,
    events: Vec<DiagnosticEvent>,
}

impl BundleBuilder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mut sanitizer: Sanitizer,
        application_version: impl AsRef<str>,
        os: HostOs,
        os_version: impl AsRef<str>,
        architecture: Architecture,
        operation_kind: OperationKind,
        operation_outcome: OperationOutcome,
        elapsed_ms: u64,
    ) -> Result<Self, DiagnosticError> {
        let application_version = sanitizer.sanitize(application_version)?;
        let os_version = sanitizer.sanitize(os_version)?;
        Ok(Self {
            run_id: Uuid::new_v4(),
            sanitizer,
            application: ApplicationSummary {
                version: application_version,
            },
            host: HostSummary {
                os,
                os_version,
                architecture,
            },
            operation: OperationSummary {
                kind: operation_kind,
                outcome: operation_outcome,
                elapsed_ms,
            },
            devices: Vec::new(),
            attributes: Vec::new(),
            events: Vec::new(),
        })
    }

    /// Add a device without accepting any stable identifier, path, label,
    /// vendor, or model. The returned ID is random and scoped to this bundle.
    pub fn add_device(
        &mut self,
        kind: DeviceKind,
        removable: bool,
        capacity: CapacityBucket,
    ) -> Result<Uuid, DiagnosticError> {
        if self.devices.len() >= MAX_DEVICES {
            return Err(DiagnosticError::CollectionLimit("devices"));
        }
        let diagnostic_id = Uuid::new_v4();
        self.devices.push(DeviceSummary {
            diagnostic_id,
            kind,
            removable,
            capacity,
        });
        Ok(diagnostic_id)
    }

    pub fn add_attribute(
        &mut self,
        name: AttributeName,
        value: impl AsRef<str>,
    ) -> Result<(), DiagnosticError> {
        if self.attributes.len() >= MAX_ATTRIBUTES {
            return Err(DiagnosticError::CollectionLimit("attributes"));
        }
        let value = self.sanitizer.sanitize(value)?;
        self.attributes.push(Attribute { name, value });
        Ok(())
    }

    /// Boundary for callers receiving string field names. Only the explicit
    /// allowlist is accepted; identity/device fields get a distinct rejection.
    pub fn try_add_named_attribute(
        &mut self,
        name: &str,
        value: impl AsRef<str>,
    ) -> Result<(), DiagnosticError> {
        self.add_attribute(AttributeName::parse(name)?, value)
    }

    pub fn add_event(
        &mut self,
        elapsed_ms: u64,
        severity: Severity,
        component: Component,
        code: EventCode,
        message: impl AsRef<str>,
    ) -> Result<(), DiagnosticError> {
        if self.events.len() >= MAX_EVENTS {
            return Err(DiagnosticError::CollectionLimit("events"));
        }
        let message = self.sanitizer.sanitize(message)?;
        self.events.push(DiagnosticEvent {
            elapsed_ms,
            severity,
            component,
            code,
            message,
        });
        Ok(())
    }

    pub fn finish(self) -> DiagnosticBundle {
        DiagnosticBundle {
            schema_version: SCHEMA_VERSION,
            run_id: self.run_id,
            application: self.application,
            host: self.host,
            operation: self.operation,
            devices: self.devices,
            attributes: self.attributes,
            events: self.events,
            privacy: PrivacySummary {
                local_only: true,
                redacted_value_count: self.sanitizer.redaction_count(),
                exact_device_identifiers_included: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RedactionKind;

    fn builder() -> BundleBuilder {
        BundleBuilder::new(
            Sanitizer::new(),
            "0.1.0-dev",
            HostOs::Windows,
            "11 24H2",
            Architecture::X86_64,
            OperationKind::WriteUsb,
            OperationOutcome::Failed,
            1234,
        )
        .unwrap()
    }

    #[test]
    fn run_and_device_ids_are_random_and_not_stable() {
        let first = builder().finish();
        let second = builder().finish();
        assert_ne!(first.run_id(), second.run_id());

        let mut builder = builder();
        let first_device = builder
            .add_device(DeviceKind::Usb, true, CapacityBucket::From32To63GiB)
            .unwrap();
        let second_device = builder
            .add_device(DeviceKind::Usb, true, CapacityBucket::From32To63GiB)
            .unwrap();
        assert_ne!(first_device, second_device);
    }

    #[test]
    fn rejects_identity_device_and_unknown_structured_fields() {
        let mut builder = builder();
        assert!(matches!(
            builder.try_add_named_attribute("disk_serial", "ABC123"),
            Err(DiagnosticError::ForbiddenAttribute(_))
        ));
        assert!(matches!(
            builder.try_add_named_attribute("username", "alice"),
            Err(DiagnosticError::ForbiddenAttribute(_))
        ));
        assert!(matches!(
            builder.try_add_named_attribute("surprise", "value"),
            Err(DiagnosticError::UnknownAttribute(_))
        ));
    }

    #[test]
    fn exports_sensitive_fixture_only_to_redacted_memory_json() {
        let mut sanitizer = Sanitizer::new();
        sanitizer
            .add_sensitive_literal("privateuser", RedactionKind::Identity)
            .unwrap();
        let mut builder = BundleBuilder::new(
            sanitizer,
            "0.1.0",
            HostOs::Linux,
            "Example Linux",
            Architecture::X86_64,
            OperationKind::WriteUsb,
            OperationOutcome::Failed,
            9000,
        )
        .unwrap();
        builder
            .add_event(
                9000,
                Severity::Error,
                Component::MediaWriter,
                EventCode::new("media.write.failed").unwrap(),
                "privateuser failed at /home/privateuser/image.iso; serial=SN-SECRET; peer=10.2.3.4",
            )
            .unwrap();
        builder
            .add_attribute(AttributeName::FailureStage, "write at /dev/sdb")
            .unwrap();

        let json = builder.finish().to_json_vec().unwrap();
        let output = String::from_utf8(json).unwrap();
        for forbidden in ["privateuser", "/home/", "SN-SECRET", "10.2.3.4", "/dev/sdb"] {
            assert!(!output.contains(forbidden), "leaked {forbidden}: {output}");
        }
        assert!(output.contains("\"localOnly\": true"));
        assert!(output.contains("\"exactDeviceIdentifiersIncluded\": false"));
        serde_json::from_str::<serde_json::Value>(&output).unwrap();
    }

    #[test]
    fn exact_sizes_are_coarsened() {
        let one_gib = 1024 * 1024 * 1024;
        assert_eq!(
            CapacityBucket::from_bytes(63 * one_gib + 12345),
            CapacityBucket::From32To63GiB
        );
    }
}
