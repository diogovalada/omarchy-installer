use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, Write};
use std::path::PathBuf;

pub const MAX_FRAME: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootDefault {
    Omarchy,
    Windows,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootMenu {
    pub default_os: BootDefault,
    pub timeout_seconds: u32,
}
impl Default for BootMenu {
    fn default() -> Self {
        Self {
            default_os: BootDefault::Omarchy,
            timeout_seconds: 5,
        }
    }
}
impl BootMenu {
    pub fn validate(&self) -> Result<(), String> {
        if ![5, 10, 15, 30].contains(&self.timeout_seconds) {
            return Err("Choose a boot menu countdown of 5, 10, 15 or 30 seconds".into());
        }
        Ok(())
    }
    pub fn summary(&self) -> String {
        let name = match self.default_os {
            BootDefault::Omarchy => "Omarchy",
            BootDefault::Windows => "Windows",
        };
        format!("Show the Omarchy / Windows boot menu at normal startup. Start {name} automatically after {} seconds. Choosing Windows uses its existing firmware entry and briefly restarts the PC. A one-time choice does not change the saved default.", self.timeout_seconds)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceImage {
    pub path: PathBuf,
    pub file_name: String,
    pub length: u64,
    pub sha256: String,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "target_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DirectTarget {
    Free {
        start_offset_bytes: u64,
    },
    Shrink {
        partition_number: u32,
        partition_guid: String,
    },
    Delete {
        partition_number: u32,
        partition_guid: String,
        start_offset_bytes: u64,
        partition_size_bytes: u64,
        confirmation: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Destination {
    InspectDirect,
    PrepareFirmware,
    PrepareRuntime,
    Usb {
        identity: Value,
    },
    InspectUsb {
        identity: Value,
    },
    UsbPreserve {
        identity: Value,
        plan: crate::usb_preserve::Plan,
    },
    DirectX86 {
        disk_number: u32,
        disk_unique_id: String,
        target: DirectTarget,
        allocation_bytes: u64,
        boot_menu: BootMenu,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationRequest {
    pub protocol: u32,
    pub source: SourceImage,
    pub destination: Destination,
}

// Shared binding for the in-app USB review and the helper's final write gate.
pub fn usb_confirmation_plan(request: &OperationRequest) -> Result<Option<Value>, String> {
    Ok(match &request.destination {
        Destination::Usb { identity } => {
            let sector = identity["blockSize"]
                .as_u64()
                .filter(|n| (512..=4096).contains(n) && n.is_power_of_two())
                .ok_or("USB physical sector size is unavailable")?;
            let padding = (sector - request.source.length % sector) % sector;
            let span = request
                .source
                .length
                .checked_add(padding)
                .ok_or("USB image span overflow")?;
            if identity["size"]
                .as_u64()
                .is_none_or(|capacity| span > capacity)
            {
                return Err("The USB cannot hold the installer".into());
            }
            Some(
                serde_json::json!({"kind":"usb","target":identity,"sourceSha256":request.source.sha256,"length":request.source.length,"writeSpanBytes":span,"paddingBytes":padding}),
            )
        }
        Destination::UsbPreserve { identity, plan } => Some(
            serde_json::json!({"kind":"usb_preserve","target":identity,"plan":plan,"sourceSha256":request.source.sha256}),
        ),
        _ => None,
    })
}

/// A successful child exit is necessary, but the completion record must also
/// prove that this exact approved image and target were written and verified.
pub fn validate_usb_receipt(request: &OperationRequest, receipt: &Value) -> Result<(), String> {
    let Destination::Usb { identity } = &request.destination else {
        return Err("A USB receipt requires a USB write request".into());
    };
    let plan = usb_confirmation_plan(request)?.ok_or("Missing USB approval plan")?;
    let valid = request.source.length > 0
        && receipt["engine"] == "etcher-sdk"
        && receipt["engineVersion"] == "10.2.14"
        && receipt["destinationKind"] == "usb-whole-device"
        && &receipt["target"] == identity
        && receipt["sha256"]
            .as_str()
            .is_some_and(|digest| digest.eq_ignore_ascii_case(&request.source.sha256))
        && ["bytesWritten", "readbackBytes"]
            .iter()
            .all(|key| receipt[key].as_u64() == Some(request.source.length))
        && ["writeSpanBytes", "readbackSpanBytes"]
            .iter()
            .all(|key| receipt[key] == plan["writeSpanBytes"])
        && receipt["paddingBytes"] == plan["paddingBytes"]
        && [
            "sdkVerification",
            "flushed",
            "fullReadbackVerification",
            "paddingVerification",
        ]
        .iter()
        .all(|key| receipt[key] == true)
        && matches!(
            receipt["eject"]["status"].as_str(),
            Some("ejected" | "unmounted" | "failed")
        )
        && receipt["eject"]["message"]
            .as_str()
            .is_some_and(|message| !message.trim().is_empty());
    if !valid {
        return Err("The USB provider returned an incomplete or inconsistent verification receipt. Treat the USB as unverified and retry.".into());
    }
    Ok(())
}

#[cfg(test)]
mod usb_receipt_tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> (OperationRequest, Value) {
        let identity = json!({"blockSize":512,"size":8192,"fingerprint":"selected-usb"});
        let request = OperationRequest {
            protocol: 1,
            source: SourceImage {
                path: "fixture.iso".into(),
                file_name: "fixture.iso".into(),
                length: 1025,
                sha256: "a".repeat(64),
                signature: vec![],
            },
            destination: Destination::Usb {
                identity: identity.clone(),
            },
        };
        let receipt = json!({"engine":"etcher-sdk","engineVersion":"10.2.14",
            "destinationKind":"usb-whole-device","target":identity,"sha256":"a".repeat(64),
            "bytesWritten":1025,"writeSpanBytes":1536,"paddingBytes":511,
            "readbackBytes":1025,"readbackSpanBytes":1536,"sdkVerification":true,
            "flushed":true,"fullReadbackVerification":true,"paddingVerification":true,
            "eject":{"status":"ejected","message":"Device removed."}});
        (request, receipt)
    }

    #[test]
    fn accepts_complete_bound_receipt_including_manual_ejection() {
        let (request, mut receipt) = fixture();
        validate_usb_receipt(&request, &receipt).unwrap();
        receipt["eject"] = json!({"status":"failed","message":"Use safely remove."});
        validate_usb_receipt(&request, &receipt).unwrap();
    }

    #[test]
    fn rejects_missing_checks_or_changed_image_target_and_byte_counts() {
        let (request, receipt) = fixture();
        for field in receipt.as_object().unwrap().keys() {
            let mut missing = receipt.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                validate_usb_receipt(&request, &missing).is_err(),
                "missing {field}"
            );
        }
        for field in [
            "sdkVerification",
            "flushed",
            "fullReadbackVerification",
            "paddingVerification",
        ] {
            let mut changed = receipt.clone();
            changed[field] = json!(false);
            assert!(
                validate_usb_receipt(&request, &changed).is_err(),
                "false {field}"
            );
        }
        for field in [
            "bytesWritten",
            "writeSpanBytes",
            "readbackBytes",
            "readbackSpanBytes",
            "paddingBytes",
        ] {
            let mut changed = receipt.clone();
            changed[field] = json!(0);
            assert!(
                validate_usb_receipt(&request, &changed).is_err(),
                "wrong {field}"
            );
        }
        let mut changed = receipt.clone();
        changed["target"]["fingerprint"] = json!("another-usb");
        assert!(validate_usb_receipt(&request, &changed).is_err());
        let mut changed = receipt.clone();
        changed["sha256"] = json!("b".repeat(64));
        assert!(validate_usb_receipt(&request, &changed).is_err());
        let mut changed = receipt;
        changed["eject"]["status"] = json!("unknown");
        assert!(validate_usb_receipt(&request, &changed).is_err());
    }
}

pub fn read_frame(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf().map_err(|e| e.to_string())?;
        if buffer.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err("Incomplete provider frame".into())
            };
        }
        let end = buffer.iter().position(|b| *b == b'\n');
        let take = end.map_or(buffer.len(), |end| end + 1);
        if bytes.len() + take > MAX_FRAME {
            return Err("Provider frame exceeds the size limit".into());
        }
        bytes.extend_from_slice(&buffer[..take]);
        reader.consume(take);
        if end.is_some() {
            if bytes.iter().all(u8::is_ascii_whitespace) {
                bytes.clear();
                continue;
            }
            return serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| format!("Invalid provider message: {e}"));
        }
    }
}

pub fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() >= MAX_FRAME {
        return Err("Provider frame exceeds the size limit".into());
    }
    bytes.push(b'\n');
    writer.write_all(&bytes).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())
}
