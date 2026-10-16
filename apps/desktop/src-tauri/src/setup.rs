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
    bit_locker: Option<Value>,
    staged_iso: Option<Value>,
    staged_testing: bool,
    staged_recovery: Option<Value>,
    staged_review: Option<Value>,
    // The running staged action and the recorded installer it acts on, so the
    // app can tell it from other records, even after reloading.
    staged_target: Option<Value>,
    // Secure Boot as Windows shows it to standard users, read with each
    // snapshot, so the home screen knows it without an elevated check.
    secure_boot: Option<bool>,
    // The steps a staged preparation goes through, and the current one, so
    // the app shows progress, also after reloading.
    steps: Option<Value>,
    step: Option<String>,
    // The background disk check: status, latest inspection and its image.
    staged_check: Option<Value>,
    // Administrator access for this run: "asking" while the startup prompt is
    // open, "ready" once approved, "declined"; none before asking or after
    // the helper went away.
    admin: Option<String>,
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
    // The running operation's ID, which its cancel frame repeats.
    operation: Option<String>,
    cancel: Arc<AtomicBool>,
    staged_review: Option<std::sync::mpsc::Sender<bool>>,
    // The user declined the staged write's review in this operation.
}
#[derive(Clone)]
pub struct Setup {
    inner: Arc<Mutex<Inner>>,
    // The elevated helper serving this app's operations. Held for the length
    // of each operation, so a launch in progress (the prompt at startup) and
    // an operation started meanwhile share one prompt.
    session: Arc<Mutex<Option<Session>>>,
    // The disk check started in the background, kept apart from the main
    // snapshot so a late result can't overwrite a foreground operation.
    check: Arc<Mutex<Check>>,
}
#[derive(Default)]
struct Check {
    // Bumped by each start; results from an older start are dropped.
    generation: u64,
    status: String,
    result: Option<Value>,
    message: String,
    // The image the check was for.
    identity: String,
    writer: Option<Arc<Mutex<Box<dyn Write + Send>>>>,
    operation: Option<String>,
}
impl Check {
    fn view(&self) -> Option<Value> {
        (!self.status.is_empty()).then(|| {
            json!({"status":self.status,"result":self.result,"message":self.message,"identity":self.identity})
        })
    }
}
struct Session {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    reader: BufReader<Box<dyn std::io::Read + Send>>,
    alive: Arc<AtomicBool>,
}
impl Session {
    fn new(connection: elevation::Connection) -> Self {
        Self {
            writer: Arc::new(Mutex::new(connection.writer)),
            reader: BufReader::new(connection.reader),
            alive: connection.alive,
        }
    }
}
// On Windows the helper serves every operation while the app is open; other
// systems keep one helper (and one prompt) per operation until tested.
const KEEP_SESSION: bool = cfg!(windows);
/// The current session, starting one (with its prompt) only if there is none
/// or its helper has exited since the last operation.
fn acquire_session(
    holder: &Mutex<Option<Session>>,
    launch: impl FnOnce() -> Result<elevation::Connection, String>,
) -> Result<std::sync::MutexGuard<'_, Option<Session>>, String> {
    let mut guard = holder.lock().map_err(|_| "Helper session is unavailable")?;
    if guard
        .as_ref()
        .is_some_and(|session| !session.alive.load(Ordering::Relaxed))
    {
        *guard = None;
    }
    if guard.is_none() {
        *guard = Some(Session::new(launch()?));
    }
    Ok(guard)
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
                    bit_locker: None,
                    staged_iso: None,
                    staged_testing: crate::direct_install_policy::STAGED_ISO_TESTING,
                    staged_recovery: None,
                    staged_review: None,
                    staged_target: None,
                    secure_boot: None,
                    steps: None,
                    step: None,
                    admin: None,
                    staged_check: None,
                },
                destinations: vec![],
                usb_review: None,
                writer: None,
                operation: None,
                cancel: Arc::new(AtomicBool::new(false)),
                staged_review: None,
            })),
            session: Arc::new(Mutex::new(None)),
            check: Arc::new(Mutex::new(Check::default())),
        }
    }
}
// Declining a confirmation in a staged operation (Review, or the arm,
// firmware and removal dialogs) isn't a failure: every one comes before any
// change, and the inspection stays so the choose screen returns.
fn record_failure(inner: &mut Inner, error: String) {
    inner.snapshot.staged_review = None;
    let checking = inner
        .snapshot
        .staged_target
        .as_ref()
        .is_some_and(|target| target["action"] == "inspect");
    if inner.snapshot.staged_target.is_some() && crate::setup_helper::declined(&error) {
        inner.snapshot.status = "cancelled".into();
        inner.snapshot.error = None;
        inner.snapshot.message = "Nothing was changed.".into();
    } else if checking && inner.cancel.load(Ordering::Relaxed) {
        // A stopped disk check isn't a failure; partitions it measured stay.
        inner.snapshot.status = "cancelled".into();
        inner.snapshot.error = None;
        inner.snapshot.message = "Disk check stopped.".into();
    } else {
        inner.snapshot.status = if inner.cancel.load(Ordering::Relaxed) {
            "cancelled"
        } else {
            "failed"
        }
        .into();
        inner.snapshot.error = Some(error);
    }
    inner.snapshot.cancel_available = false;
    if inner.snapshot.kind.as_deref() == Some("usb") {
        for choice in &mut inner.snapshot.choices {
            choice.usb = None;
        }
    }
    inner.writer = None;
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
#[cfg(any(windows, test))]
fn read_staging_summary(path: &std::path::Path) -> Option<Value> {
    use std::io::Read;
    // Display-only data. A forged index cannot authorize any helper operation.
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 1_048_576 {
        return None;
    }
    let summary: Value = serde_json::from_slice(&bytes).ok()?;
    let operations = summary["operations"].as_array()?;
    let errors = summary["recordErrors"].as_array()?;
    if operations.len() > 256 || errors.len() > 256 {
        return None;
    }
    for operation in operations {
        uuid::Uuid::parse_str(operation["operationId"].as_str()?).ok()?;
        operation["status"].as_str()?;
        operation["diskNumber"].as_u64()?;
        operation["temporaryBytes"].as_u64()?;
        // The partition the installer must delete, shown as text.
        if !operation["replaces"].is_null() && operation["replaces"].as_str()?.chars().count() > 64
        {
            return None;
        }
    }
    for error in errors {
        uuid::Uuid::parse_str(error["operationId"].as_str()?).ok()?;
        error["message"].as_str()?;
    }
    let mut summary = summary;
    mark_restarts(&mut summary, current_boot_unix_seconds());
    Some(summary)
}
// Arming records the Windows boot it happened in. A later boot means the
// selected restart has happened, so the card can move on to removal.
#[cfg(any(windows, test))]
fn mark_restarts(summary: &mut Value, current_boot: Option<u64>) {
    let (Some(current_boot), Some(operations)) =
        (current_boot, summary["operations"].as_array_mut())
    else {
        return;
    };
    for operation in operations {
        if let Some(scheduled) = operation["scheduledBootUnixSeconds"].as_u64() {
            // Both boot times are estimates; allow for clock adjustments.
            operation["restartedSinceScheduled"] = Value::Bool(current_boot > scheduled + 120);
        }
    }
}
#[cfg(windows)]
fn current_boot_unix_seconds() -> Option<u64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let uptime = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() } / 1000;
    now.checked_sub(uptime)
}
#[cfg(all(test, not(windows)))]
fn current_boot_unix_seconds() -> Option<u64> {
    None
}
impl Setup {
    pub(crate) fn active_operation(&self) -> bool {
        self.inner
            .lock()
            .map(|inner| active(&inner.snapshot.status))
            .unwrap_or(true)
    }
    fn snapshot(&self) -> Result<Snapshot, String> {
        let mut snapshot = self
            .inner
            .lock()
            .map_err(|_| "Setup state is unavailable")?
            .snapshot
            .clone();
        snapshot.staged_check = self.check.lock().ok().and_then(|check| check.view());
        #[cfg(windows)]
        {
            let mut snapshot = snapshot;
            snapshot.staged_recovery =
                crate::provider_process::program_data()
                    .ok()
                    .and_then(|root| {
                        read_staging_summary(&root.join("OmarchyStagedInstaller/summary.json"))
                    });
            snapshot.secure_boot = crate::startup::secure_boot();
            Ok(snapshot)
        }
        #[cfg(not(windows))]
        Ok(snapshot)
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
        inner.snapshot.staged_review = None;
        inner.snapshot.staged_target = None;
        inner.snapshot.steps = None;
        inner.snapshot.step = None;
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
        self.update(app, |inner| record_failure(inner, error));
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
                        label: "Use unallocated space".into(),
                        detail: name.clone(),
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
                        label: format!("Make space on {display_volume}"),
                        detail: format!("{name} · NTFS"),
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
                        label: format!("Replace partition {partition_number}"),
                        detail: format!("{name} · {}", label(&candidate["fileSystem"])),
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
    crate::direct_install_policy::check_destination(&destination)?;
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
pub fn staged_iso(
    action: crate::setup_protocol::StagedAction,
    selection: Option<crate::setup_protocol::StagedSelection>,
    operation_id: Option<String>,
    resize: Option<crate::setup_protocol::StagedResizeQuery>,
    app: tauri::AppHandle,
    setup: State<'_, Setup>,
    downloads: State<'_, Downloads>,
) -> Result<Snapshot, String> {
    use crate::setup_protocol::StagedAction;
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("Staged installation requires Windows x64".into());
    }
    if app
        .state::<crate::apple_setup::AppleSetupService>()
        .active_operation()
    {
        return Err("Another installation is active".into());
    }
    let destination = Destination::StagedIso {
        action: action.clone(),
        selection,
        operation_id,
        resize,
    };
    crate::direct_install_policy::check_destination(&destination)?;
    let target = match &destination {
        Destination::StagedIso {
            action,
            operation_id,
            ..
        } => json!({"action": action, "operationId": operation_id}),
        _ => Value::Null,
    };
    let source = if matches!(action, StagedAction::Stage) {
        source(&downloads)?
    } else if matches!(action, StagedAction::Inspect) {
        // A finished download when there is one; its identity otherwise.
        source(&downloads).or_else(|_| identity_source(&downloads))?
    } else {
        // Existing staging records are self-contained. Cleanup must still work
        // after the downloaded ISO was removed or a newer release appeared.
        SourceImage {
            path: std::env::current_exe().map_err(|e| e.to_string())?,
            file_name: String::new(),
            length: 0,
            sha256: String::new(),
            signature: vec![],
        }
    };
    setup.begin("direct", "running")?;
    setup.update(&app, |inner| {
        inner.snapshot.kind = None;
        inner.snapshot.stage = "staged-installer".into();
        inner.snapshot.staged_target = Some(target);
        if matches!(action, StagedAction::Inspect) {
            inner.snapshot.staged_iso = None;
            inner.snapshot.bit_locker = None;
        }
    });
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
            Ok(result) => service.update(&app, |inner| {
                inner.snapshot.status = "complete".into();
                inner.snapshot.stage = "complete".into();
                inner.snapshot.message = result["message"]
                    .as_str()
                    .unwrap_or("Temporary installer updated.")
                    .into();
                inner.snapshot.staged_iso = Some(result);
                inner.snapshot.staged_review = None;
                inner.snapshot.cancel_available = false;
                inner.writer = None;
            }),
            Err(error) => service.fail(&app, error),
        }
    });
    setup.snapshot()
}

#[tauri::command]
pub fn prepare_bitlocker(
    reminder_only: bool,
    app: tauri::AppHandle,
    setup: State<'_, Setup>,
    downloads: State<'_, Downloads>,
) -> Result<Snapshot, String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("Windows encryption preparation requires Windows x64".into());
    }
    if app
        .state::<crate::apple_setup::AppleSetupService>()
        .active_operation()
    {
        return Err("Another installation is active".into());
    }
    let source = source(&downloads)?;
    let mut previous = setup.snapshot()?;
    previous.usb_review = None;
    previous.bit_locker = None;
    setup.begin(previous.kind.as_deref().unwrap_or("direct"), "running")?;
    setup.update(&app, |inner| {
        inner.snapshot.kind = previous.kind.clone();
        inner.snapshot.stage = "windows-encryption".into();
        inner.snapshot.bit_locker = None;
    });
    let service = setup.inner().clone();
    std::thread::spawn(move || {
        let result = run_elevated(
            &service,
            &app,
            OperationRequest {
                protocol: 1,
                source,
                destination: Destination::PrepareBitLocker { reminder_only },
            },
        );
        service.update(&app, |inner| {
            inner.snapshot = previous;
            inner.writer = None;
            match result {
                Ok(result) => {
                    inner.snapshot.bit_locker = Some(result);
                    inner.snapshot.error = None;
                }
                Err(error) => inner.snapshot.error = Some(error),
            }
        });
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

/// Show the staging plan in the app, like the USB review, and wait for the
/// user's answer. The helper gives up after 15 minutes, so the wait does too.
/// `review` is the helper's structured review, drawn beside the summary.
fn review_staged_write(
    service: &Setup,
    app: &tauri::AppHandle,
    summary: &str,
    review: &Value,
) -> bool {
    let (sender, receiver) = std::sync::mpsc::channel();
    service.update(app, |inner| {
        inner.staged_review = Some(sender);
        inner.snapshot.staged_review = Some(json!({"summary": summary, "review": review}));
        inner.snapshot.cancel_available = false;
    });
    let approved = receiver
        .recv_timeout(std::time::Duration::from_secs(900))
        .unwrap_or(false);
    // An approved review stays, marked approved, until the operation ends, so
    // the app shows preparation where the review was, even after a reload.
    service.update(app, |inner| {
        inner.staged_review = None;
        inner.snapshot.staged_review =
            approved.then(|| json!({"summary": summary, "review": review, "approved": true}));
    });
    approved
}

#[tauri::command]
pub fn respond_staged_review(approved: bool, setup: State<'_, Setup>) -> Result<Snapshot, String> {
    let sender = setup
        .inner
        .lock()
        .map_err(|_| "Setup state is unavailable")?
        .staged_review
        .take()
        .ok_or("No installer preparation is waiting for review")?;
    sender
        .send(approved)
        .map_err(|_| "The review is no longer active")?;
    setup.snapshot()
}

/// Asks for administrator access once, right after the window opens, so
/// later operations need no prompt of their own. Windows only. Skipped while
/// a session exists or an operation holds it (it then has one, or is
/// launching one and will share it).
#[tauri::command]
pub fn start_admin_session(
    app: tauri::AppHandle,
    setup: State<'_, Setup>,
) -> Result<Snapshot, String> {
    if KEEP_SESSION {
        let service = setup.inner().clone();
        std::thread::spawn(move || {
            let Ok(mut session) = service.session.try_lock() else {
                return;
            };
            if session.is_some() {
                return;
            }
            service.update(&app, |inner| inner.snapshot.admin = Some("asking".into()));
            let admin = match elevation::launch() {
                Ok(connection) => {
                    *session = Some(Session::new(connection));
                    "ready"
                }
                Err(_) => "declined",
            };
            service.update(&app, |inner| inner.snapshot.admin = Some(admin.into()));
        });
    }
    setup.snapshot()
}

/// Checks disks in the background as soon as the session is up and the
/// image's identity is known, so Install without USB has results waiting.
/// Never prompts: without a session, or while an operation holds it, it does
/// nothing. Testing builds only, like the flow it serves.
#[tauri::command]
pub fn staged_background_check(
    app: tauri::AppHandle,
    setup: State<'_, Setup>,
    downloads: State<'_, Downloads>,
) -> Result<Snapshot, String> {
    if !(KEEP_SESSION && crate::direct_install_policy::STAGED_ISO_TESTING) {
        return setup.snapshot();
    }
    let Ok(source) = identity_source(&downloads) else {
        return setup.snapshot();
    };
    let service = setup.inner().clone();
    std::thread::spawn(move || {
        let Ok(mut session) = service.session.try_lock() else {
            return;
        };
        if !session
            .as_ref()
            .is_some_and(|session| session.alive.load(Ordering::Relaxed))
        {
            return;
        }
        let generation = {
            let Ok(mut check) = service.check.lock() else {
                return;
            };
            check.generation += 1;
            check.status = "running".into();
            check.result = None;
            check.message = "Checking disks…".into();
            check.identity = source.sha256.clone();
            check.generation
        };
        let request = OperationRequest {
            protocol: 1,
            source,
            destination: Destination::StagedIso {
                action: crate::setup_protocol::StagedAction::Inspect,
                selection: None,
                operation_id: None,
                resize: None,
            },
        };
        let mut finished = false;
        let outcome = exchange(
            &service,
            &app,
            request,
            Prepared {
                staged_inspection: true,
                usb_approval: None,
                export: None,
                #[cfg(windows)]
                verified_iso: None,
            },
            &uuid::Uuid::new_v4().to_string(),
            session.as_mut().expect("checked above"),
            &mut finished,
            Some((&service.check, generation)),
        );
        if !finished {
            *session = None;
        }
        drop(session);
        if let Ok(mut check) = service.check.lock() {
            finish_check(&mut check, generation, outcome);
        }
    });
    setup.snapshot()
}

/// Records how a background check ended, unless a newer one has started.
fn finish_check(check: &mut Check, generation: u64, outcome: Result<Value, String>) {
    check.writer = None;
    check.operation = None;
    if check.generation != generation {
        return;
    }
    match outcome {
        Ok(result) => {
            check.status = "done".into();
            check.result = Some(result);
            check.message = String::new();
        }
        // Stopped for a foreground operation: what it measured stays.
        Err(_) if check.status == "stopping" => check.status = "stopped".into(),
        Err(error) => {
            check.status = "failed".into();
            check.message = error;
        }
    }
}

/// The image as a disk check needs it: only its identity, so checking can
/// start before the download finishes. Inspection never reads the image.
fn identity_source(downloads: &Downloads) -> Result<SourceImage, String> {
    let release = downloads.release().ok_or("Choose an image first")?;
    Ok(SourceImage {
        path: std::env::current_exe().map_err(|e| e.to_string())?,
        file_name: release.file_name().into(),
        length: release.length(),
        sha256: release.sha256().into(),
        signature: vec![],
    })
}

/// Asks a running background check to stop before a foreground operation
/// takes the session; the operation then waits for the measurement in
/// progress to finish.
fn stop_background_check(service: &Setup, app: &tauri::AppHandle) {
    let Ok(mut check) = service.check.lock() else {
        return;
    };
    if check.status != "running" {
        return;
    }
    check.status = "stopping".into();
    if let (Some(writer), Some(operation)) = (&check.writer, &check.operation) {
        if let Ok(mut writer) = writer.lock() {
            let _ = write_frame(
                &mut *writer,
                &json!({"protocol":1,"type":"cancel","operation":operation}),
            );
        }
    }
    drop(check);
    service.update(app, |inner| {
        inner.snapshot.message = "Finishing the disk check…".into()
    });
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
            &json!({"protocol":1,"type":"cancel","operation":inner.operation}),
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

/// The firmware confirmation promises the app reopens; when the reopen entry
/// couldn't be written, it says to open the app instead.
fn firmware_summary(summary: &str, reopens: bool) -> String {
    if reopens {
        summary.to_owned()
    } else {
        summary.replace(
            crate::setup_helper::FIRMWARE_REOPENS,
            "Windows starts again; open this app yourself then.",
        )
    }
}

fn run_elevated(
    service: &Setup,
    app: &tauri::AppHandle,
    request: OperationRequest,
) -> Result<Value, String> {
    // Before the prompt, as before sessions: choosing a records folder may
    // show a dialog.
    let prepared = Prepared {
        staged_inspection: matches!(
            &request.destination,
            Destination::StagedIso {
                action: crate::setup_protocol::StagedAction::Inspect,
                ..
            }
        ),
        usb_approval: crate::setup_protocol::usb_confirmation_plan(&request)?,
        export: crate::operation_records::Export::choose(app, &request.destination)?,
        #[cfg(windows)]
        verified_iso: app.state::<Downloads>().verified_lease(&request.source),
    };
    stop_background_check(service, app);
    let mut session = acquire_session(&service.session, elevation::launch)?;
    service.update(app, |inner| inner.snapshot.admin = Some("ready".into()));
    let operation_id = uuid::Uuid::new_v4().to_string();
    // A result or error from the helper ends the operation normally; any
    // other ending leaves the session in an unknown state, so it is dropped.
    let mut finished = false;
    let outcome = exchange(
        service,
        app,
        request,
        prepared,
        &operation_id,
        session.as_mut().ok_or("Helper session is unavailable")?,
        &mut finished,
        None,
    );
    if !finished || !KEEP_SESSION {
        *session = None;
        service.update(app, |inner| {
            if inner.snapshot.admin.as_deref() == Some("ready") {
                inner.snapshot.admin = None;
            }
        });
    }
    outcome
}

/// What an operation prepares in the desktop before the helper runs it.
struct Prepared {
    staged_inspection: bool,
    usb_approval: Option<Value>,
    export: Option<crate::operation_records::Export>,
    #[cfg(windows)]
    verified_iso: Option<Arc<crate::iso_source::HeldIso>>,
}

fn exchange(
    service: &Setup,
    app: &tauri::AppHandle,
    request: OperationRequest,
    prepared: Prepared,
    operation_id: &str,
    session: &mut Session,
    finished: &mut bool,
    // A background check reports to its own slot (at this generation),
    // never the main snapshot.
    background: Option<(&Mutex<Check>, u64)>,
) -> Result<Value, String> {
    let Prepared {
        staged_inspection,
        usb_approval,
        mut export,
        #[cfg(windows)]
        verified_iso,
    } = prepared;
    let mut usb_approval_used = false;
    let mut exported_path = None;
    let mut export_error = None;
    let writer = Arc::clone(&session.writer);
    let mut reopen: Option<String> = None;
    let mut restarting = false;
    let mut envelope = serde_json::to_value(&request).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    if let Some(guard) = &verified_iso {
        envelope["sourceVerification"] = guard.verified_binding(&request.source)?;
    }
    if let Some(export) = &export {
        envelope["recordsDirectory"] = json!(export.directory);
    }
    write_frame(
        &mut *writer
            .lock()
            .map_err(|_| "Helper connection is unavailable")?,
        &json!({"protocol":1,"type":"request","operation":operation_id,"request":envelope}),
    )?;
    if let Some((check, _)) = background {
        if let Ok(mut check) = check.lock() {
            check.writer = Some(Arc::clone(&writer));
            check.operation = Some(operation_id.into());
        }
    } else {
        service.update(app, |inner| {
            inner.writer = Some(Arc::clone(&writer));
            inner.operation = Some(operation_id.into());
            inner.snapshot.cancel_available = true;
        });
    }
    let reader = &mut session.reader;
    while let Some(value) = read_frame(reader)? {
        if let Some((check, generation)) = background {
            match value["type"].as_str() {
                Some("event") => {
                    if let Ok(mut check) = check.lock() {
                        if check.generation == generation {
                            if value["stagedIsoInspection"]["disks"].is_array() {
                                check.result = Some(value["stagedIsoInspection"].clone());
                            }
                            if let Some(message) = value["message"].as_str() {
                                check.message = message.into();
                            }
                        }
                    }
                    continue;
                }
                // A disk check asks for nothing; refuse anything else.
                Some("confirmation_required") => {
                    write_frame(
                        &mut *writer
                            .lock()
                            .map_err(|_| "Helper connection is unavailable")?,
                        &json!({"protocol":1,"type":"confirmation","operation":operation_id,"binding":value["binding"],"approved":false}),
                    )?;
                    continue;
                }
                _ => {}
            }
        }
        // The restart is under way: keep the reopen entry from here on.
        restarting |= value["type"] == "event" && value["stage"] == "restarting";
        match value["type"].as_str() {
            Some("confirmation_required") => {
                let binding = value["binding"]
                    .as_str()
                    .ok_or("The helper omitted the confirmation binding")?;
                let summary = value["summary"]
                    .as_str()
                    .ok_or("The helper omitted the confirmation summary")?;
                let operation = value["plan"]["kind"].as_str().unwrap_or("");
                // Reopen the app after the firmware restart. Written before the
                // dialog, since the helper restarts right after approval; if it
                // can't be written, the dialog says so and the restart goes on.
                let summary = if operation == "staged_firmware" {
                    reopen = crate::startup::reopen_at_sign_in().ok();
                    firmware_summary(summary, reopen.is_some())
                } else {
                    summary.to_owned()
                };
                let summary = summary.as_str();
                let (dialog_kind, button) = match operation {
                    "usb_preserve" => (MessageDialogKind::Info, "Keep files and add installer"),
                    "usb" => (MessageDialogKind::Warning, "Erase USB and create installer"),
                    "bitlocker_reminder" => (MessageDialogKind::Info, "Set restoration reminder"),
                    "staged_write" => (MessageDialogKind::Warning, "Prepare installer partitions"),
                    "staged_arm" => (MessageDialogKind::Info, "Select next startup"),
                    "staged_restart" => (MessageDialogKind::Warning, "Restart now"),
                    "staged_firmware" => {
                        (MessageDialogKind::Warning, "Restart to firmware settings")
                    }
                    "staged_cleanup" => (MessageDialogKind::Warning, "Remove temporary installer"),
                    _ => (MessageDialogKind::Warning, "Continue"),
                };
                let approved = if operation == "source_lease" {
                    #[cfg(windows)]
                    {
                        verified_iso.as_ref().is_some_and(|guard| {
                            guard
                                .verified_binding(&request.source)
                                .is_ok_and(|expected| {
                                    expected == value["plan"]["sourceVerification"]
                                })
                        })
                    }
                    #[cfg(not(windows))]
                    {
                        false
                    }
                } else if operation == "record_export" {
                    match export
                        .as_mut()
                        .ok_or("No records destination was prepared".to_owned())
                        .and_then(|export| export.save(&value["plan"]))
                    {
                        Ok(path) => {
                            exported_path = Some(path);
                            true
                        }
                        Err(error) => {
                            export_error = Some(error);
                            false
                        }
                    }
                } else if operation == "staged_write" {
                    review_staged_write(service, app, summary, &value["review"])
                } else if let Some(expected) = &usb_approval {
                    consume_usb_confirmation(expected, &value["plan"], &mut usb_approval_used)?;
                    !service
                        .inner
                        .lock()
                        .map_err(|_| "Setup state is unavailable")?
                        .cancel
                        .load(Ordering::Relaxed)
                } else {
                    let window = app
                        .get_webview_window("main")
                        .ok_or("The installer window is unavailable for confirmation")?;
                    // UAC may have moved focus away while the helper started.
                    // Keep the confirmation owned by the restored app window,
                    // rather than creating an independent dialog behind it.
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    app.dialog()
                        .message(summary)
                        .title("Confirm Omarchy setup")
                        .parent(&window)
                        .kind(dialog_kind)
                        .buttons(MessageDialogButtons::OkCancelCustom(
                            button.into(),
                            "Cancel".into(),
                        ))
                        .blocking_show()
                };
                if !approved && operation == "staged_firmware" {
                    if let Some(command) = reopen.take() {
                        crate::startup::cancel_reopen(&command);
                    }
                }
                // The app shows a staged action as under way once confirmed.
                if approved
                    && matches!(
                        operation,
                        "staged_arm" | "staged_restart" | "staged_firmware" | "staged_cleanup"
                    )
                {
                    service.update(app, |inner| {
                        if let Some(target) = inner.snapshot.staged_target.as_mut() {
                            target["confirmed"] = json!(true);
                        }
                    });
                }
                write_frame(
                    &mut *writer
                        .lock()
                        .map_err(|_| "Helper connection is unavailable")?,
                    &json!({"protocol":1,"type":"confirmation","operation":operation_id,"binding":binding,"approved":approved}),
                )?;
            }
            Some("event") => service.update(app, |inner| {
                if staged_inspection
                    && value["stagedIsoInspection"]["disks"].is_array()
                    && value["stagedIsoInspection"]["choices"].is_array()
                    && value["stagedIsoInspection"]["blocked"].is_array()
                    && value["stagedIsoInspection"]["minimumLinuxBytes"].is_u64()
                    && value["stagedIsoInspection"]["temporaryBytes"].is_u64()
                {
                    inner.snapshot.staged_iso = Some(value["stagedIsoInspection"].clone());
                }
                if value["recovery"].is_object() {
                    inner.snapshot.recovery = if value["recovery"]["cleanup"]["workspaceRemoved"]
                        == true
                        && exported_path.is_none()
                    {
                        None
                    } else {
                        Some(value["recovery"].clone())
                    };
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
                        "writing" => "Writing the image…",
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
                if value["steps"].is_array() {
                    inner.snapshot.steps = Some(value["steps"].clone());
                }
                if let Some(step) = value["step"].as_str() {
                    inner.snapshot.step = Some(step.into());
                }
            }),
            Some("result" | "error") if value["operation"] != operation_id => {
                return Err("The helper answered for another operation".into());
            }
            Some("result") => {
                *finished = true;
                let mut result = value["result"].clone();
                // Secure Boot was already off, so the PC doesn't restart.
                if let Some(command) = reopen.as_deref().filter(|_| result["secureBoot"] == false) {
                    crate::startup::cancel_reopen(command);
                }
                if let Some(path) = &exported_path {
                    result["receiptPath"] = json!(path);
                    service.update(app, |inner| {
                        if result["workspaceRemoved"] == true {
                            if let Some(recovery) = &mut inner.snapshot.recovery {
                                recovery["filesPath"] = json!(path.parent());
                                recovery["message"] = json!("Temporary operation files were removed. Records are saved beside the installer or in your chosen folder.");
                            }
                        }
                    });
                }
                if let Some(error) = export_error {
                    result["recordWarning"] = json!(format!("The operation completed, but records could not be exported: {error}. The protected originals have been retained."));
                }
                return Ok(result);
            }
            Some("error") => {
                *finished = true;
                // A failure before the restart began means the PC won't
                // restart. A lost connection keeps the entry: that is
                // usually the restart.
                if let Some(command) = reopen.as_deref().filter(|_| !restarting) {
                    crate::startup::cancel_reopen(command);
                }
                return Err(value["message"]
                    .as_str()
                    .unwrap_or("Installation failed")
                    .into());
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
    fn connection() -> elevation::Connection {
        elevation::Connection {
            reader: Box::new(std::io::empty()),
            writer: Box::new(std::io::sink()),
            alive: Arc::new(AtomicBool::new(true)),
        }
    }
    #[test]
    fn a_background_check_keeps_its_own_results_and_drops_stale_ones() {
        let mut check = Check {
            generation: 2,
            status: "running".into(),
            result: Some(json!({"disks":["partial"]})),
            ..Check::default()
        };
        // A check replaced by a newer one changes nothing.
        finish_check(&mut check, 1, Ok(json!({"disks":["old"]})));
        assert_eq!(
            (check.status.as_str(), &check.result),
            ("running", &Some(json!({"disks":["partial"]})))
        );
        // Stopped for a foreground operation: the partial result stays.
        check.status = "stopping".into();
        finish_check(&mut check, 2, Err("The disk check was stopped.".into()));
        assert_eq!(
            (check.status.as_str(), &check.result),
            ("stopped", &Some(json!({"disks":["partial"]})))
        );
        check.status = "running".into();
        finish_check(&mut check, 2, Ok(json!({"disks":["all"]})));
        assert_eq!(
            (check.status.as_str(), &check.result),
            ("done", &Some(json!({"disks":["all"]})))
        );
        check.status = "running".into();
        finish_check(&mut check, 2, Err("Windows failed".into()));
        assert_eq!(
            (check.status.as_str(), check.message.as_str()),
            ("failed", "Windows failed")
        );
        // The main snapshot never holds it; it's shown beside.
        assert_eq!(check.view().unwrap()["status"], "failed");
        assert!(Check::default().view().is_none());
    }

    #[test]
    fn overlapping_requests_share_one_prompt_and_a_lost_session_is_replaced() {
        let holder = Arc::new(Mutex::new(None));
        let launches = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let threads: Vec<_> = (0..2)
            .map(|_| {
                let (holder, launches) = (Arc::clone(&holder), Arc::clone(&launches));
                std::thread::spawn(move || {
                    let guard = acquire_session(&holder, || {
                        launches.fetch_add(1, Ordering::Relaxed);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        Ok(connection())
                    })
                    .unwrap();
                    assert!(guard.is_some());
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(launches.load(Ordering::Relaxed), 1);
        // A declined prompt leaves no session, so the next action asks again.
        *holder.lock().unwrap() = None;
        assert!(acquire_session(&holder, || Err("declined".into())).is_err());
        assert!(holder.lock().unwrap().is_none());
        drop(acquire_session(&holder, || Ok(connection())).unwrap());
        assert!(holder.lock().unwrap().is_some());
        // A helper that exited between operations is replaced, not reused.
        let before = launches.load(Ordering::Relaxed);
        holder
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .alive
            .store(false, Ordering::Relaxed);
        let guard = acquire_session(&holder, || {
            launches.fetch_add(1, Ordering::Relaxed);
            Ok(connection())
        })
        .unwrap();
        assert!(guard.as_ref().unwrap().alive.load(Ordering::Relaxed));
        assert_eq!(launches.load(Ordering::Relaxed), before + 1);
    }
    #[test]
    fn a_stopped_disk_check_is_cancelled_and_keeps_what_it_measured() {
        let setup = Setup::default();
        let mut inner = setup.inner.lock().unwrap();
        inner.snapshot.status = "running".into();
        inner.snapshot.staged_iso = Some(json!({"disks":[{"diskNumber":0}]}));
        inner.snapshot.staged_target = Some(json!({"action":"inspect","operationId":null}));
        inner.cancel.store(true, Ordering::Relaxed);
        record_failure(&mut inner, "The disk check was stopped.".into());
        assert_eq!(inner.snapshot.status, "cancelled");
        assert_eq!(inner.snapshot.error, None);
        assert_eq!(inner.snapshot.message, "Disk check stopped.");
        assert_eq!(
            inner.snapshot.staged_iso,
            Some(json!({"disks":[{"diskNumber":0}]}))
        );
        // A failed check without Stop is still a failure.
        inner.cancel.store(false, Ordering::Relaxed);
        record_failure(&mut inner, "Windows failed".into());
        assert_eq!(inner.snapshot.status, "failed");
        // Stopping a preparation keeps its error, so removal is offered.
        inner.snapshot.staged_target = Some(json!({"action":"stage","operationId":null}));
        inner.cancel.store(true, Ordering::Relaxed);
        record_failure(&mut inner, "Preparation was cancelled.".into());
        assert_eq!(inner.snapshot.status, "cancelled");
        assert!(inner.snapshot.error.is_some());
    }

    #[test]
    fn each_operation_starts_without_the_previous_steps() {
        let setup = Setup::default();
        {
            let mut inner = setup.inner.lock().unwrap();
            inner.snapshot.steps =
                Some(json!([{"id":"verify","label":"Verify the installer image"}]));
            inner.snapshot.step = Some("copy".into());
            inner.snapshot.status = "complete".into();
        }
        setup.begin("direct", "running").unwrap();
        let inner = setup.inner.lock().unwrap();
        assert_eq!(inner.snapshot.steps, None);
        assert_eq!(inner.snapshot.step, None);
    }

    #[test]
    fn the_firmware_confirmation_says_when_the_app_cannot_reopen() {
        let summary = crate::setup_helper::FIRMWARE_SUMMARY;
        assert!(summary.contains(crate::setup_helper::FIRMWARE_REOPENS));
        assert_eq!(firmware_summary(summary, true), summary);
        let fallback = firmware_summary(summary, false);
        assert!(!fallback.contains("this app reopens"));
        assert!(fallback.contains("open this app yourself"));
    }

    #[test]
    fn a_declined_staged_confirmation_is_cancelled_without_an_error() {
        let setup = Setup::default();
        {
            let mut inner = setup.inner.lock().unwrap();
            inner.snapshot.status = "running".into();
            inner.snapshot.staged_iso = Some(json!({"disks":[]}));
            // Review, or any other confirmation of a staged action.
            inner.snapshot.staged_target = Some(json!({"action":"firmware","operationId":"id"}));
            // As the helper reports it, with its operation files.
            let reported = crate::setup_helper::operation_failure(
                crate::setup_helper::NOT_CONFIRMED,
                std::path::Path::new("C:\\ProgramData\\operation"),
            );
            record_failure(&mut inner, reported);
            assert_eq!(inner.snapshot.status, "cancelled");
            assert_eq!(inner.snapshot.error, None);
            assert_eq!(inner.snapshot.message, "Nothing was changed.");
            assert_eq!(inner.snapshot.staged_iso, Some(json!({"disks":[]})));
            // Any other error after a declined review is still a failure.
            record_failure(&mut inner, "Disk changed".into());
            assert_eq!(inner.snapshot.status, "failed");
            assert_eq!(inner.snapshot.error.as_deref(), Some("Disk changed"));
            record_failure(
                &mut inner,
                "Copy failed\nOperation was not confirmed".into(),
            );
            assert_eq!(inner.snapshot.status, "failed");
            // An approved review shown during preparation ends with the operation.
            inner.snapshot.staged_review = Some(json!({"summary":"Prepare?","approved":true}));
            record_failure(&mut inner, "Copy failed".into());
            assert_eq!(inner.snapshot.staged_review, None);
            inner.snapshot.staged_review = Some(json!({"summary":"Prepare?","approved":true}));
            inner.snapshot.status = "failed".into();
        }
        // A new operation forgets the approved review and the previous staged
        // target, so a declined confirmation outside a staged operation (USB)
        // stays a failure.
        setup.begin("direct", "running").unwrap();
        let mut inner = setup.inner.lock().unwrap();
        assert_eq!(inner.snapshot.staged_review, None);
        assert_eq!(inner.snapshot.staged_target, None);
        record_failure(&mut inner, crate::setup_helper::NOT_CONFIRMED.into());
        assert_eq!(inner.snapshot.status, "failed");
    }
    #[test]
    fn passive_staging_discovery_handles_absent_and_malformed_summaries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("summary.json");
        assert!(read_staging_summary(&path).is_none());
        std::fs::write(&path, b"broken").unwrap();
        assert!(read_staging_summary(&path).is_none());
        let summary = json!({"operations":[{"operationId":uuid::Uuid::new_v4().to_string(),"status":"copying","diskNumber":0,"temporaryBytes":8589934592_u64}],"recordErrors":[]});
        std::fs::write(&path, serde_json::to_vec(&summary).unwrap()).unwrap();
        assert_eq!(read_staging_summary(&path), Some(summary.clone()));
        std::fs::write(&path, vec![b' '; 1_048_577]).unwrap();
        assert!(read_staging_summary(&path).is_none());
        // The partition to delete in the installer is short text, if present.
        let mut replacing = summary.clone();
        replacing["operations"][0]["replaces"] = json!("Windows");
        std::fs::write(&path, serde_json::to_vec(&replacing).unwrap()).unwrap();
        assert_eq!(read_staging_summary(&path), Some(replacing.clone()));
        for invalid in [json!(7), json!("x".repeat(65))] {
            replacing["operations"][0]["replaces"] = invalid;
            std::fs::write(&path, serde_json::to_vec(&replacing).unwrap()).unwrap();
            assert!(read_staging_summary(&path).is_none());
        }
    }
    #[test]
    fn scheduled_operations_are_marked_once_windows_restarts() {
        let mut summary = json!({"operations":[
            {"status":"boot-scheduled","scheduledBootUnixSeconds":1_000_u64},
            {"status":"staged"}
        ]});
        mark_restarts(&mut summary, Some(1_060));
        assert_eq!(
            summary["operations"][0]["restartedSinceScheduled"],
            json!(false)
        );
        mark_restarts(&mut summary, Some(5_000));
        assert_eq!(
            summary["operations"][0]["restartedSinceScheduled"],
            json!(true)
        );
        assert!(summary["operations"][1]
            .get("restartedSinceScheduled")
            .is_none());
        let mut unknown = json!({"operations":[{"status":"boot-scheduled","scheduledBootUnixSeconds":1_000_u64}]});
        mark_restarts(&mut unknown, None);
        assert!(unknown["operations"][0]
            .get("restartedSinceScheduled")
            .is_none());
    }
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
