//! What the desktop process (not the elevated helper) reads and writes in the
//! signed-in user's Windows session: Secure Boot's state, which Windows shows
//! to standard users, a one-time entry that reopens the app at the next
//! sign-in, and the follow-up while a temporary installer exists.

/// Secure Boot as Windows reports it; `None` when the PC doesn't say (legacy
/// BIOS, or the value can't be read). For display only: arming the installer
/// checks Secure Boot itself.
pub fn secure_boot() -> Option<bool> {
    platform::secure_boot()
}

/// Reopen the app at the next sign-in. Windows deletes the entry as it runs it.
/// Returns the registered command, which carries a token of its own.
pub fn reopen_at_sign_in() -> Result<String, String> {
    platform::reopen_at_sign_in()
}

/// Remove the entry once the PC is known not to restart, unless another copy
/// of the app has registered its own since (`command` is what this one wrote).
pub fn cancel_reopen(command: &str) {
    platform::cancel_reopen(command)
}

/// While the app shows a temporary installer that paused BitLocker, a named
/// event keeps the BitLocker sign-in reminder quiet, since the card offers the
/// same choices. While an installer hasn't opted out, a Run entry reopens the
/// app at each sign-in. Called with every recovery summary; acts only on
/// changes. Returns whether the Run entry is in place.
pub fn follow_up(quiet_reminder: bool, reopen: bool) -> bool {
    platform::follow_up(quiet_reminder, reopen)
}

/// Whether the follow-up's Run entry already reopens the app at sign-in, so
/// the firmware restart needn't add its own one-time entry (two would start
/// two copies).
pub fn reopens_at_sign_in() -> bool {
    platform::reopens_at_sign_in()
}

/// Started by the reopen entry at sign-in.
pub fn reopened() -> bool {
    std::env::args().any(|arg| arg == "--reopened")
}

/// Installers whose card said "Don't open at sign-in".
pub fn opted_out() -> Vec<String> {
    platform::opted_out()
}

/// `existing`: the installers that still exist, so removed ones are dropped.
pub fn set_reopen(operation_id: &str, reopen: bool, existing: &[String]) -> Result<(), String> {
    platform::set_reopen(operation_id, reopen, existing)
}

/// The opt-out list after one installer changes its choice: IDs separated by
/// semicolons, only of installers that still exist, without duplicates. At
/// most six (the oldest go), which keeps it within the registry helper's 260
/// characters; more installers than that never exist at once.
fn updated_opt_outs(
    current: &str,
    operation_id: &str,
    reopen: bool,
    existing: &[String],
) -> String {
    let mut ids: Vec<&str> = current
        .split(';')
        .filter(|id| *id != operation_id && existing.iter().any(|kept| kept == id))
        .collect();
    if !reopen {
        ids.push(operation_id);
    }
    let skip = ids.len().saturating_sub(6);
    ids[skip..].join(";")
}

#[cfg(test)]
mod opt_out_tests {
    #[test]
    fn opting_out_and_back_in_edits_only_that_installer() {
        let existing: Vec<String> = ["a", "b"].map(String::from).to_vec();
        let list = super::updated_opt_outs("", "a", false, &existing);
        assert_eq!(list, "a");
        let list = super::updated_opt_outs(&list, "b", false, &existing);
        assert_eq!(super::updated_opt_outs(&list, "b", false, &existing), "a;b");
        assert_eq!(super::updated_opt_outs(&list, "a", true, &existing), "b");
        // Removed installers drop out instead of crowding out existing ones.
        assert_eq!(
            super::updated_opt_outs("old;a", "b", false, &existing),
            "a;b"
        );
        let many: Vec<String> = (0..10).map(|_| uuid::Uuid::new_v4().to_string()).collect();
        let full = many.iter().fold(String::new(), |list, id| {
            super::updated_opt_outs(&list, id, false, &many)
        });
        assert_eq!(full, many[4..].join(";"));
        assert!(full.encode_utf16().count() <= 260);
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Registry::*;

    const SECURE_BOOT_KEY: &str = r"SYSTEM\CurrentControlSet\Control\SecureBoot\State";
    const RUN_ONCE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "OmarchyInstaller";
    const SETTINGS_KEY: &str = r"Software\OmarchyInstaller";
    const OPT_OUT_VALUE: &str = "NoReopen";
    // Its counterpart in BitLockerSetup.ps1 (Test-AppFollowupOpen).
    const FOLLOW_UP_EVENT: &str = r"Local\OmarchyInstallerFollowUp";
    // RunOnce runs commands of up to 260 characters.
    const MAX_COMMAND: usize = 260;

    fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
        value.as_ref().encode_wide().chain(Some(0)).collect()
    }

    pub fn secure_boot() -> Option<bool> {
        read_dword(HKEY_LOCAL_MACHINE, SECURE_BOOT_KEY, "UEFISecureBootEnabled").map(|v| v != 0)
    }
    pub fn reopen_at_sign_in() -> Result<String, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        // The token tells this copy's entry from another's; the app ignores it.
        let command = format!("\"{}\" --reopened {}", exe.display(), uuid::Uuid::new_v4());
        write_string(HKEY_CURRENT_USER, RUN_ONCE_KEY, VALUE_NAME, &command)?;
        Ok(command)
    }
    pub fn cancel_reopen(command: &str) {
        delete_value_if(HKEY_CURRENT_USER, RUN_ONCE_KEY, VALUE_NAME, command);
    }

    // The event handle (as a number, so the state can be static), the Run
    // entry last asked for, and whether it is in place.
    struct FollowUp {
        event: usize,
        reopen: Option<bool>,
        registered: bool,
    }
    static FOLLOW_UP: std::sync::Mutex<FollowUp> = std::sync::Mutex::new(FollowUp {
        event: 0,
        reopen: None,
        registered: false,
    });

    pub fn follow_up(quiet_reminder: bool, reopen: bool) -> bool {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::CreateEventW;
        let Ok(mut state) = FOLLOW_UP.lock() else {
            return false;
        };
        if quiet_reminder && state.event == 0 {
            // A failure leaves it 0, so the next summary tries again.
            state.event =
                unsafe { CreateEventW(std::ptr::null(), 1, 0, wide(FOLLOW_UP_EVENT).as_ptr()) }
                    as usize;
        } else if !quiet_reminder && state.event != 0 {
            unsafe { CloseHandle(state.event as _) };
            state.event = 0;
        }
        // A Run entry, unlike RunOnce, stays until removed, so it's written
        // once per change. Every copy of the app reads the same installers,
        // so the entry is this app's, whichever copy wrote it.
        if state.reopen != Some(reopen) {
            state.registered = if reopen {
                // Read first: the entry may be what started this run, and a
                // startup program shouldn't rewrite the key that launched it.
                std::env::current_exe()
                    .map_err(|e| e.to_string())
                    .and_then(|exe| {
                        let command = format!("\"{}\" --reopened", exe.display());
                        if read_string(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME).as_deref()
                            == Some(command.as_str())
                        {
                            return Ok(());
                        }
                        write_string(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME, &command)
                    })
                    .is_ok()
            } else {
                unsafe {
                    RegDeleteKeyValueW(
                        HKEY_CURRENT_USER,
                        wide(RUN_KEY).as_ptr(),
                        wide(VALUE_NAME).as_ptr(),
                    );
                }
                false
            };
            state.reopen = Some(reopen);
        }
        state.registered
    }
    pub fn reopens_at_sign_in() -> bool {
        FOLLOW_UP
            .lock()
            .map(|state| state.registered)
            .unwrap_or(false)
    }
    pub fn opted_out() -> Vec<String> {
        read_string(HKEY_CURRENT_USER, SETTINGS_KEY, OPT_OUT_VALUE)
            .map(|list| {
                list.split(';')
                    .filter(|id| !id.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn set_reopen(operation_id: &str, reopen: bool, existing: &[String]) -> Result<(), String> {
        let current =
            read_string(HKEY_CURRENT_USER, SETTINGS_KEY, OPT_OUT_VALUE).unwrap_or_default();
        let list = super::updated_opt_outs(&current, operation_id, reopen, existing);
        write_string(HKEY_CURRENT_USER, SETTINGS_KEY, OPT_OUT_VALUE, &list)
    }

    pub(super) fn read_dword(root: HKEY, key: &str, name: &str) -> Option<u32> {
        let mut value = 0_u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            RegGetValueW(
                root,
                wide(key).as_ptr(),
                wide(name).as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut value as *mut u32).cast(),
                &mut size,
            )
        };
        (status == 0).then_some(value)
    }
    pub(super) fn write_string(
        root: HKEY,
        key: &str,
        name: &str,
        data: &str,
    ) -> Result<(), String> {
        // Windows counts UTF-16 units.
        if data.encode_utf16().count() > MAX_COMMAND {
            return Err("The app's path is too long to reopen it at sign-in".into());
        }
        let data = wide(data);
        let status = unsafe {
            RegSetKeyValueW(
                root,
                wide(key).as_ptr(),
                wide(name).as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(std::io::Error::from_raw_os_error(status as i32).to_string())
        }
    }
    pub(super) fn read_string(root: HKEY, key: &str, name: &str) -> Option<String> {
        let mut buffer = vec![0_u16; 1024];
        let mut size = (buffer.len() * 2) as u32;
        let status = unsafe {
            RegGetValueW(
                root,
                wide(key).as_ptr(),
                wide(name).as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }
        let units = (size as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buffer[..units]))
    }
    pub(super) fn delete_value_if(root: HKEY, key: &str, name: &str, expected: &str) {
        if read_string(root, key, name).as_deref() == Some(expected) {
            unsafe {
                RegDeleteKeyValueW(root, wide(key).as_ptr(), wide(name).as_ptr());
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // A scratch key, never the real RunOnce or Secure Boot keys.
        #[test]
        fn values_round_trip_and_missing_values_read_as_unknown() {
            let key = format!(r"Software\OmarchyInstallerTest-{}", uuid::Uuid::new_v4());
            assert_eq!(read_dword(HKEY_CURRENT_USER, &key, "Enabled"), None);
            write_string(
                HKEY_CURRENT_USER,
                &key,
                "Command",
                "\"C:\\Omarchy\\app.exe\"",
            )
            .unwrap();
            assert!(write_string(HKEY_CURRENT_USER, &key, "Long", &"x".repeat(261)).is_err());
            // 130 characters outside the Basic Multilingual Plane are 260 units.
            assert!(
                write_string(HKEY_CURRENT_USER, &key, "Wide", &"\u{1F600}".repeat(130)).is_ok()
            );
            assert!(
                write_string(HKEY_CURRENT_USER, &key, "Wider", &"\u{1F600}".repeat(131)).is_err()
            );
            assert_eq!(
                read_string(HKEY_CURRENT_USER, &key, "Command").as_deref(),
                Some("\"C:\\Omarchy\\app.exe\"")
            );
            // A string isn't a DWORD.
            assert_eq!(read_dword(HKEY_CURRENT_USER, &key, "Command"), None);
            let one = 1_u32.to_le_bytes();
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    wide(&key).as_ptr(),
                    wide("Enabled").as_ptr(),
                    REG_DWORD,
                    one.as_ptr().cast(),
                    4,
                );
            }
            assert_eq!(read_dword(HKEY_CURRENT_USER, &key, "Enabled"), Some(1));
            // Another copy's entry stays; this copy's own is removed.
            delete_value_if(HKEY_CURRENT_USER, &key, "Command", "\"C:\\Other\\app.exe\"");
            assert!(read_string(HKEY_CURRENT_USER, &key, "Command").is_some());
            delete_value_if(
                HKEY_CURRENT_USER,
                &key,
                "Command",
                "\"C:\\Omarchy\\app.exe\"",
            );
            assert_eq!(read_string(HKEY_CURRENT_USER, &key, "Command"), None);
            delete_value_if(
                HKEY_CURRENT_USER,
                &key,
                "Command",
                "\"C:\\Omarchy\\app.exe\"",
            );
            unsafe {
                RegDeleteTreeW(HKEY_CURRENT_USER, wide(&key).as_ptr());
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn secure_boot() -> Option<bool> {
        None
    }
    pub fn reopen_at_sign_in() -> Result<String, String> {
        Err("Reopening at sign-in requires Windows".into())
    }
    pub fn cancel_reopen(_: &str) {}
    pub fn follow_up(_: bool, _: bool) -> bool {
        false
    }
    pub fn reopens_at_sign_in() -> bool {
        false
    }
    pub fn opted_out() -> Vec<String> {
        Vec::new()
    }
    pub fn set_reopen(_: &str, _: bool, _: &[String]) -> Result<(), String> {
        Err("Reopening at sign-in requires Windows".into())
    }
}
