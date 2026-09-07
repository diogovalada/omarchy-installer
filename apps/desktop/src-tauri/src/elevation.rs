//! One-shot elevation transport. The webview cannot select the executable,
//! transport address, helper arguments, or privileged operation workspace.
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct Connection {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
}

pub fn launch() -> Result<Connection, String> {
    platform::launch()
}
pub fn connect(id: &str, parent: u32) -> Result<Connection, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "Invalid helper session")?;
    platform::connect(id, parent)
}
pub fn protected_directory(parent: Option<&Path>, name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("Invalid operation directory name".into());
    }
    platform::protected_directory(parent, name)
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ffi::OsStr;
    use std::fs::File;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::ptr::{null, null_mut};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Security::Authorization::*;
    use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows_sys::Win32::Storage::FileSystem::*;
    use windows_sys::Win32::System::Pipes::*;
    use windows_sys::Win32::System::Threading::*;
    use windows_sys::Win32::UI::Shell::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
        value.as_ref().encode_wide().chain(Some(0)).collect()
    }
    fn os_error() -> String {
        std::io::Error::last_os_error().to_string()
    }
    fn pipe_name(id: &str, channel: &str) -> String {
        format!(r"\\.\pipe\omarchy-setup-{id}-{channel}")
    }
    fn create_pipe(name: &str, access: u32) -> Result<File, String> {
        // Separate channels permit cancellation writes while the event
        // reader blocks. Reject remote clients and namespace pre-creation.
        let handle = unsafe {
            CreateNamedPipeW(
                wide(name).as_ptr(),
                access | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                0,
                null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(os_error());
        }
        Ok(unsafe { File::from_raw_handle(handle) })
    }
    fn accept(
        pipe: &File,
        process: &OwnedHandle,
        pid: u32,
        deadline: Instant,
    ) -> Result<(), String> {
        loop {
            let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle(), null_mut()) } != 0;
            let error = unsafe { GetLastError() };
            if connected || error == ERROR_PIPE_CONNECTED {
                break;
            }
            if error != ERROR_PIPE_LISTENING && error != ERROR_NO_DATA {
                return Err(os_error());
            }
            if Instant::now() > deadline
                || unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } == WAIT_OBJECT_0
            {
                return Err("The elevated helper did not connect".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let mut peer = 0;
        if unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &mut peer) } == 0
            || peer != pid
        {
            return Err("Elevated helper identity mismatch".into());
        }
        let mode = PIPE_WAIT | PIPE_READMODE_BYTE;
        if unsafe { SetNamedPipeHandleState(pipe.as_raw_handle(), &mode, null(), null()) } == 0 {
            return Err(format!(
                "Could not configure the administrator connection: {}",
                os_error()
            ));
        }
        Ok(())
    }
    fn create_channels(id: &str) -> Result<(File, File), String> {
        let requests = create_pipe(&pipe_name(id, "requests"), PIPE_ACCESS_OUTBOUND)?;
        // SetNamedPipeHandleState needs write-attribute access when accept()
        // changes the server from polling to blocking reads. INBOUND grants
        // only GENERIC_READ; DUPLEX also grants the required GENERIC_WRITE.
        // CreateNamedPipe cannot take FILE_WRITE_ATTRIBUTES as an open-mode flag.
        // The helper still opens write-only, and Connection exposes only Read.
        let events = create_pipe(&pipe_name(id, "events"), PIPE_ACCESS_DUPLEX)?;
        Ok((requests, events))
    }
    pub fn launch() -> Result<Connection, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let (requests, events) = create_channels(&id)?;
        let exe = wide(std::env::current_exe().map_err(|e| e.to_string())?);
        let parameters = wide(format!("--setup-helper {id} {}", std::process::id()));
        let verb = wide("runas");
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = exe.as_ptr();
        info.lpParameters = parameters.as_ptr();
        info.nShow = SW_HIDE;
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            return Err(format!(
                "Administrator approval was not completed: {}",
                os_error()
            ));
        }
        if info.hProcess.is_null() {
            return Err("Elevation did not return a helper process".into());
        }
        let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess) };
        let pid = unsafe { GetProcessId(process.as_raw_handle()) };
        let deadline = Instant::now() + Duration::from_secs(120);
        accept(&requests, &process, pid, deadline)?;
        accept(&events, &process, pid, deadline)?;
        std::thread::spawn(move || unsafe {
            WaitForSingleObject(process.as_raw_handle(), INFINITE);
        });
        Ok(Connection {
            reader: Box::new(events),
            writer: Box::new(requests),
        })
    }
    pub fn connect(id: &str, parent: u32) -> Result<Connection, String> {
        if unsafe { IsUserAnAdmin() } == 0 {
            return Err("This operation requires administrator approval".into());
        }
        let requests = std::fs::OpenOptions::new()
            .read(true)
            .open(pipe_name(id, "requests"))
            .map_err(|e| e.to_string())?;
        let events = std::fs::OpenOptions::new()
            .write(true)
            .open(pipe_name(id, "events"))
            .map_err(|e| e.to_string())?;
        for file in [&requests, &events] {
            let mut peer = 0;
            if unsafe { GetNamedPipeServerProcessId(file.as_raw_handle(), &mut peer) } == 0
                || peer != parent
            {
                return Err("Desktop process identity mismatch".into());
            }
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, parent) };
        if process.is_null() {
            return Err(os_error());
        }
        let process = unsafe { OwnedHandle::from_raw_handle(process) };
        let mut buffer = vec![0_u16; 32768];
        let mut length = buffer.len() as u32;
        if unsafe {
            QueryFullProcessImageNameW(process.as_raw_handle(), 0, buffer.as_mut_ptr(), &mut length)
        } == 0
        {
            return Err(os_error());
        }
        let parent_exe =
            std::fs::canonicalize(String::from_utf16_lossy(&buffer[..length as usize]))
                .map_err(|e| e.to_string())?;
        let helper_exe = std::fs::canonicalize(std::env::current_exe().map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if parent_exe != helper_exe {
            return Err("The helper was not started by this desktop application".into());
        }
        Ok(Connection {
            reader: Box::new(requests),
            writer: Box::new(events),
        })
    }
    pub fn protected_directory(parent: Option<&Path>, name: &str) -> Result<PathBuf, String> {
        let base = if let Some(parent) = parent {
            parent.to_path_buf()
        } else {
            crate::provider_process::program_files()?
        };
        if std::fs::symlink_metadata(&base)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Operation parent is a symbolic link".into());
        }
        let path = base.join(name);
        let descriptor = wide("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");
        let mut security: PSECURITY_DESCRIPTOR = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor.as_ptr(),
                SDDL_REVISION_1,
                &mut security,
                null_mut(),
            )
        } == 0
        {
            return Err(os_error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security,
            bInheritHandle: 0,
        };
        let created = unsafe { CreateDirectoryW(wide(&path).as_ptr(), &attributes) };
        let error = os_error();
        unsafe {
            LocalFree(security);
        }
        if created == 0 {
            return Err(format!(
                "Could not create protected operation directory: {error}"
            ));
        }
        Ok(path)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::windows::io::BorrowedHandle;

        // Real Windows pipe handles, with no UAC prompt or privileged operation.
        // This exercises the same accept/mode transition as the launched helper.
        #[test]
        fn helper_channels_accept_and_exchange_messages() {
            let id = uuid::Uuid::new_v4().to_string();
            let (mut requests, mut events) = create_channels(&id).unwrap();
            let mut client_requests = std::fs::OpenOptions::new()
                .read(true)
                .open(pipe_name(&id, "requests"))
                .unwrap();
            let mut client_events = std::fs::OpenOptions::new()
                .write(true)
                .open(pipe_name(&id, "events"))
                .unwrap();
            let process = unsafe { BorrowedHandle::borrow_raw(GetCurrentProcess()) }
                .try_clone_to_owned()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            accept(&requests, &process, std::process::id(), deadline).unwrap();
            accept(&events, &process, std::process::id(), deadline).unwrap();
            requests.write_all(b"request").unwrap();
            let mut request = [0; 7];
            client_requests.read_exact(&mut request).unwrap();
            assert_eq!(&request, b"request");
            client_events.write_all(b"event").unwrap();
            let mut event = [0; 5];
            events.read_exact(&mut event).unwrap();
            assert_eq!(&event, b"event");
        }

        #[test]
        fn helper_channel_rejects_unexpected_process() {
            let id = uuid::Uuid::new_v4().to_string();
            let (requests, _events) = create_channels(&id).unwrap();
            let _client = std::fs::OpenOptions::new()
                .read(true)
                .open(pipe_name(&id, "requests"))
                .unwrap();
            let process = unsafe { BorrowedHandle::borrow_raw(GetCurrentProcess()) }
                .try_clone_to_owned()
                .unwrap();
            let error = accept(
                &requests,
                &process,
                std::process::id().wrapping_add(1),
                Instant::now() + Duration::from_secs(3),
            )
            .unwrap_err();
            assert_eq!(error, "Elevated helper identity mismatch");
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    fn peer_identity(stream: &UnixStream) -> Result<(u32, u32), String> {
        #[cfg(target_os = "linux")]
        {
            let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
            let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
            if unsafe {
                libc::getsockopt(
                    stream.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_PEERCRED,
                    (&mut credentials as *mut libc::ucred).cast(),
                    &mut size,
                )
            } != 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            if credentials.pid <= 0 {
                return Err("Socket peer process is unavailable".into());
            }
            Ok((credentials.uid, credentials.pid as u32))
        }
        #[cfg(target_os = "macos")]
        {
            let mut uid = 0;
            let mut gid = 0;
            if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let mut pid: libc::pid_t = 0;
            let mut size = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
            if unsafe {
                libc::getsockopt(
                    stream.as_raw_fd(),
                    libc::SOL_LOCAL,
                    libc::LOCAL_PEERPID,
                    (&mut pid as *mut libc::pid_t).cast(),
                    &mut size,
                )
            } != 0
                || pid <= 0
            {
                return Err("Socket peer process is unavailable".into());
            }
            Ok((uid, pid as u32))
        }
    }
    fn socket_directory(id: &str) -> PathBuf {
        PathBuf::from(format!("/tmp/omarchy-setup-{id}"))
    }
    pub fn launch() -> Result<Connection, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let directory = socket_directory(&id);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .map_err(|e| e.to_string())?;
        let listener = UnixListener::bind(directory.join("control")).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        #[cfg(target_os = "linux")]
        let mut command = {
            let mut cmd = Command::new("/usr/bin/pkexec");
            cmd.arg(&exe)
                .args(["--setup-helper", &id, &std::process::id().to_string()]);
            cmd
        };
        #[cfg(target_os = "macos")]
        let mut command = {
            let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
            let shell = format!(
                "{} --setup-helper {} {}",
                quote(&exe.to_string_lossy()),
                quote(&id),
                std::process::id()
            );
            let script = format!(
                "do shell script {} with administrator privileges",
                serde_json::to_string(&shell).map_err(|e| e.to_string())?
            );
            let mut cmd = Command::new("/usr/bin/osascript");
            cmd.args(["-e", &script]);
            cmd
        };
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Could not request administrator approval: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(120);
        let accepted = (|| -> Result<UnixStream, String> {
            loop {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream.set_nonblocking(false).map_err(|e| e.to_string())?;
                        let (uid, pid) = peer_identity(&stream)?;
                        if uid != 0 {
                            return Err("Helper is not privileged".into());
                        }
                        #[cfg(target_os = "linux")]
                        if pid != child.id() {
                            return Err("Elevated helper process identity mismatch".into());
                        }
                        #[cfg(target_os = "macos")]
                        if process_executable(pid)?
                            != std::fs::canonicalize(&exe).map_err(|e| e.to_string())?
                        {
                            return Err("Elevated helper executable identity mismatch".into());
                        }
                        break Ok(stream);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => return Err(error.to_string()),
                }
                if Instant::now() > deadline
                    || child.try_wait().map_err(|e| e.to_string())?.is_some()
                {
                    return Err("Administrator approval was not completed".into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })();
        let stream = match accepted {
            Ok(stream) => stream,
            Err(error) => {
                // No operation request has been sent at this point. Reap a
                // denied/timed-out launcher instead of leaving an orphan/zombie.
                let _ = child.kill();
                let _ = child.wait();
                drop(listener);
                let _ = std::fs::remove_file(directory.join("control"));
                let _ = std::fs::remove_dir(&directory);
                return Err(error);
            }
        };
        // The accepted stream survives unlinking its one-shot rendezvous path.
        drop(listener);
        let _ = std::fs::remove_file(directory.join("control"));
        let _ = std::fs::remove_dir(&directory);
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(Connection {
            reader: Box::new(stream.try_clone().map_err(|e| e.to_string())?),
            writer: Box::new(stream),
        })
    }
    pub fn connect(id: &str, parent: u32) -> Result<Connection, String> {
        if unsafe { libc::geteuid() } != 0 {
            return Err("Administrator approval is required".into());
        }
        let directory = socket_directory(id);
        let metadata = std::fs::symlink_metadata(&directory).map_err(|e| e.to_string())?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.mode() & 0o077 != 0
            || metadata.uid() == 0
        {
            return Err("Invalid desktop socket owner or permissions".into());
        }
        #[cfg(target_os = "linux")]
        {
            let process = PathBuf::from(format!("/proc/{parent}"));
            if std::fs::metadata(&process)
                .map_err(|e| e.to_string())?
                .uid()
                != metadata.uid()
                || std::fs::read_link(process.join("exe")).map_err(|e| e.to_string())?
                    != std::env::current_exe().map_err(|e| e.to_string())?
            {
                return Err("Desktop process identity mismatch".into());
            }
        }
        #[cfg(target_os = "macos")]
        {
            if process_executable(parent)?
                != std::fs::canonicalize(std::env::current_exe().map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?
            {
                return Err("Desktop process identity mismatch".into());
            }
        }
        let stream = UnixStream::connect(directory.join("control")).map_err(|e| e.to_string())?;
        if peer_identity(&stream)? != (metadata.uid(), parent) {
            return Err("Desktop socket process identity mismatch".into());
        }
        Ok(Connection {
            reader: Box::new(stream.try_clone().map_err(|e| e.to_string())?),
            writer: Box::new(stream),
        })
    }
    pub fn protected_directory(parent: Option<&Path>, name: &str) -> Result<PathBuf, String> {
        // macOS /var is a symlink to /private/var. Return the canonical trusted
        // base so the provider's ordinary-local-path policy accepts staged files.
        let base = match parent {
            Some(parent) => parent.to_path_buf(),
            None => std::fs::canonicalize("/var/tmp").map_err(|e| e.to_string())?,
        };
        let metadata = std::fs::symlink_metadata(&base).map_err(|e| e.to_string())?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.uid() != 0
            || (parent.is_some() && metadata.mode() & 0o077 != 0)
        {
            return Err("Invalid privileged operation parent".into());
        }
        let path = base.join(name);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|e| e.to_string())?;
        Ok(path)
    }

    #[cfg(target_os = "macos")]
    fn process_executable(pid: u32) -> Result<PathBuf, String> {
        if pid > i32::MAX as u32 {
            return Err("Invalid process identifier".into());
        }
        let mut buffer = vec![0_u8; 4096];
        let length = unsafe {
            libc::proc_pidpath(pid as i32, buffer.as_mut_ptr().cast(), buffer.len() as u32)
        };
        if length <= 0 {
            return Err("Desktop process is unavailable".into());
        }
        let text = std::ffi::CStr::from_bytes_until_nul(&buffer)
            .map_err(|e| e.to_string())?
            .to_string_lossy();
        std::fs::canonicalize(text.as_ref()).map_err(|e| e.to_string())
    }
}
