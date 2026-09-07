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
        format!("Boot menu: start {name} after {} seconds. Choosing Windows includes a brief extra restart.", self.timeout_seconds)
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

/// Bind the provider's plan to the requested disk, space and startup settings
/// before presenting its final confirmation. Presentation never defines scope.
pub fn validate_direct_plan(
    request: &OperationRequest,
    operation_id: &str,
    built: &Value,
    plan: &Value,
) -> Result<(), String> {
    let Destination::DirectX86 {
        disk_number,
        disk_unique_id,
        target,
        allocation_bytes,
        boot_menu,
    } = &request.destination
    else {
        return Err("A Windows installation plan requires a direct installation request".into());
    };
    let target_kind = match target {
        DirectTarget::Free { .. } => "free",
        DirectTarget::Shrink { .. } => "shrink",
        DirectTarget::Delete { .. } => "delete",
    };
    let digest = |value: &Value| {
        value.as_str().is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    };
    let partitions = plan["partitions"]
        .as_array()
        .filter(|p| p.len() == 2)
        .ok_or("The installation plan omitted its exact partitions")?;
    let start = partitions[0]["offsetBytes"]
        .as_u64()
        .ok_or("The plan omitted a partition offset")?;
    let root_start = start
        .checked_add(2_147_483_648)
        .ok_or("The installation plan exceeds disk bounds")?;
    let end = start
        .checked_add(*allocation_bytes)
        .ok_or("The installation plan exceeds disk bounds")?;
    let root_size = allocation_bytes
        .checked_sub(2_147_483_648)
        .filter(|size| *size >= 40_800_092_160)
        .ok_or("The installation allocation is too small")?;
    let valid = plan["schemaVersion"] == 2
        && plan["kind"] == "omarchy-windows-alongside-plan"
        && plan["operationId"] == operation_id
        && plan["diskNumber"].as_u64() == Some(u64::from(*disk_number))
        && plan["diskUniqueId"].as_str() == Some(disk_unique_id.as_str())
        && plan["targetKind"] == target_kind
        && plan["allocationBytes"].as_u64() == Some(*allocation_bytes)
        && allocation_bytes % 1_048_576 == 0
        && plan["manifestSha256"] == built["manifestSha256"]
        && digest(&plan["manifestSha256"])
        && digest(&plan["planSha256"])
        && plan["bootMenu"] == serde_json::json!(boot_menu)
        && plan["bootPolicy"] == "menu-first-preserve-existing-entries"
        && plan["encryption"] == "luks2"
        && plan["protectionState"] == "owner-setup-required"
        && plan["reboot"] == false
        && start >= 1_048_576
        && start % 1_048_576 == 0
        && plan["diskSizeBytes"]
            .as_u64()
            .is_some_and(|size| end <= size)
        && partitions[0]["role"] == "esp"
        && partitions[0]["sizeBytes"] == 2_147_483_648_u64
        && partitions[0]["imageSizeBytes"] == 2_147_483_648_u64
        && partitions[0]["file"] == "esp.img.enc"
        && partitions[1]["role"] == "root"
        && partitions[1]["sizeBytes"].as_u64() == Some(root_size)
        && partitions[1]["imageSizeBytes"] == 40_800_092_160_u64
        && partitions[1]["offsetBytes"].as_u64() == Some(root_start)
        && partitions[1]["file"] == "root.img.enc"
        && partitions.iter().all(|p| digest(&p["sha256"]))
        && partitions.iter().all(|p| {
            p["partitionGuid"]
                .as_str()
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .is_some_and(|id| !id.is_nil())
        })
        && uuid::Uuid::parse_str(partitions[0]["partitionGuid"].as_str().unwrap_or_default()).ok()
            != uuid::Uuid::parse_str(partitions[1]["partitionGuid"].as_str().unwrap_or_default())
                .ok();
    let free_matches = match target {
        DirectTarget::Free { start_offset_bytes } => {
            start == *start_offset_bytes && plan["shrink"].is_null() && plan["delete"].is_null()
        }
        DirectTarget::Shrink { .. } => plan["delete"].is_null(),
        DirectTarget::Delete { .. } => plan["shrink"].is_null(),
    };
    if !valid || !free_matches {
        return Err("The provider changed the reviewed Windows installation plan".into());
    }
    Ok(())
}

/// Never display installation success for a partial or unrelated completion.
pub fn validate_direct_receipt(plan: &Value, receipt: &Value) -> Result<(), String> {
    let matches = [
        "operationId",
        "planSha256",
        "manifestSha256",
        "diskUniqueId",
        "targetKind",
        "allocationBytes",
        "shrink",
        "delete",
        "partitions",
        "bootMenu",
        "bootPolicy",
        "encryption",
        "protectionState",
    ]
    .iter()
    .all(|key| receipt[*key] == plan[*key]);
    let suspend = plan["bitLocker"]["suspendVolumes"]
        .as_array()
        .ok_or("The plan omitted its BitLocker changes")?;
    let restored = &receipt["bitLockerRestoration"];
    let boot_registered = receipt["bootEntry"].as_str().is_some_and(|entry| {
        entry.len() == 8
            && entry.starts_with("Boot")
            && entry[4..].bytes().all(|b| b.is_ascii_hexdigit())
    });
    let required = !suspend.is_empty();
    let restored_volumes = restored["volumes"].as_array().is_some_and(|volumes| {
        volumes.len() == suspend.len()
            && suspend.iter().all(|expected| {
                let Some(id) = expected["volumeId"].as_str() else {
                    return false;
                };
                volumes
                    .iter()
                    .filter(|actual| actual["volumeId"].as_str() == Some(id))
                    .count()
                    == 1
            })
    });
    if !matches
        || receipt["schemaVersion"] != 2
        || receipt["status"] != "deployed"
        || receipt["readbackVerified"] != true
        || receipt["rebooted"] != false
        || !boot_registered
        || restored["required"] != required
        || restored["verified"] != true
        || !restored_volumes
    {
        return Err("The Windows installation receipt did not verify the approved disk, boot menu and encryption state".into());
    }
    Ok(())
}

#[cfg(test)]
mod direct_receipt_tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> (OperationRequest, Value, Value, Value) {
        let allocation = 80 * 1_073_741_824_u64;
        let request = OperationRequest {
            protocol: 1,
            source: SourceImage {
                path: "fixture.iso".into(),
                file_name: "fixture.iso".into(),
                length: 1000,
                sha256: "a".repeat(64),
                signature: vec![],
            },
            destination: Destination::DirectX86 {
                disk_number: 2,
                disk_unique_id: "disk-two".into(),
                target: DirectTarget::Free {
                    start_offset_bytes: 1_048_576,
                },
                allocation_bytes: allocation,
                boot_menu: BootMenu::default(),
            },
        };
        let built = json!({"manifestSha256":"b".repeat(64)});
        let plan = json!({"schemaVersion":2,"kind":"omarchy-windows-alongside-plan","operationId":"test-operation",
            "diskNumber":2,"diskUniqueId":"disk-two","diskSizeBytes":200*1_073_741_824_u64,
            "targetKind":"free","allocationBytes":allocation,"manifestSha256":built["manifestSha256"],"planSha256":"c".repeat(64),
            "bootMenu":BootMenu::default(),"bootPolicy":"menu-first-preserve-existing-entries",
            "encryption":"luks2","protectionState":"owner-setup-required","reboot":false,"shrink":null,"delete":null,
            "bitLocker":{"suspendVolumes":[{"volumeId":"os-volume","driveLetter":"C:"}]},
            "partitions":[
                {"role":"esp","partitionGuid":"aaaaaaaa-0000-4000-8000-000000000001","offsetBytes":1_048_576,"sizeBytes":2_147_483_648_u64,"imageSizeBytes":2_147_483_648_u64,"file":"esp.img.enc","sha256":"d".repeat(64)},
                {"role":"root","partitionGuid":"bbbbbbbb-0000-4000-8000-000000000002","offsetBytes":2_148_532_224_u64,"sizeBytes":allocation-2_147_483_648,"imageSizeBytes":40_800_092_160_u64,"file":"root.img.enc","sha256":"e".repeat(64)}]});
        let mut receipt = plan.clone();
        for (key, value) in json!({"status":"deployed","bootEntry":"Boot000A","readbackVerified":true,"rebooted":false,
            "bitLockerRestoration":{"required":true,"verified":true,"volumes":[{"volumeId":"os-volume","protectionStatus":1}]}}).as_object().unwrap() {
            receipt[key] = value.clone();
        }
        (request, built, plan, receipt)
    }

    #[test]
    fn accepts_bound_plan_and_verified_restoration() {
        let (request, built, mut plan, mut receipt) = fixture();
        validate_direct_plan(&request, "test-operation", &built, &plan).unwrap();
        validate_direct_receipt(&plan, &receipt).unwrap();
        plan["bitLocker"]["suspendVolumes"] = json!([]);
        receipt["bitLockerRestoration"] = json!({"required":false,"verified":true,"volumes":[]});
        validate_direct_receipt(&plan, &receipt).unwrap();
    }

    #[test]
    fn rejects_plan_changes_before_confirmation() {
        let (request, built, plan, _) = fixture();
        for (field, value) in [
            ("diskNumber", json!(3)),
            ("diskUniqueId", json!("other-disk")),
            ("targetKind", json!("delete")),
            ("allocationBytes", json!(40 * 1_073_741_824_u64)),
            ("diskSizeBytes", json!(1024)),
            ("operationId", json!("other-operation")),
            ("manifestSha256", json!("f".repeat(64))),
            ("planSha256", json!("invalid")),
            (
                "bootMenu",
                json!({"defaultOs":"windows","timeoutSeconds":30}),
            ),
            ("delete", json!({"partitionNumber":1})),
            ("reboot", json!(true)),
        ] {
            let mut changed = plan.clone();
            changed[field] = value;
            assert!(
                validate_direct_plan(&request, "test-operation", &built, &changed).is_err(),
                "{field}"
            );
        }
        for (index, field, value) in [
            (0, "offsetBytes", json!(2_097_152)),
            (1, "offsetBytes", json!(1_048_576)),
            (1, "imageSizeBytes", json!(4096)),
            (0, "sha256", json!("bad")),
            (
                1,
                "partitionGuid",
                json!("AAAAAAAA-0000-4000-8000-000000000001"),
            ),
        ] {
            let mut changed = plan.clone();
            changed["partitions"][index][field] = value;
            assert!(
                validate_direct_plan(&request, "test-operation", &built, &changed).is_err(),
                "partition {index} {field}"
            );
        }
    }

    #[test]
    fn rejects_unrelated_or_incomplete_completion() {
        let (_, _, plan, receipt) = fixture();
        for field in [
            "operationId",
            "planSha256",
            "manifestSha256",
            "diskUniqueId",
            "targetKind",
            "allocationBytes",
            "partitions",
            "bootMenu",
            "bootPolicy",
            "encryption",
            "protectionState",
            "readbackVerified",
            "bootEntry",
            "rebooted",
            "bitLockerRestoration",
        ] {
            let mut changed = receipt.clone();
            changed.as_object_mut().unwrap().remove(field);
            assert!(
                validate_direct_receipt(&plan, &changed).is_err(),
                "missing {field}"
            );
        }
        for (field, value) in [
            ("diskUniqueId", json!("other-disk")),
            ("readbackVerified", json!(false)),
            ("bootEntry", json!("unknown")),
            ("rebooted", json!(true)),
        ] {
            let mut changed = receipt.clone();
            changed[field] = value;
            assert!(
                validate_direct_receipt(&plan, &changed).is_err(),
                "changed {field}"
            );
        }
        for value in [
            json!({"required":false,"verified":true,"volumes":[{"volumeId":"os-volume"}]}),
            json!({"required":true,"verified":false,"volumes":[{"volumeId":"os-volume"}]}),
            json!({"required":true,"verified":true,"volumes":[]}),
            json!({"required":true,"verified":true,"volumes":[{"volumeId":"other-volume"}]}),
            json!({"required":true,"verified":true,"volumes":[{"volumeId":"os-volume"},{"volumeId":"os-volume"}]}),
        ] {
            let mut changed = receipt.clone();
            changed["bitLockerRestoration"] = value;
            assert!(validate_direct_receipt(&plan, &changed).is_err());
        }
    }
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
