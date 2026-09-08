//! Persistent, unprivileged client for the pinned Apple native coordinator.
//! The webview supplies intents and a state revision, never credentials, paths,
//! process arguments, disk extents, signing requirements or approval objects.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};
#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::{AppHandle, State};

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct AppleSetupAction {
    pub expected_revision: u64,
    pub intent: AppleSetupIntent,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum AppleSetupIntent {
    Connect,
    Inspect,
    PreparePlan {
        #[serde(default)]
        allocation_bytes: Option<String>,
    },
    ChooseStorage {
        choice_id: String,
        #[serde(default)]
        allocation_bytes: Option<String>,
        #[serde(default)]
        confirmation: Option<String>,
    },
    ReviewPlan,
    ApprovePlan,
    Execute,
    RetryRecovery,
    Cancel,
    Refresh,
}

impl AppleSetupIntent {
    #[cfg(target_os = "macos")]
    fn name(&self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::Inspect => "inspect",
            Self::PreparePlan { .. } => "prepare_plan",
            Self::ChooseStorage { .. } => "choose_storage",
            Self::ReviewPlan => "review_plan",
            Self::ApprovePlan => "approve_plan",
            Self::Execute => "execute",
            Self::RetryRecovery => "retry_recovery",
            Self::Cancel => "cancel",
            Self::Refresh => "refresh",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AppleSetupError {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AppleSetupSnapshot {
    pub revision: u64,
    pub status: String,
    /// The authenticated companion is connected. Inspect native/probe gates
    /// separately; this does not mean this Mac can install.
    pub available: bool,
    pub host_os: &'static str,
    pub host_architecture: &'static str,
    pub busy: bool,
    pub pending_intent: Option<String>,
    pub probe: Option<Value>,
    pub native: Option<Value>,
    /// Retained display-only plan, including during installation and Recovery.
    pub plan: Option<Value>,
    pub error: Option<AppleSetupError>,
    pub outcome_unknown: bool,
}

struct Inner {
    snapshot: AppleSetupSnapshot,
    #[cfg(target_os = "macos")]
    generation: u64,
    #[cfg(target_os = "macos")]
    marker_checked: bool,
    #[cfg(target_os = "macos")]
    connection: Option<Arc<native::Connection>>,
    #[cfg(target_os = "macos")]
    approved_digest: Option<String>,
}

#[derive(Clone)]
pub struct AppleSetupService {
    inner: Arc<Mutex<Inner>>,
}

impl Default for AppleSetupService {
    fn default() -> Self {
        let supported_host = cfg!(all(target_os = "macos", target_arch = "aarch64"));
        Self {
            inner: Arc::new(Mutex::new(Inner {
                snapshot: AppleSetupSnapshot {
                    revision: 0,
                    status: if supported_host { "disconnected" } else { "unavailable" }.into(),
                    available: false,
                    host_os: std::env::consts::OS,
                    host_architecture: std::env::consts::ARCH,
                    busy: false,
                    pending_intent: None,
                    probe: None,
                    native: None,
                    plan: None,
                    error: (!supported_host).then(|| AppleSetupError {
                        code: "unsupported_host".into(),
                        message: "This native installation provider requires Apple Silicon macOS 15 or later. Intel Macs and Rosetta are not supported.".into(),
                    }),
                    outcome_unknown: false,
                },
                #[cfg(target_os = "macos")]
                generation: 0,
                #[cfg(target_os = "macos")]
                marker_checked: false,
                #[cfg(target_os = "macos")]
                connection: None,
                #[cfg(target_os = "macos")]
                approved_digest: None,
            })),
        }
    }
}

impl AppleSetupService {
    pub fn snapshot(&self) -> Result<AppleSetupSnapshot, String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| "Apple installation state is unavailable")?
            .snapshot
            .clone())
    }

    /// The enclosing app can use this for its close/exit confirmation. Closing
    /// the desktop is never implemented here as cancellation of root disk work.
    pub(crate) fn active_operation(&self) -> bool {
        self.inner
            .lock()
            .map(|inner| {
                inner.snapshot.busy
                    || inner
                        .snapshot
                        .native
                        .as_ref()
                        .and_then(|native| native.get("busy"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
            })
            .unwrap_or(true)
    }

    #[cfg(target_os = "macos")]
    fn update(&self, app: &AppHandle, change: impl FnOnce(&mut Inner)) {
        let snapshot = match self.inner.lock() {
            Ok(mut inner) => {
                change(&mut inner);
                inner.snapshot.revision += 1;
                inner.snapshot.clone()
            }
            Err(_) => return,
        };
        let _ = app.emit("apple-setup-state", &snapshot);
    }

    #[cfg(target_os = "macos")]
    fn check_previous_operation(&self, app: &AppHandle) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Apple installation state is unavailable")?;
        if inner.marker_checked {
            return Ok(());
        }
        let marker = native::marker_path(app)?;
        let marker_exists = marker
            .try_exists()
            .map_err(|_| "Cannot inspect the Apple operation record")?;
        inner.marker_checked = true;
        if marker_exists {
            inner.snapshot.outcome_unknown = true;
            inner.snapshot.status = "unknown".into();
            inner.snapshot.error = Some(AppleSetupError {
                code: "unsettled_previous_operation".into(),
                message: "A previous native installation has no recorded completion in this app. Review the upstream trusted journal and Recovery state before starting another installation. The app will not retry automatically.".into(),
            });
            inner.snapshot.revision += 1;
        }
        Ok(())
    }
}

#[tauri::command]
pub fn apple_setup_status(
    app: AppHandle,
    service: State<'_, AppleSetupService>,
) -> Result<AppleSetupSnapshot, String> {
    #[cfg(target_os = "macos")]
    service.check_previous_operation(&app)?;
    #[cfg(not(target_os = "macos"))]
    let _ = app;
    service.snapshot()
}

#[tauri::command]
pub fn apple_setup_action(
    app: AppHandle,
    service: State<'_, AppleSetupService>,
    action: AppleSetupAction,
) -> Result<AppleSetupSnapshot, String> {
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        if app.state::<crate::setup::Setup>().active_operation() {
            return Err("Wait for the USB or other installation operation to finish before using the Apple installer".into());
        }
        service.check_previous_operation(&app)?;
        native::start_action(app, service.inner().clone(), action)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, action);
        Err(service
            .snapshot()?
            .error
            .map(|error| error.message)
            .unwrap_or_else(|| "Apple native installation is unavailable on this host".into()))
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use std::fs::{self, OpenOptions};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::PathBuf;
    use std::process::{ChildStdin, Command, Stdio};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{self, Sender};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tauri::Manager;
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    const SOURCE_COMMIT: &str = "00daf3ebef9e4fbe89fb9b32bd184e65aa75d357";
    const BRIDGE_ID: &str = "community.omarchy.setup.apple-bridge";
    const MAX_OUTPUT_FRAME: usize = 256 * 1024;
    const MAX_REQUEST_FRAME: usize = 65_536;

    struct Pending {
        reply: Sender<Result<Value, String>>,
        mutating: bool,
    }

    pub(super) struct Connection {
        stdin: Mutex<Option<ChildStdin>>,
        pending: Mutex<HashMap<String, Pending>>,
        sequence: AtomicU64,
        alive: AtomicBool,
        exited: AtomicBool,
        mutation_pending: AtomicBool,
        generation: u64,
    }

    impl Connection {
        fn call<T: Serialize>(
            &self,
            command: &'static str,
            parameters: &T,
            mutating: bool,
            timeout: Option<Duration>,
        ) -> Result<Value, String> {
            if !self.alive.load(Ordering::SeqCst) {
                return Err("The native companion is disconnected".into());
            }
            let id = format!("rust-{}", self.sequence.fetch_add(1, Ordering::SeqCst));
            #[derive(Serialize)]
            struct Request<'a, T: Serialize> {
                version: u8,
                id: &'a str,
                command: &'a str,
                params: &'a T,
            }
            let mut bytes = serde_json::to_vec(&Request {
                version: 1,
                id: &id,
                command,
                params: parameters,
            })
            .map_err(|_| "The native request could not be encoded")?;
            if bytes.len() > MAX_REQUEST_FRAME {
                bytes.fill(0);
                return Err("The native request is too large".into());
            }
            bytes.push(b'\n');
            let (sender, receiver) = mpsc::channel();
            self.pending
                .lock()
                .map_err(|_| "Native reply state is unavailable")?
                .insert(
                    id.clone(),
                    Pending {
                        reply: sender,
                        mutating,
                    },
                );
            if mutating {
                self.mutation_pending.store(true, Ordering::SeqCst);
            }
            let sent = self
                .stdin
                .lock()
                .map_err(|_| "Native input is unavailable")
                .and_then(|mut input| {
                    let writer = input
                        .as_mut()
                        .ok_or("The native companion input is closed")?;
                    writer
                        .write_all(&bytes)
                        .and_then(|_| writer.flush())
                        .map_err(|_| "The native companion disconnected while receiving a request")
                });
            bytes.fill(0);
            if let Err(error) = sent {
                self.close(error);
                return Err(error.into());
            }
            let result = if let Some(timeout) = timeout {
                receiver.recv_timeout(timeout).map_err(|_| {
                    "The native companion did not finish the request in time".to_string()
                })
            } else {
                receiver.recv().map_err(|_| {
                    "The native companion disconnected before completing the request".to_string()
                })
            };
            match result {
                Ok(reply) => reply,
                Err(error) => {
                    self.close(&error);
                    Err(error)
                }
            }
        }

        fn close(&self, message: &str) {
            self.alive.store(false, Ordering::SeqCst);
            if let Ok(mut input) = self.stdin.lock() {
                input.take();
            }
            if let Ok(mut pending) = self.pending.lock() {
                for (_, request) in pending.drain() {
                    let _ = request.reply.send(Err(message.to_owned()));
                }
            }
            // Never send a signal or kill a submitted helper/companion.
        }
    }

    pub(super) fn marker_path(app: &AppHandle) -> Result<PathBuf, String> {
        Ok(app
            .path()
            .app_local_data_dir()
            .map_err(|_| "Apple operation storage is unavailable")?
            .join("apple-installation-in-flight.json"))
    }

    pub(super) fn start_action(
        app: AppHandle,
        service: AppleSetupService,
        action: AppleSetupAction,
    ) -> Result<AppleSetupSnapshot, String> {
        if !cfg!(target_arch = "aarch64") {
            return Err("Use the native Apple Silicon build of Omarchy Installer".into());
        }
        let accepted = {
            let mut inner = service
                .inner
                .lock()
                .map_err(|_| "Apple installation state is unavailable")?;
            if action.expected_revision != inner.snapshot.revision {
                return Err("The Apple installation state changed. Review the current state before continuing.".into());
            }
            if inner.snapshot.busy {
                return Err("Another Apple installation action is still running".into());
            }
            if inner.snapshot.outcome_unknown {
                return Err("The previous installation outcome is uncertain. Review its trusted journal and Recovery state before proceeding.".into());
            }
            validate_intent(&inner, &action.intent)?;
            inner.snapshot.busy = true;
            inner.snapshot.pending_intent = Some(action.intent.name().into());
            inner.snapshot.status = if matches!(&action.intent, AppleSetupIntent::Connect) {
                "connecting"
            } else {
                "busy"
            }
            .into();
            inner.snapshot.error = None;
            inner.snapshot.revision += 1;
            inner.snapshot.clone()
        };
        let _ = app.emit("apple-setup-state", &accepted);
        std::thread::spawn(move || {
            let outcome = perform(&app, &service, action.intent);
            service.update(&app, |inner| {
                inner.snapshot.busy = false;
                inner.snapshot.pending_intent = None;
                if let Err(message) = outcome {
                    inner.snapshot.error = Some(AppleSetupError {
                        code: "action_failed".into(),
                        message,
                    });
                    inner.snapshot.status = if inner.snapshot.outcome_unknown {
                        "unknown"
                    } else {
                        "failed"
                    }
                    .into();
                } else {
                    inner.snapshot.status = "ready".into();
                }
            });
        });
        Ok(accepted)
    }

    fn phase(inner: &Inner) -> &str {
        inner
            .snapshot
            .native
            .as_ref()
            .and_then(|value| value.get("phase"))
            .and_then(Value::as_str)
            .unwrap_or("idle")
    }

    fn flag(inner: &Inner, name: &str) -> bool {
        inner
            .snapshot
            .native
            .as_ref()
            .and_then(|value| value.get(name))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn validate_intent(inner: &Inner, intent: &AppleSetupIntent) -> Result<(), String> {
        if matches!(intent, AppleSetupIntent::Connect) {
            return Ok(());
        }
        if !inner.snapshot.available {
            return Err("Connect the packaged Apple companion first".into());
        }
        let valid = match intent {
            AppleSetupIntent::Connect | AppleSetupIntent::Refresh => true,
            AppleSetupIntent::Inspect => !flag(inner, "hasExecutionStarted"),
            AppleSetupIntent::PreparePlan { allocation_bytes } => {
                if let Some(bytes) = allocation_bytes {
                    if bytes.is_empty()
                        || !bytes.bytes().all(|byte| byte.is_ascii_digit())
                        || bytes
                            .parse::<u64>()
                            .ok()
                            .filter(|value| *value > 0)
                            .is_none()
                    {
                        return Err("Choose a positive whole-byte allocation".into());
                    }
                }
                ["welcome", "plan_prepared", "plan_review"].contains(&phase(inner))
                    && (phase(inner) == "welcome" || allocation_bytes.is_some())
            }
            AppleSetupIntent::ReviewPlan => phase(inner) == "plan_prepared",
            AppleSetupIntent::ChooseStorage {
                choice_id,
                allocation_bytes,
                confirmation,
            } => {
                let choice = inner
                    .snapshot
                    .native
                    .as_ref()
                    .and_then(|native| native.get("storageChoices"))
                    .and_then(Value::as_array)
                    .and_then(|choices| {
                        choices.iter().find(|choice| {
                            choice.get("id").and_then(Value::as_str) == Some(choice_id.as_str())
                        })
                    })
                    .ok_or("Choose storage from the current native inspection")?;
                if let Some(deletion) = choice.get("deletion") {
                    let identifier = deletion
                        .get("identifier")
                        .and_then(Value::as_str)
                        .ok_or("The replacement identifier is unavailable")?;
                    if allocation_bytes.is_some()
                        || !confirmation
                            .as_deref()
                            .is_some_and(|typed| typed.trim().eq_ignore_ascii_case(identifier))
                    {
                        return Err("Type the current installation identifier to confirm its full replacement".into());
                    }
                } else {
                    if confirmation.is_some() {
                        return Err("This choice does not delete an installation".into());
                    }
                    if let Some(bytes) = allocation_bytes {
                        if bytes.is_empty()
                            || !bytes.bytes().all(|byte| byte.is_ascii_digit())
                            || bytes
                                .parse::<u64>()
                                .ok()
                                .filter(|value| *value > 0)
                                .is_none()
                        {
                            return Err("Choose a positive whole-byte allocation".into());
                        }
                    }
                }
                phase(inner) == "existing_install_choice" && !flag(inner, "hasExecutionStarted")
            }
            AppleSetupIntent::ApprovePlan => {
                phase(inner) == "plan_review" && inner.snapshot.plan.is_some()
            }
            AppleSetupIntent::Execute => {
                phase(inner) == "awaiting_install"
                    && flag(inner, "canExecute")
                    && inner.approved_digest.is_some()
            }
            AppleSetupIntent::RetryRecovery => {
                flag(inner, "canRetryRecovery") && inner.approved_digest.is_some()
            }
            AppleSetupIntent::Cancel => flag(inner, "cancelAvailable"),
        };
        if !valid {
            return Err(
                "This action is not available in the current native installation state".into(),
            );
        }
        Ok(())
    }

    fn connection(service: &AppleSetupService) -> Result<Arc<Connection>, String> {
        service
            .inner
            .lock()
            .map_err(|_| "Apple installation state is unavailable")?
            .connection
            .clone()
            .ok_or_else(|| "The Apple companion is not connected".into())
    }

    fn perform(
        app: &AppHandle,
        service: &AppleSetupService,
        intent: AppleSetupIntent,
    ) -> Result<(), String> {
        if matches!(intent, AppleSetupIntent::Connect) {
            let connection = connect(app, service)?;
            let probe =
                connection.call("probe", &json!({}), false, Some(Duration::from_secs(60)))?;
            if probe.get("sourceCommit").and_then(Value::as_str) != Some(SOURCE_COMMIT)
                || probe.get("engineVersion").and_then(Value::as_str) != Some("v0.9.0-omarchy.7")
                || probe.get("protocolVersion").and_then(Value::as_u64) != Some(1)
            {
                connection.close("The packaged Apple bridge does not match this desktop build");
                return Err("The packaged Apple bridge does not match this desktop build".into());
            }
            service.update(app, |inner| {
                inner.snapshot.available = true;
                inner.snapshot.probe = Some(probe);
            });
            return Ok(());
        }
        let connection = connection(service)?;
        match intent {
            AppleSetupIntent::Inspect => {
                clear_plan(app, service);
                connection.call("inspect", &json!({}), false, Some(Duration::from_secs(600)))?;
            }
            AppleSetupIntent::PreparePlan { allocation_bytes } => {
                clear_plan(app, service);
                let params = allocation_bytes
                    .map(|bytes| json!({"allocationBytes": bytes}))
                    .unwrap_or_else(|| json!({}));
                connection.call("prepare_plan", &params, false, None)?;
            }
            AppleSetupIntent::ReviewPlan => {
                connection.call(
                    "review_plan",
                    &json!({}),
                    false,
                    Some(Duration::from_secs(60)),
                )?;
            }
            AppleSetupIntent::ChooseStorage {
                choice_id,
                allocation_bytes,
                confirmation,
            } => {
                clear_plan(app, service);
                let mut params = json!({"choiceId":choice_id});
                if let Some(bytes) = allocation_bytes {
                    params["allocationBytes"] = json!(bytes);
                }
                if let Some(typed) = confirmation {
                    params["confirmation"] = json!(typed);
                }
                connection.call("choose_storage", &params, false, None)?;
            }
            AppleSetupIntent::ApprovePlan => approve(app, service, &connection)?,
            AppleSetupIntent::Execute => execute(app, service, &connection, false)?,
            AppleSetupIntent::RetryRecovery => execute(app, service, &connection, true)?,
            AppleSetupIntent::Cancel => {
                connection.call("cancel", &json!({}), false, Some(Duration::from_secs(60)))?;
                clear_plan(app, service);
            }
            AppleSetupIntent::Refresh => {
                connection.call(
                    "refresh_helper",
                    &json!({}),
                    false,
                    Some(Duration::from_secs(60)),
                )?;
                let probe =
                    connection.call("probe", &json!({}), false, Some(Duration::from_secs(60)))?;
                service.update(app, |inner| inner.snapshot.probe = Some(probe));
            }
            AppleSetupIntent::Connect => unreachable!(),
        }
        Ok(())
    }

    fn clear_plan(app: &AppHandle, service: &AppleSetupService) {
        service.update(app, |inner| {
            inner.snapshot.plan = None;
            inner.approved_digest = None;
        });
    }

    fn retained_plan(service: &AppleSetupService) -> Result<(String, Value), String> {
        let inner = service
            .inner
            .lock()
            .map_err(|_| "Apple installation state is unavailable")?;
        let plan = inner
            .snapshot
            .plan
            .clone()
            .ok_or("The retained native plan is unavailable")?;
        let digest = plan
            .get("bindingDigest")
            .and_then(Value::as_str)
            .ok_or("The native plan has no binding digest")?
            .to_owned();
        Ok((digest, plan))
    }

    fn confirmation_text(plan: &Value, purpose: &str) -> String {
        let mut text = format!("{purpose}\n\n");
        if let Some(deletion) = plan.get("deletion") {
            text.push_str("PERMANENT DELETION: ");
            text.push_str(
                deletion
                    .get("identifier")
                    .and_then(Value::as_str)
                    .unwrap_or("Existing installation"),
            );
            text.push_str(" — ");
            text.push_str(
                deletion
                    .get("sourceIdentifier")
                    .and_then(Value::as_str)
                    .unwrap_or("unavailable"),
            );
            text.push_str("\nThe entire existing installation, operating system and files will be deleted. This cannot be undone.\n\n");
        }
        if let Some(facts) = plan.get("facts").and_then(Value::as_array) {
            for row in facts.iter().take(24) {
                if let (Some(label), Some(value)) = (
                    row.get("label").and_then(Value::as_str),
                    row.get("value").and_then(Value::as_str),
                ) {
                    text.push_str(label);
                    text.push_str(": ");
                    text.push_str(value);
                    text.push('\n');
                }
            }
        }
        text.push_str("\nOmarchy allocation: ");
        text.push_str(
            plan.get("omarchyBytes")
                .and_then(Value::as_str)
                .unwrap_or("unavailable"),
        );
        text.push_str(" bytes\nPlan binding: ");
        text.push_str(
            plan.get("bindingDigest")
                .and_then(Value::as_str)
                .unwrap_or("unavailable"),
        );
        text
    }

    fn confirm(app: &AppHandle, plan: &Value, purpose: &str, button: &str) -> bool {
        let dialog = app
            .dialog()
            .message(confirmation_text(plan, purpose))
            .title("Omarchy Installer — Apple installation")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                button.into(),
                "Cancel".into(),
            ));
        if let Some(window) = app.get_webview_window("main") {
            dialog.parent(&window).blocking_show()
        } else {
            dialog.blocking_show()
        }
    }

    fn assert_current_plan(
        service: &AppleSetupService,
        connection: &Connection,
        digest: &str,
        expected_phase: Option<&str>,
    ) -> Result<(), String> {
        let inner = service
            .inner
            .lock()
            .map_err(|_| "Apple installation state is unavailable")?;
        if inner.generation != connection.generation
            || !connection.alive.load(Ordering::SeqCst)
            || inner.snapshot.outcome_unknown
            || inner
                .snapshot
                .plan
                .as_ref()
                .and_then(|plan| plan.get("bindingDigest"))
                .and_then(Value::as_str)
                != Some(digest)
            || expected_phase.is_some_and(|expected| phase(&inner) != expected)
        {
            return Err(
                "The native plan changed while confirmation was open. Review it again.".into(),
            );
        }
        Ok(())
    }

    fn approve(
        app: &AppHandle,
        service: &AppleSetupService,
        connection: &Connection,
    ) -> Result<(), String> {
        let (digest, plan) = retained_plan(service)?;
        if !confirm(app, &plan, "Approve this exact native installation plan. Keep a current backup and review the allocation and Recovery steps.", "Approve plan") {
            return Ok(());
        }
        assert_current_plan(service, connection, &digest, Some("plan_review"))?;
        connection.call(
            "acknowledge",
            &json!({"value": true}),
            false,
            Some(Duration::from_secs(60)),
        )?;
        let result = connection.call(
            "approve",
            &json!({"bindingDigest": digest}),
            false,
            Some(Duration::from_secs(60)),
        )?;
        if result.get("phase").and_then(Value::as_str) == Some("awaiting_install") {
            service.update(app, |inner| inner.approved_digest = Some(digest));
        }
        Ok(())
    }

    struct Credentials {
        username: String,
        password: String,
    }
    impl Drop for Credentials {
        fn drop(&mut self) {
            // Best-effort cleanup of our owned buffers; OS/Foundation/allocator
            // copies are not claimed to be completely zeroized.
            unsafe {
                self.username.as_mut_vec().fill(0);
                self.password.as_mut_vec().fill(0);
            }
        }
    }

    fn credentials() -> Result<Option<Credentials>, String> {
        // Fixed native dialog program. No credential or plan text is interpolated
        // into script source, argv, environment, an on-disk script, or the webview.
        const SCRIPT: &str = r#"
try
  set ownerDialog to display dialog "Enter the short account name of this Mac's machine owner." default answer (system attribute "USER") with title "Omarchy Installer — Machine owner" buttons {"Cancel", "Continue"} default button "Continue" cancel button "Cancel"
  set secretDialog to display dialog "Enter the machine owner's password to authorize the approved native installation." default answer "" with hidden answer with title "Omarchy Installer — Authorize installation" buttons {"Cancel", "Authorize"} default button "Authorize" cancel button "Cancel"
  return (text returned of ownerDialog) & linefeed & (text returned of secretDialog)
on error number -128
  return ""
end try
"#;
        let mut output = Command::new("/usr/bin/osascript")
            .args(["-e", SCRIPT])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .map_err(|_| "The native credential dialog could not be opened")?;
        if !output.status.success() {
            output.stdout.fill(0);
            return Err("The native credential dialog did not complete".into());
        }
        if output.stdout == b"\n" || output.stdout.is_empty() {
            output.stdout.fill(0);
            return Ok(None);
        }
        if output.stdout.last() == Some(&b'\n') {
            output.stdout.pop();
        }
        let parsed = (|| {
            if output.stdout.len() > 1400 {
                return Err("Machine-owner credentials exceed the supported length");
            }
            let text = std::str::from_utf8(&output.stdout)
                .map_err(|_| "Machine-owner credentials must be valid UTF-8")?;
            let (username, password) = text
                .split_once('\n')
                .ok_or("The native credential dialog returned an invalid response")?;
            if username.is_empty()
                || username.len() > 255
                || !username
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-._".contains(&byte))
                || password.is_empty()
                || password.len() > 1024
                || password.bytes().any(|byte| [0, 10, 13].contains(&byte))
            {
                return Err("Enter a valid machine-owner account and password");
            }
            Ok(Credentials {
                username: username.to_owned(),
                password: password.to_owned(),
            })
        })();
        output.stdout.fill(0);
        parsed.map(Some).map_err(str::to_owned)
    }

    fn execute(
        app: &AppHandle,
        service: &AppleSetupService,
        connection: &Connection,
        retry: bool,
    ) -> Result<(), String> {
        let (digest, plan) = retained_plan(service)?;
        let (purpose, button, command) = if retry {
            ("Retry only the upstream-approved Recovery authorization step for this retained plan.", "Retry Recovery authorization", "retry_recovery")
        } else {
            ("Start this approved installation now. The helper will change the selected internal disk allocation and install Omarchy. Native execution cannot be cancelled once submitted.", "Start installation", "execute")
        };
        if !confirm(app, &plan, purpose, button) {
            return Ok(());
        }
        let Some(credentials) = credentials()? else {
            return Ok(());
        };
        assert_current_plan(
            service,
            connection,
            &digest,
            (!retry).then_some("awaiting_install"),
        )?;
        {
            let inner = service
                .inner
                .lock()
                .map_err(|_| "Apple installation state is unavailable")?;
            if inner.approved_digest.as_deref() != Some(digest.as_str())
                || !flag(
                    &inner,
                    if retry {
                        "canRetryRecovery"
                    } else {
                        "canExecute"
                    },
                )
            {
                return Err("The approved native installation is no longer ready".into());
            }
        }
        let marker = marker_path(app)?;
        let token = uuid::Uuid::new_v4().to_string();
        fs::create_dir_all(
            marker
                .parent()
                .ok_or("Apple operation storage is unavailable")?,
        )
        .map_err(|_| "Could not create Apple operation storage")?;
        let mut record = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&marker)
            .map_err(|_| "A previous Apple operation record exists or cannot be created; review its state before installing")?;
        serde_json::to_writer(&mut record, &json!({
            "schema": 1, "token": token, "operation": command, "bindingDigest": digest,
            "sourceCommit": SOURCE_COMMIT,
            "startedAtUnix": SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| "The system clock is invalid")?.as_secs(),
        })).map_err(|_| "Could not record the pending Apple installation")?;
        record
            .sync_all()
            .map_err(|_| "Could not flush the pending Apple installation record")?;
        drop(record);
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Parameters<'a> {
            binding_digest: &'a str,
            username: &'a str,
            password: &'a str,
        }
        let result = connection.call(
            command,
            &Parameters {
                binding_digest: &digest,
                username: &credentials.username,
                password: &credentials.password,
            },
            true,
            None,
        );
        drop(credentials);
        let rejected_before_submission =
            result.is_err() && !connection.mutation_pending.load(Ordering::SeqCst);
        if result.is_ok() || rejected_before_submission {
            let recorded: Value = serde_json::from_slice(
                &fs::read(&marker).map_err(|_| "The Apple operation record could not be read")?,
            )
            .map_err(|_| "The Apple operation record changed unexpectedly")?;
            if recorded.get("token").and_then(Value::as_str) != Some(token.as_str()) {
                return Err("The Apple operation record changed unexpectedly".into());
            }
            fs::remove_file(&marker).map_err(|_| "Native execution settled, but its pending record could not be cleared. Review it before another run.")?;
            return result.map(|_| ());
        }
        let error = result
            .err()
            .unwrap_or_else(|| "The native completion is unavailable".into());
        service.update(app, |inner| inner.snapshot.outcome_unknown = true);
        Err(format!("The submitted operation has no trusted completion in this app: {error}. Keep the native journal and follow Recovery guidance; do not repeat installation automatically."))
    }

    fn validated_executable(app: &AppHandle) -> Result<(PathBuf, PathBuf), String> {
        let resources = app
            .path()
            .resource_dir()
            .map_err(|_| "The packaged resource directory is unavailable")?;
        let bundle = resources.join("direct-apple/Omarchy Apple Bridge.app");
        let executable = bundle.join("Contents/MacOS/omarchy-apple-bridge");
        for path in [
            &resources.join("direct-apple"),
            &bundle,
            &bundle.join("Contents"),
            &bundle.join("Contents/MacOS"),
            &executable,
        ] {
            if fs::symlink_metadata(path).map_err(|_| "The native Apple companion is not packaged. Build and assemble providers/direct-apple on macOS first.")?.file_type().is_symlink() {
                return Err("The native Apple companion contains an unexpected symbolic link".into());
            }
        }
        if !executable.is_file() {
            return Err("The native Apple companion executable is unavailable".into());
        }
        let parent_executable =
            std::env::current_exe().map_err(|_| "The desktop executable is unavailable")?;
        let identity = Command::new("/usr/bin/codesign")
            .args(["-d", "--verbose=4"])
            .arg(&parent_executable)
            .stdin(Stdio::null())
            .output()
            .map_err(|_| "Could not inspect the desktop signing identity")?;
        if !identity.status.success() {
            return Err("Apple installation requires a signed desktop distribution".into());
        }
        let information = String::from_utf8_lossy(&identity.stderr);
        let team = information
            .lines()
            .find_map(|line| line.strip_prefix("TeamIdentifier="))
            .filter(|team| {
                team.len() == 10
                    && team
                        .bytes()
                        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
            })
            .ok_or("The desktop has no Apple signing Team ID")?;
        let requirement = format!("anchor apple generic and identifier \"{BRIDGE_ID}\" and certificate leaf[subject.OU] = \"{team}\"");
        let status = Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict", "--deep", "-R", &requirement])
            .arg(&bundle)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "Could not verify the native Apple companion signature")?;
        if !status.success() {
            return Err("The native Apple companion is not intact or does not match the desktop signing identity".into());
        }
        Ok((executable, resources))
    }

    fn connect(app: &AppHandle, service: &AppleSetupService) -> Result<Arc<Connection>, String> {
        let generation = {
            let mut inner = service
                .inner
                .lock()
                .map_err(|_| "Apple installation state is unavailable")?;
            if let Some(existing) = &inner.connection {
                if existing.alive.load(Ordering::SeqCst) {
                    return Ok(existing.clone());
                }
                if !existing.exited.load(Ordering::SeqCst) {
                    return Err("The previous native companion is still settling. Do not start another instance.".into());
                }
                if flag(&inner, "hasExecutionStarted") {
                    return Err("An installation was already submitted. Review its journal before starting a new native session.".into());
                }
            }
            inner.generation += 1;
            inner.generation
        };
        let (executable, resources) = validated_executable(app)?;
        let mut command = Command::new(&executable);
        command
            .arg("--stdio")
            .current_dir(&resources)
            .env_clear()
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        for variable in ["HOME", "USER", "LOGNAME", "TMPDIR", "LANG", "LC_ALL"] {
            if let Some(value) = std::env::var_os(variable) {
                command.env(variable, value);
            }
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| "The native Apple companion could not be started")?;
        let input = child
            .stdin
            .take()
            .ok_or("The native companion input is unavailable")?;
        let output = child
            .stdout
            .take()
            .ok_or("The native companion output is unavailable")?;
        let mut diagnostic = child
            .stderr
            .take()
            .ok_or("The native companion diagnostic stream is unavailable")?;
        let connection = Arc::new(Connection {
            stdin: Mutex::new(Some(input)),
            pending: Mutex::new(HashMap::new()),
            sequence: AtomicU64::new(1),
            alive: AtomicBool::new(true),
            exited: AtomicBool::new(false),
            mutation_pending: AtomicBool::new(false),
            generation,
        });
        service.update(app, |inner| {
            inner.connection = Some(connection.clone());
            inner.approved_digest = None;
            inner.snapshot.native = None;
            inner.snapshot.plan = None;
        });
        // Drain continuously instead of retaining arbitrary diagnostics or
        // stopping after a prefix and deadlocking the child's stderr pipe.
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut diagnostic, &mut std::io::sink());
        });
        let reader_connection = connection.clone();
        let reader_service = service.clone();
        let reader_app = app.clone();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            let failure = loop {
                match read_frame(&mut reader) {
                    Ok(Some(frame)) => {
                        if let Err(error) =
                            receive(&reader_app, &reader_service, &reader_connection, frame)
                        {
                            break error;
                        }
                    }
                    Ok(None) => break "The native Apple companion disconnected".to_owned(),
                    Err(error) => break error,
                }
            };
            reader_connection.close(&failure);
            reader_service.update(&reader_app, |inner| {
                if inner.generation != generation {
                    return;
                }
                inner.snapshot.available = false;
                inner.snapshot.outcome_unknown |=
                    reader_connection.mutation_pending.load(Ordering::SeqCst);
                inner.snapshot.status = if inner.snapshot.outcome_unknown {
                    "unknown"
                } else {
                    "disconnected"
                }
                .into();
                inner.snapshot.error = Some(AppleSetupError {
                    code: "bridge_disconnected".into(),
                    message: failure,
                });
            });
        });
        let wait_connection = connection.clone();
        std::thread::spawn(move || {
            let _ = child.wait();
            wait_connection.exited.store(true, Ordering::SeqCst);
        });
        Ok(connection)
    }

    fn read_frame(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
        let mut bytes = Vec::new();
        loop {
            let buffer = reader
                .fill_buf()
                .map_err(|_| "The native protocol stream could not be read")?;
            if buffer.is_empty() {
                return if bytes.is_empty() {
                    Ok(None)
                } else {
                    Err("The native protocol stream ended inside a message".into())
                };
            }
            let end = buffer.iter().position(|byte| *byte == b'\n');
            let take = end.map_or(buffer.len(), |index| index + 1);
            if bytes.len() + take > MAX_OUTPUT_FRAME {
                return Err("The native companion exceeded the protocol frame limit".into());
            }
            bytes.extend_from_slice(&buffer[..take]);
            reader.consume(take);
            if end.is_some() {
                return serde_json::from_slice(&bytes)
                    .map(Some)
                    .map_err(|_| "The native companion sent an invalid protocol message".into());
            }
        }
    }

    fn receive(
        app: &AppHandle,
        service: &AppleSetupService,
        connection: &Connection,
        frame: Value,
    ) -> Result<(), String> {
        if frame.get("version").and_then(Value::as_u64) != Some(1) {
            return Err("The native companion uses an unsupported protocol".into());
        }
        match frame.get("type").and_then(Value::as_str) {
            Some("state") => apply_native(
                app,
                service,
                connection.generation,
                frame
                    .get("data")
                    .cloned()
                    .ok_or("Native state is missing")?,
            ),
            Some("result") | Some("error") => {
                let id = frame
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or("The native reply has no request identifier")?;
                let result = if frame.get("type").and_then(Value::as_str) == Some("result") {
                    let data = frame
                        .get("data")
                        .cloned()
                        .ok_or("The native reply is missing its data")?;
                    if data.get("sessionId").is_some() {
                        apply_native(app, service, connection.generation, data.clone())?;
                    }
                    Ok(data)
                } else {
                    Err(frame
                        .get("error")
                        .and_then(|error| error.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("The native coordinator rejected the request")
                        .to_owned())
                };
                if let Some(pending) = connection
                    .pending
                    .lock()
                    .map_err(|_| "Native reply state is unavailable")?
                    .remove(id)
                {
                    if pending.mutating {
                        connection.mutation_pending.store(false, Ordering::SeqCst);
                    }
                    let _ = pending.reply.send(result);
                }
                Ok(())
            }
            _ => Err("The native companion sent an unknown message type".into()),
        }
    }

    fn apply_native(
        app: &AppHandle,
        service: &AppleSetupService,
        generation: u64,
        value: Value,
    ) -> Result<(), String> {
        let phase = value
            .get("phase")
            .and_then(Value::as_str)
            .ok_or("Native state has no phase")?;
        if ![
            "idle",
            "inspecting",
            "unsupported",
            "welcome",
            "existing_install_choice",
            "preparing_plan",
            "plan_prepared",
            "plan_review",
            "awaiting_install",
            "installing",
            "awaiting_recovery",
            "done",
            "failed",
        ]
        .contains(&phase)
            || value
                .get("sessionId")
                .and_then(Value::as_str)
                .and_then(|id| uuid::Uuid::parse_str(id).ok())
                .is_none()
        {
            return Err("The native companion sent an invalid state snapshot".into());
        }
        service.update(app, |inner| {
            if inner.generation != generation {
                return;
            }
            if let Some(plan) = value.get("plan") {
                inner.snapshot.plan = Some(plan.clone());
            }
            inner.snapshot.native = Some(value);
        });
        Ok(())
    }
}
