//! Native target inventory and one active, independently elevated operation.
use crate::setup_protocol::{
    read_frame, write_frame, BootMenu, Destination, DirectTarget, OperationRequest, SourceImage,
};
use crate::{downloads::Downloads, elevation, provider_process, provider_runtime};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    id: String,
    label: String,
    detail: String,
    size_bytes: u64,
    eligible: bool,
    reasons: Vec<String>,
    allocation: Option<AllocationChoice>,
    deletion: Option<DeletionChoice>,
    usb: Option<crate::usb_preserve::Inspection>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionChoice {
    identifier: String,
    partition_label: String,
    file_system: String,
    size_bytes: u64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationChoice {
    mode: &'static str,
    minimum_bytes: u64,
    maximum_bytes: u64,
    recommended_bytes: u64,
    current_partition_bytes: Option<u64>,
    windows_volume: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    kind: Option<String>,
    status: String,
    stage: String,
    message: String,
    choices: Vec<Choice>,
    requirements: Vec<String>,
    bytes: u64,
    total_bytes: u64,
    cancel_available: bool,
    cancel_requested: bool,
    error: Option<String>,
    receipt: Option<Value>,
    preparation: Option<Value>,
    recovery: Option<Value>,
    usb_review: Option<UsbReview>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbReview {
    token: String,
    choice_id: String,
    mode: String,
    label: String,
    detail: String,
    size_bytes: u64,
    file_name: String,
    replaces_bootloader: bool,
}
struct PendingUsbReview {
    token: String,
    request: Value,
    created: std::time::Instant,
}
impl PendingUsbReview {
    fn matches(&self, token: Option<&str>, request: &OperationRequest) -> bool {
        token == Some(self.token.as_str())
            && self.created.elapsed() < std::time::Duration::from_secs(300)
            && serde_json::to_value(request).is_ok_and(|value| value == self.request)
    }
}
struct Inner {
    snapshot: Snapshot,
    destinations: Vec<(String, Destination)>,
    usb_review: Option<PendingUsbReview>,
    writer: Option<Arc<Mutex<Box<dyn Write + Send>>>>,
    cancel: Arc<AtomicBool>,
}
#[derive(Clone)]
pub struct Setup {
    inner: Arc<Mutex<Inner>>,
}
impl Default for Setup {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                snapshot: Snapshot {
                    kind: None,
                    status: "idle".into(),
                    stage: String::new(),
                    message: String::new(),
                    choices: vec![],
                    requirements: vec![],
                    bytes: 0,
                    total_bytes: 0,
                    cancel_available: false,
                    cancel_requested: false,
                    error: None,
                    receipt: None,
                    preparation: None,
                    recovery: None,
                    usb_review: None,
                },
                destinations: vec![],
                usb_review: None,
                writer: None,
                cancel: Arc::new(AtomicBool::new(false)),
            })),
        }
    }
}
fn active(status: &str) -> bool {
    matches!(status, "inspecting" | "running")
}
fn label(value: &Value) -> String {
    value
        .as_str()
        .unwrap_or("")
        .chars()
        .filter(|c| {
            !c.is_control()
                && !('\u{202a}'..='\u{202e}').contains(c)
                && !('\u{2066}'..='\u{2069}').contains(c)
        })
        .take(200)
        .collect()
}
fn reasons(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| items.iter().map(label).collect())
        .unwrap_or_default()
}
impl Setup {
    pub(crate) fn active_operation(&self) -> bool {
        self.inner
            .lock()
            .map(|inner| active(&inner.snapshot.status))
            .unwrap_or(true)
    }
    fn snapshot(&self) -> Result<Snapshot, String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| "Setup state is unavailable")?
            .snapshot
            .clone())
    }
    fn update(&self, app: &tauri::AppHandle, change: impl FnOnce(&mut Inner)) {
        if let Ok(mut inner) = self.inner.lock() {
            change(&mut inner);
            let _ = app.emit("setup-state", &inner.snapshot);
        }
    }
    fn begin(&self, kind: &str, status: &str) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Setup state is unavailable")?;
        if active(&inner.snapshot.status) {
            return Err("A setup operation is already running".into());
        }
        inner.usb_review = None;
        inner.snapshot.usb_review = None;
        inner.snapshot.kind = Some(kind.into());
        inner.snapshot.status = status.into();
        inner.snapshot.error = None;
        inner.snapshot.receipt = None;
        inner.snapshot.recovery = None;
        inner.snapshot.bytes = 0;
        inner.snapshot.total_bytes = 0;
        inner.snapshot.cancel_requested = false;
        inner.snapshot.cancel_available = false;
        inner.snapshot.stage = status.into();
        inner.snapshot.message = if status == "inspecting" {
            "Checking this computer…"
        } else {
            "Waiting for administrator approval…"
        }
        .into();
        if status == "inspecting" {
            inner.snapshot.choices.clear();
            inner.snapshot.requirements.clear();
            inner.snapshot.preparation = None;
            inner.destinations.clear();
        }
        inner.cancel = Arc::new(AtomicBool::new(false));
        Ok(())
    }
    fn fail(&self, app: &tauri::AppHandle, error: String) {
        self.update(app, |inner| {
            inner.snapshot.status = if inner.cancel.load(Ordering::Relaxed) {
                "cancelled"
            } else {
                "failed"
            }
            .into();
            inner.snapshot.error = Some(error);
            inner.snapshot.cancel_available = false;
            if inner.snapshot.kind.as_deref() == Some("usb") {
                for choice in &mut inner.snapshot.choices {
                    choice.usb = None;
                }
            }
            inner.writer = None;
        });
    }
    fn inventory(
        &self,
        app: &tauri::AppHandle,
        kind: &str,
        result: Value,
        source_length: u64,
    ) -> Result<(), String> {
        let mut choices = vec![];
        let mut destinations = vec![];
        let mut requirements = vec![];
        if kind == "usb" {
            for drive in result["drives"]
                .as_array()
                .ok_or("The USB provider did not return an inventory")?
            {
                let identity = &drive["identity"];
                let size = identity["size"].as_u64().unwrap_or(0);
                let sector = identity["blockSize"].as_u64().unwrap_or(0);
                let mut blockers = reasons(&drive["reasons"]);
                if sector == 0
                    || source_length
                        .checked_add(sector.saturating_sub(1))
                        .map(|v| v / sector * sector)
                        .is_none_or(|v| v > size)
                {
                    blockers.push("The drive is too small for this image.".into());
                }
                let eligible = drive["eligible"] == true && blockers.is_empty();
                let id = uuid::Uuid::new_v4().to_string();
                choices.push(Choice {
                    usb: None,
                    id: id.clone(),
                    label: label(&identity["description"]),
                    detail: label(&identity["device"]),
                    size_bytes: size,
                    eligible,
                    reasons: blockers,
                    allocation: None,
                    deletion: None,
                });
                if eligible {
                    destinations.push((
                        id,
                        Destination::Usb {
                            identity: identity.clone(),
                        },
                    ));
                }
            }
        } else {
            for prerequisite in result["prerequisites"]
                .as_array()
                .ok_or("The install provider did not return prerequisites")?
            {
                if prerequisite["available"] != true {
                    requirements.push(if prerequisite["code"] == "pinned_runtime" {
                        "The required local construction runtime is not installed.".into()
                    } else {
                        label(&prerequisite["message"])
                    });
                }
            }
            let minimum = result["minimumUnallocatedBytes"]
                .as_u64()
                .ok_or("The install provider did not report its required space")?;
            if minimum == 0 || minimum % 1_048_576 != 0 {
                return Err("The install provider reported invalid allocation limits".into());
            }
            requirements.extend(reasons(&result["bitLocker"]["blockers"]));
            if result["bitLocker"]["known"] != true {
                requirements.push("BitLocker status must be checked with administrator access before installation.".into());
            }
            for disk in result["disks"]
                .as_array()
                .ok_or("The install provider did not return disks")?
            {
                let number = disk["diskNumber"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or("Invalid disk number")?;
                let name = format!("Disk {number} · {}", label(&disk["friendlyName"]));
                let blockers = reasons(&disk["blockers"]);
                let extents = disk["freeExtents"]
                    .as_array()
                    .ok_or("The disk has no extent inventory")?;
                let fitting: Vec<_> = extents
                    .iter()
                    .filter(|extent| {
                        extent["sizeBytes"]
                            .as_u64()
                            .is_some_and(|size| size >= minimum)
                    })
                    .collect();
                let choices_before = choices.len();
                for extent in fitting {
                    let id = uuid::Uuid::new_v4().to_string();
                    let eligible = disk["eligible"] == true && requirements.is_empty();
                    let maximum = extent["sizeBytes"].as_u64().ok_or("Missing extent size")?
                        / 1_048_576
                        * 1_048_576;
                    let recommended = (64 * 1024_u64.pow(3)).clamp(minimum, maximum);
                    choices.push(Choice {
                        usb: None,
                        id: id.clone(),
                        label: name.clone(),
                        detail: "Unallocated space".into(),
                        size_bytes: extent["sizeBytes"].as_u64().unwrap_or(0),
                        eligible,
                        reasons: blockers.clone(),
                        allocation: Some(AllocationChoice {
                            mode: "free",
                            minimum_bytes: minimum,
                            maximum_bytes: maximum,
                            recommended_bytes: recommended,
                            current_partition_bytes: None,
                            windows_volume: None,
                        }),
                        deletion: None,
                    });
                    if eligible {
                        destinations.push((
                            id,
                            Destination::DirectX86 {
                                disk_number: number,
                                disk_unique_id: disk["diskUniqueId"]
                                    .as_str()
                                    .ok_or("Missing disk identity")?
                                    .to_owned(),
                                target: DirectTarget::Free {
                                    start_offset_bytes: extent["offsetBytes"]
                                        .as_u64()
                                        .ok_or("Missing extent offset")?,
                                },
                                allocation_bytes: recommended,
                                boot_menu: BootMenu::default(),
                            },
                        ));
                    }
                }
                for candidate in disk["shrinkCandidates"].as_array().into_iter().flatten() {
                    let id = uuid::Uuid::new_v4().to_string();
                    let maximum = candidate["maximumAllocationBytes"].as_u64().unwrap_or(0)
                        / 1_048_576
                        * 1_048_576;
                    let eligible = disk["eligible"] == true
                        && candidate["eligible"] == true
                        && requirements.is_empty()
                        && maximum >= minimum;
                    let mut candidate_blockers = blockers.clone();
                    candidate_blockers.extend(reasons(&candidate["blockers"]));
                    let drive_letter = candidate["driveLetter"]
                        .as_str()
                        .unwrap_or("")
                        .trim_end_matches(':')
                        .to_owned();
                    let display_volume = if drive_letter.is_empty() {
                        format!("Partition {}", candidate["partitionNumber"])
                    } else {
                        format!("{drive_letter}:")
                    };
                    let recommended = if maximum >= minimum {
                        (64 * 1024_u64.pow(3)).clamp(minimum, maximum)
                    } else {
                        0
                    };
                    choices.push(Choice {
                        usb: None,
                        id: id.clone(),
                        label: name.clone(),
                        detail: format!("Make space by shrinking {display_volume}"),
                        size_bytes: maximum,
                        eligible,
                        reasons: candidate_blockers,
                        allocation: Some(AllocationChoice {
                            mode: "shrink",
                            minimum_bytes: minimum,
                            maximum_bytes: maximum,
                            recommended_bytes: recommended,
                            current_partition_bytes: candidate["sizeBytes"].as_u64(),
                            windows_volume: Some(display_volume),
                        }),
                        deletion: None,
                    });
                    if eligible {
                        destinations.push((
                            id,
                            Destination::DirectX86 {
                                disk_number: number,
                                disk_unique_id: disk["diskUniqueId"]
                                    .as_str()
                                    .ok_or("Missing disk identity")?
                                    .to_owned(),
                                target: DirectTarget::Shrink {
                                    partition_number: candidate["partitionNumber"]
                                        .as_u64()
                                        .and_then(|value| u32::try_from(value).ok())
                                        .ok_or("Missing Windows partition number")?,
                                    partition_guid: candidate["partitionGuid"]
                                        .as_str()
                                        .ok_or("Missing Windows partition identity")?
                                        .to_owned(),
                                },
                                allocation_bytes: recommended,
                                boot_menu: BootMenu::default(),
                            },
                        ));
                    }
                }
                for candidate in disk["deleteCandidates"].as_array().into_iter().flatten() {
                    let partition_number = candidate["partitionNumber"]
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .ok_or("Missing deletion partition number")?;
                    let size = candidate["sizeBytes"].as_u64().unwrap_or(0);
                    let maximum = candidate["maximumAllocationBytes"].as_u64().unwrap_or(0)
                        / 1_048_576
                        * 1_048_576;
                    let eligible = disk["eligible"] == true
                        && candidate["eligible"] == true
                        && requirements.is_empty()
                        && maximum >= minimum;
                    let recommended = if maximum >= minimum {
                        (64 * 1024_u64.pow(3)).clamp(minimum, maximum)
                    } else {
                        0
                    };
                    let mut candidate_blockers = blockers.clone();
                    candidate_blockers.extend(reasons(&candidate["blockers"]));
                    let id = uuid::Uuid::new_v4().to_string();
                    let identifier = format!("Disk {number} Partition {partition_number}");
                    choices.push(Choice {
                        usb: None,
                        id: id.clone(),
                        label: name.clone(),
                        detail: format!(
                            "Delete partition {partition_number} and use this space · {}",
                            label(&candidate["fileSystem"])
                        ),
                        size_bytes: size,
                        eligible,
                        reasons: candidate_blockers,
                        allocation: Some(AllocationChoice {
                            mode: "delete",
                            minimum_bytes: minimum,
                            maximum_bytes: maximum,
                            recommended_bytes: recommended,
                            current_partition_bytes: Some(size),
                            windows_volume: None,
                        }),
                        deletion: Some(DeletionChoice {
                            identifier,
                            partition_label: label(&candidate["label"]),
                            file_system: label(&candidate["fileSystem"]),
                            size_bytes: size,
                        }),
                    });
                    if eligible {
                        destinations.push((
                            id,
                            Destination::DirectX86 {
                                disk_number: number,
                                disk_unique_id: disk["diskUniqueId"]
                                    .as_str()
                                    .ok_or("Missing disk identity")?
                                    .to_owned(),
                                target: DirectTarget::Delete {
                                    partition_number,
                                    partition_guid: candidate["partitionGuid"]
                                        .as_str()
                                        .ok_or("Missing deletion partition identity")?
                                        .to_owned(),
                                    start_offset_bytes: candidate["offsetBytes"]
                                        .as_u64()
                                        .ok_or("Missing deletion partition offset")?,
                                    partition_size_bytes: size,
                                    confirmation: String::new(),
                                },
                                allocation_bytes: recommended,
                                boot_menu: BootMenu::default(),
                            },
                        ));
                    }
                }
                if choices.len() == choices_before {
                    let mut disk_reasons = blockers.clone();
                    disk_reasons.push("No sufficiently large unallocated space or supported Windows shrink candidate was found.".into());
                    choices.push(Choice {
                        usb: None,
                        id: uuid::Uuid::new_v4().to_string(),
                        label: name,
                        detail: "No suitable installation space".into(),
                        size_bytes: disk["sizeBytes"].as_u64().unwrap_or(0),
                        eligible: false,
                        reasons: disk_reasons,
                        allocation: None,
                        deletion: None,
                    });
                }
            }
        }
        self.update(app, |inner| {
            inner.destinations = destinations;
            inner.snapshot.choices = choices;
            inner.snapshot.requirements = requirements;
            inner.snapshot.preparation = (kind == "direct").then(|| json!({
                "secureBoot":label(&result["secureBoot"]),
                "runtimePackaged":result["runtimePackaged"] == true,
                "runtimeReady":result["prerequisites"].as_array().is_some_and(|items| items.iter().any(|item| item["code"] == "pinned_runtime" && item["available"] == true))
            }));
            inner.snapshot.status = "ready".into();
            inner.snapshot.stage = "ready".into();
            inner.snapshot.message = "Choose a destination to continue.".into();
            inner.snapshot.cancel_available = false;
            inner.writer = None;
        });
        Ok(())
    }
}
fn source(downloads: &Downloads) -> Result<SourceImage, String> {
    let (path, release) = downloads.verified_source()?;
    Ok(SourceImage {
        path,
        file_name: release.file_name().into(),
        length: release.length(),
        sha256: release.sha256().into(),
        signature: release.signature().to_vec(),
    })
}

#[tauri::command]
pub fn setup_status(setup: State<'_, Setup>) -> Result<Snapshot, String> {
    setup.snapshot()
}

#[tauri::command]
pub fn inspect_setup(
    kind: String,
    app: tauri::AppHandle,
    setup: State<'_, Setup>,
    downloads: State<'_, Downloads>,
) -> Result<Snapshot, String> {
    if app
        .state::<crate::apple_setup::AppleSetupService>()
        .active_operation()
    {
        return Err("An Apple installation operation is already running".into());
    }
    if kind != "usb" && kind != "direct" {
        return Err("Unknown installation option".into());
    }
    if kind == "direct" && !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("Use the Apple installation flow on a supported Mac. Direct installation on this host platform is not available.".into());
    }
    let source = source(&downloads)?;
    setup.begin(&kind, "inspecting")?;
    let service = setup.inner().clone();
    std::thread::spawn(move || {
        let result = if kind == "direct" {
            run_elevated(
                &service,
                &app,
                OperationRequest {
                    protocol: 1,
                    source: source.clone(),
                    destination: Destination::InspectDirect,
                },
            )
            .and_then(|result| {
                service.inventory(&app, &kind, result["inspection"].clone(), source.length)
            })
        } else {
            (|| {
                let manifest = provider_runtime::manifest()?;
                let root = provider_runtime::locate(&manifest)?;
                let cancel = Arc::new(AtomicBool::new(false));
                provider_runtime::verify_inspection(&root, &manifest, &cancel)?;
                let result = provider_process::run(
                    provider_process::inspection_command(&root, &manifest)?,
                    Some(json!({"protocol":1,"action":"list","sourcePath":source.path})),
                    cancel,
                    None,
                    false,
                    |_| {},
                )?;
                service.inventory(&app, &kind, result, source.length)
            })()
        };
        if let Err(error) = result {
            service.fail(&app, error);
        }
    });
    setup.snapshot()
}

#[tauri::command]
pub fn prepare_setup(
    action: String,
    app: tauri::AppHandle,
    setup: State<'_, Setup>,
    downloads: State<'_, Downloads>,
) -> Result<Snapshot, String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("This preparation requires Windows x64".into());
    }
    if app
        .state::<crate::apple_setup::AppleSetupService>()
        .active_operation()
    {
        return Err("Another installation is active".into());
    }
    let destination = match action.as_str() {
        "firmware" => Destination::PrepareFirmware,
        "runtime" => Destination::PrepareRuntime,
        _ => return Err("Unknown preparation action".into()),
    };
    let source = source(&downloads)?;
    setup.begin("direct", "running")?;
    let service = setup.inner().clone();
    std::thread::spawn(move || {
        match run_elevated(&service, &app, OperationRequest { protocol:1, source, destination }) {
            Ok(receipt) => service.update(&app, |inner| {
                inner.snapshot.status = "prepared".into();
                inner.snapshot.stage = "prepared".into();
                inner.snapshot.message = if action == "firmware" {
                    "Restart requested. Disable Secure Boot in firmware, leave TPM enabled, save and return to Windows. Reopen this app and select Check disks."
                } else { "Construction runtime ready. Select Refresh disks to continue." }.into();
                inner.snapshot.choices.clear(); inner.destinations.clear();
                inner.snapshot.requirements.clear(); inner.snapshot.preparation = None;
                inner.snapshot.receipt = Some(receipt); inner.snapshot.cancel_available = false; inner.writer = None;
            }),
            Err(error) => service.fail(&app, error),
        }
    });
    setup.snapshot()
}

#[tauri::command]
pub fn inspect_usb_choice(
    choice_id: String,
    elevated: bool,
    app: tauri::AppHandle,
    setup: State<'_, Setup>,
    downloads: State<'_, Downloads>,
) -> Result<Snapshot, String> {
    if app
        .state::<crate::apple_setup::AppleSetupService>()
        .active_operation()
    {
        return Err("Another installation is active".into());
    }
    let image = source(&downloads)?;
    let identity = {
        let mut inner = setup
            .inner
            .lock()
            .map_err(|_| "Setup state is unavailable")?;
        if active(&inner.snapshot.status) {
            return Err("A setup operation is already running".into());
        }
        let identity = inner
            .destinations
            .iter()
            .find_map(|(id, dest)| match dest {
                Destination::Usb { identity } if id == &choice_id => Some(identity.clone()),
                _ => None,
            })
            .ok_or("Choose a USB from a fresh disk inspection")?;
        inner.snapshot.status = "running".into();
        inner.snapshot.stage = "checking-usb".into();
        inner.snapshot.message = "Checking this USB’s free space and boot compatibility…".into();
        inner.snapshot.error = None;
        inner.snapshot.cancel_available = false;
        if let Some(choice) = inner
            .snapshot
            .choices
            .iter_mut()
            .find(|choice| choice.id == choice_id)
        {
            choice.usb = None;
        }
        identity
    };
    let service = setup.inner().clone();
    std::thread::spawn(move || {
        let inspection = if elevated {
            run_elevated(
                &service,
                &app,
                OperationRequest {
                    protocol: 1,
                    source: image.clone(),
                    destination: Destination::InspectUsb { identity },
                },
            )
            .and_then(|value| {
                serde_json::from_value::<crate::usb_preserve::Inspection>(
                    value["usbInspection"].clone(),
                )
                .map_err(|e| e.to_string())
            })
        } else {
            Ok(crate::usb_preserve::inspect(
                &identity,
                image.length,
                &image.sha256,
            ))
        };
        match inspection {
            Ok(inspection) => service.update(&app, |inner| {
                if let Some(choice) = inner
                    .snapshot
                    .choices
                    .iter_mut()
                    .find(|choice| choice.id == choice_id)
                {
                    choice.usb = Some(inspection);
                }
                inner.snapshot.status = "ready".into();
                inner.snapshot.stage = "ready".into();
                inner.snapshot.message = "USB compatibility checked.".into();
                inner.snapshot.cancel_available = false;
                inner.writer = None;
            }),
            Err(error) => service.fail(&app, error),
        }
    });
    setup.snapshot()
}

#[tauri::command]
pub fn review_usb_setup(
    choice_id: String,
    mode: String,
    app: tauri::AppHandle,
    setup: State<'_, Setup>,
    downloads: State<'_, Downloads>,
) -> Result<Snapshot, String> {
    if app
        .state::<crate::apple_setup::AppleSetupService>()
        .active_operation()
    {
        return Err("Another installation is active".into());
    }
    if !["erase", "preserve"].contains(&mode.as_str()) {
        return Err("Unknown USB operation".into());
    }
    let source = source(&downloads)?;
    let mut inner = setup
        .inner
        .lock()
        .map_err(|_| "Setup state is unavailable")?;
    if active(&inner.snapshot.status) {
        return Err("A setup operation is already running".into());
    }
    let choice = inner
        .snapshot
        .choices
        .iter()
        .find(|c| c.id == choice_id && c.eligible)
        .ok_or("Choose a USB from a fresh disk inspection")?
        .clone();
    let identity = inner
        .destinations
        .iter()
        .find_map(|(id, d)| match d {
            Destination::Usb { identity } if id == &choice_id => Some(identity.clone()),
            _ => None,
        })
        .ok_or("Choose a USB from a fresh disk inspection")?;
    let destination = if mode == "preserve" {
        let inspection = choice
            .usb
            .as_ref()
            .filter(|i| i.eligible)
            .ok_or("Check this USB's compatibility first")?;
        Destination::UsbPreserve {
            identity,
            plan: inspection
                .plan
                .clone()
                .ok_or("Missing USB compatibility plan")?,
        }
    } else {
        Destination::Usb { identity }
    };
    let request = OperationRequest {
        protocol: 1,
        source: source.clone(),
        destination,
    };
    crate::setup_protocol::usb_confirmation_plan(&request)?;
    let token = uuid::Uuid::new_v4().to_string();
    inner.usb_review = Some(PendingUsbReview {
        token: token.clone(),
        request: serde_json::to_value(&request).map_err(|e| e.to_string())?,
        created: std::time::Instant::now(),
    });
    inner.snapshot.usb_review = Some(UsbReview {
        token,
        choice_id,
        mode,
        label: choice.label,
        detail: choice.detail,
        size_bytes: choice.size_bytes,
        file_name: source.file_name,
        replaces_bootloader: choice.usb.is_some_and(|i| i.replaces_bootloader),
    });
    inner.snapshot.error = None;
    Ok(inner.snapshot.clone())
}

#[tauri::command]
pub fn dismiss_usb_review(setup: State<'_, Setup>) -> Result<Snapshot, String> {
    let mut inner = setup
        .inner
        .lock()
        .map_err(|_| "Setup state is unavailable")?;
    inner.usb_review = None;
    inner.snapshot.usb_review = None;
    Ok(inner.snapshot.clone())
}

#[tauri::command]
pub fn start_setup(
    choice_id: String,
    allocation_bytes: Option<String>,
    delete_confirmation: Option<String>,
    boot_menu: Option<BootMenu>,
    usb_mode: Option<String>,
    usb_review_token: Option<String>,
    app: tauri::AppHandle,
    setup: State<'_, Setup>,
    downloads: State<'_, Downloads>,
) -> Result<Snapshot, String> {
    if app
        .state::<crate::apple_setup::AppleSetupService>()
        .active_operation()
    {
        return Err("An Apple installation operation is already running".into());
    }
    let source = source(&downloads)?;
    let (mut destination, allocation) = {
        let inner = setup
            .inner
            .lock()
            .map_err(|_| "Setup state is unavailable")?;
        if active(&inner.snapshot.status) {
            return Err("A setup operation is already running".into());
        }
        let destination = inner
            .destinations
            .iter()
            .find(|(id, _)| id == &choice_id)
            .map(|(_, dest)| dest.clone())
            .ok_or("Choose an eligible destination from a fresh disk inspection")?;
        let allocation = inner
            .snapshot
            .choices
            .iter()
            .find(|choice| choice.id == choice_id && choice.eligible)
            .and_then(|choice| choice.allocation.clone());
        (destination, allocation)
    };
    if let Some(mode) = usb_mode.as_deref() {
        if !["erase", "preserve"].contains(&mode) {
            return Err("Unknown USB operation".into());
        }
        let Destination::Usb { identity } = &destination else {
            return Err("USB mode requires a USB target".into());
        };
        if mode != "erase" {
            let inner = setup
                .inner
                .lock()
                .map_err(|_| "Setup state is unavailable")?;
            let inspection = inner
                .snapshot
                .choices
                .iter()
                .find(|c| c.id == choice_id)
                .and_then(|c| c.usb.as_ref())
                .ok_or("Check the selected USB’s compatibility first")?;
            if !inspection.eligible {
                return Err("This USB operation is not available for the inspected layout".into());
            }
            destination = Destination::UsbPreserve {
                identity: identity.clone(),
                plan: inspection
                    .plan
                    .clone()
                    .ok_or("Missing USB compatibility plan")?,
            };
        }
    }
    let kind = match &mut destination {
        Destination::Usb { .. } | Destination::UsbPreserve { .. } => {
            if allocation_bytes.is_some() || delete_confirmation.is_some() || boot_menu.is_some() {
                return Err("USB creation does not accept a partition allocation".into());
            }
            "usb"
        }
        Destination::DirectX86 {
            allocation_bytes: selected_size,
            boot_menu: selected_boot,
            disk_number,
            target,
            ..
        } => {
            let settings = boot_menu.ok_or("Review the startup menu settings")?;
            settings.validate()?;
            *selected_boot = settings;
            match target {
                DirectTarget::Delete {
                    partition_number,
                    confirmation,
                    ..
                } => {
                    let expected = format!("Disk {disk_number} Partition {partition_number}");
                    if !delete_confirmation
                        .as_deref()
                        .is_some_and(|typed| typed.trim().eq_ignore_ascii_case(&expected))
                    {
                        return Err(format!(
                            "Type {expected} in the partition deletion warning to continue"
                        ));
                    }
                    *confirmation = expected;
                }
                _ if delete_confirmation.is_some() => {
                    return Err("Deletion confirmation does not match this target".into())
                }
                _ => {}
            }
            let bounds = allocation.ok_or("Refresh the installation space choices")?;
            let raw = allocation_bytes
                .as_deref()
                .ok_or("Choose the space to allocate to Omarchy")?;
            if raw.is_empty() || raw.len() > 16 || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("Installation size must be an integer byte count".into());
            }
            let requested = raw
                .parse::<u64>()
                .map_err(|_| "Invalid installation size")?;
            if requested < bounds.minimum_bytes
                || requested > bounds.maximum_bytes
                || requested % 1_048_576 != 0
            {
                return Err(
                    "Choose an installation size within the inspected limits, aligned to 1 MiB"
                        .into(),
                );
            }
            *selected_size = requested;
            "direct"
        }
        _ => return Err("Invalid destination".into()),
    };
    if kind == "usb" {
        let request = OperationRequest {
            protocol: 1,
            source: source.clone(),
            destination: destination.clone(),
        };
        let mut inner = setup
            .inner
            .lock()
            .map_err(|_| "Setup state is unavailable")?;
        let review = inner
            .usb_review
            .take()
            .ok_or("Review and confirm this USB in the app first")?;
        inner.snapshot.usb_review = None;
        if !review.matches(usb_review_token.as_deref(), &request) {
            return Err(
                "The drive or installer changed, or the review expired. Review this USB again."
                    .into(),
            );
        }
    }
    setup.begin(kind, "running")?;
    let service = setup.inner().clone();
    std::thread::spawn(move || {
        match run_elevated(
            &service,
            &app,
            OperationRequest {
                protocol: 1,
                source,
                destination,
            },
        ) {
            Ok(receipt) => service.update(&app, |inner| {
                inner.snapshot.status = "complete".into();
                inner.snapshot.stage = "complete".into();
                inner.snapshot.message = "Operation completed.".into();
                inner.snapshot.receipt = Some(receipt);
                inner.snapshot.cancel_available = false;
                inner.writer = None;
            }),
            Err(error) => service.fail(&app, error),
        }
    });
    setup.snapshot()
}

#[tauri::command]
pub fn cancel_setup(setup: State<'_, Setup>) -> Result<Snapshot, String> {
    let mut inner = setup
        .inner
        .lock()
        .map_err(|_| "Setup state is unavailable")?;
    if !active(&inner.snapshot.status) || !inner.snapshot.cancel_available {
        return Err("Cancellation is not available during this stage".into());
    }
    inner.cancel.store(true, Ordering::Relaxed);
    inner.snapshot.cancel_requested = true;
    if let Some(writer) = &inner.writer {
        write_frame(
            &mut *writer
                .lock()
                .map_err(|_| "Helper connection is unavailable")?,
            &json!({"protocol":1,"type":"cancel"}),
        )?;
    }
    Ok(inner.snapshot.clone())
}

fn consume_usb_confirmation(
    expected: &Value,
    actual: &Value,
    used: &mut bool,
) -> Result<(), String> {
    if *used || expected != actual {
        return Err("The USB write plan changed. Review the drive again.".into());
    }
    *used = true;
    Ok(())
}

fn run_elevated(
    service: &Setup,
    app: &tauri::AppHandle,
    request: OperationRequest,
) -> Result<Value, String> {
    let usb_approval = crate::setup_protocol::usb_confirmation_plan(&request)?;
    let mut usb_approval_used = false;
    let connection = elevation::launch()?;
    let writer = Arc::new(Mutex::new(connection.writer));
    write_frame(
        &mut *writer
            .lock()
            .map_err(|_| "Helper connection is unavailable")?,
        &request,
    )?;
    service.update(app, |inner| {
        inner.writer = Some(Arc::clone(&writer));
        inner.snapshot.cancel_available = true;
    });
    let mut reader = BufReader::new(connection.reader);
    while let Some(value) = read_frame(&mut reader)? {
        match value["type"].as_str() {
            Some("confirmation_required") => {
                let binding = value["binding"]
                    .as_str()
                    .ok_or("The helper omitted the confirmation binding")?;
                let summary = value["summary"]
                    .as_str()
                    .ok_or("The helper omitted the confirmation summary")?;
                let operation = value["plan"]["kind"].as_str().unwrap_or("");
                let (dialog_kind, button) = match operation {
                    "usb_preserve" => (MessageDialogKind::Info, "Keep files and add installer"),
                    "usb" => (MessageDialogKind::Warning, "Erase USB and create installer"),
                    _ => (MessageDialogKind::Warning, "Continue"),
                };
                let approved = if let Some(expected) = &usb_approval {
                    consume_usb_confirmation(expected, &value["plan"], &mut usb_approval_used)?;
                    !service
                        .inner
                        .lock()
                        .map_err(|_| "Setup state is unavailable")?
                        .cancel
                        .load(Ordering::Relaxed)
                } else {
                    app.dialog()
                        .message(summary)
                        .title("Confirm Omarchy setup")
                        .kind(dialog_kind)
                        .buttons(MessageDialogButtons::OkCancelCustom(
                            button.into(),
                            "Cancel".into(),
                        ))
                        .blocking_show()
                };
                write_frame(
                    &mut *writer
                        .lock()
                        .map_err(|_| "Helper connection is unavailable")?,
                    &json!({"protocol":1,"type":"confirmation","binding":binding,"approved":approved}),
                )?;
            }
            Some("event") => service.update(app, |inner| {
                if value["recovery"].is_object() {
                    inner.snapshot.recovery = Some(value["recovery"].clone());
                }
                let previous = inner.snapshot.stage.clone();
                if let Some(stage) = value["stage"].as_str() {
                    inner.snapshot.stage = stage.into();
                }
                if previous != inner.snapshot.stage {
                    inner.snapshot.bytes = 0;
                    inner.snapshot.total_bytes = 0;
                }
                if let Some(message) = value["message"].as_str() {
                    inner.snapshot.message = message.into();
                } else {
                    inner.snapshot.message = match inner.snapshot.stage.as_str() {
                        "validating" => "Checking the image and USB drive…",
                        "unmounting" => "Locking and unmounting USB volumes…",
                        "writing" => "Writing the image to the USB drive…",
                        "verifying" => "Verifying the written image…",
                        "readback" => "Reading back the USB to verify its checksum…",
                        "flushing" => "Flushing pending writes to the USB drive…",
                        "ejecting" => "Ejecting the USB drive…",
                        "hashing" => "Checking the source image checksum…",
                        "finishing" => "Finishing the operation…",
                        "finished" => "Completing setup…",
                        _ => "Working…",
                    }
                    .into();
                }
                if let Some(bytes) = value["bytes"].as_u64() {
                    inner.snapshot.bytes = bytes;
                }
                if let Some(total) = value["totalBytes"].as_u64() {
                    inner.snapshot.total_bytes = total;
                }
                if let Some(available) = value["cancelAvailable"].as_bool() {
                    inner.snapshot.cancel_available = available;
                }
            }),
            Some("result") => return Ok(value["result"].clone()),
            Some("error") => {
                return Err(value["message"]
                    .as_str()
                    .unwrap_or("Installation failed")
                    .into())
            }
            _ => return Err("The helper sent an unexpected response".into()),
        }
    }
    Err(
        "The helper disconnected before completing. Inspect the destination before retrying."
            .into(),
    )
}

#[cfg(test)]
mod usb_review_tests {
    use super::*;
    fn request() -> OperationRequest {
        OperationRequest {
            protocol: 1,
            source: SourceImage {
                path: "C:/installer.iso".into(),
                file_name: "installer.iso".into(),
                length: 1025,
                sha256: "verified-image".into(),
                signature: vec![1],
            },
            destination: Destination::Usb {
                identity: json!({"device":"PhysicalDrive9","fingerprint":"disk-a","blockSize":512,"size":8192}),
            },
        }
    }
    #[test]
    fn review_is_bound_to_token_drive_mode_source_and_expiration() {
        let original = request();
        let mut review = PendingUsbReview {
            token: "review-token".into(),
            request: serde_json::to_value(&original).unwrap(),
            created: std::time::Instant::now(),
        };
        assert!(review.matches(Some("review-token"), &original));
        assert!(!review.matches(None, &original));
        assert!(!review.matches(Some("other-token"), &original));
        let mut changed = request();
        changed.source.path = "D:/installer.iso".into();
        assert!(!review.matches(Some("review-token"), &changed));
        let mut changed = request();
        changed.source.sha256 = "different-image".into();
        assert!(!review.matches(Some("review-token"), &changed));
        let mut changed = request();
        if let Destination::Usb { identity } = &mut changed.destination {
            identity["fingerprint"] = json!("disk-b");
        }
        assert!(!review.matches(Some("review-token"), &changed));
        let mut changed = request();
        changed.destination = Destination::InspectUsb {
            identity: json!({}),
        };
        assert!(!review.matches(Some("review-token"), &changed));
        review.created = std::time::Instant::now() - std::time::Duration::from_secs(301);
        assert!(!review.matches(Some("review-token"), &original));
    }
    #[test]
    fn helper_gate_rejects_changed_or_repeated_write_plans() {
        let plan = crate::setup_protocol::usb_confirmation_plan(&request())
            .unwrap()
            .unwrap();
        assert_eq!(plan["writeSpanBytes"], 1536);
        let mut used = false;
        let mut changed = plan.clone();
        changed["target"]["fingerprint"] = json!("other-disk");
        assert!(consume_usb_confirmation(&plan, &changed, &mut used).is_err());
        assert!(!used);
        let mut changed = plan.clone();
        changed["sourceSha256"] = json!("other-image");
        assert!(consume_usb_confirmation(&plan, &changed, &mut used).is_err());
        consume_usb_confirmation(&plan, &plan, &mut used).unwrap();
        assert!(consume_usb_confirmation(&plan, &plan, &mut used).is_err());
    }
}
