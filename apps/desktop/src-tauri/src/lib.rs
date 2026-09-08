//! Unprivileged desktop commands and a narrow, separately elevated helper.

mod apple_setup;
mod downloads;
mod elevation;
#[cfg(windows)]
mod iso_source;
mod operation_cleanup;
mod operation_records;
mod provider_process;
mod provider_runtime;
mod setup;
mod setup_helper;
mod setup_protocol;
mod usb_preserve;

use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

fn operation_active(app: &tauri::AppHandle) -> bool {
    app.try_state::<setup::Setup>()
        .is_some_and(|state| state.active_operation())
        || app
            .try_state::<apple_setup::AppleSetupService>()
            .is_some_and(|state| state.active_operation())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if operation_active(window.app_handle()) {
                    api.prevent_close();
                    window.dialog().message("Wait for the setup operation to finish, or use Cancel in the app when cancellation is available.").title("Setup is in progress").show(|_| {});
                }
            }
        })
        .setup(|app| {
            app.manage(setup::Setup::default());
            app.manage(apple_setup::AppleSetupService::default());
            app.manage(downloads::Downloads::new(
                app.path().app_cache_dir()?.join("images"),
                app.path().download_dir()?.join("Omarchy"),
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            downloads::download_status,
            downloads::resolve_download,
            downloads::start_download,
            downloads::cancel_download,
            downloads::choose_download_directory,
            setup::setup_status,
            setup::inspect_setup,
            setup::inspect_usb_choice,
            setup::prepare_setup,
            setup::review_usb_setup,
            setup::dismiss_usb_review,
            setup::start_setup,
            setup::cancel_setup,
            apple_setup::apple_setup_status,
            apple_setup::apple_setup_action,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Omarchy Installer")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if operation_active(app) { api.prevent_exit(); }
            }
        });
}

pub fn run_helper(id: &str, parent: u32) -> Result<(), String> {
    setup_helper::entry(id, parent)
}
