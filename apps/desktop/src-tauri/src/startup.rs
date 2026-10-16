//! What the desktop process (not the elevated helper) reads and writes in the
//! signed-in user's Windows session: Secure Boot's state, which Windows shows
//! to standard users, and a one-time entry that reopens the app at the next
//! sign-in after the firmware restart.

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

#[cfg(windows)]
mod platform {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Registry::*;

    const SECURE_BOOT_KEY: &str = r"SYSTEM\CurrentControlSet\Control\SecureBoot\State";
    const RUN_ONCE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
    const VALUE_NAME: &str = "OmarchyInstaller";
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
}
