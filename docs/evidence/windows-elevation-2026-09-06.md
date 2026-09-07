# Administrator connection access-denied fix

The user reported `Access is denied. (os error 5)` after approving the Windows
administrator prompt for **Check with administrator access**.

The desktop created its events pipe with `PIPE_ACCESS_INBOUND`, then called
`SetNamedPipeHandleState` to switch that server handle from polling to blocking
reads after accepting the helper. The inbound handle lacked the access required
for that mode change. Consequently, connection setup failed after the UAC launch
had already succeeded and before the inspection request was sent.

Microsoft documents the handle-access requirement for
[SetNamedPipeHandleState](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-setnamedpipehandlestate).
[CreateNamedPipe](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea)
accepts a bounded set of open-mode flags; adding `FILE_WRITE_ATTRIBUTES` directly
to that function's open mode is not a supported fix.

The events server now uses `PIPE_ACCESS_DUPLEX` to acquire the required rights.
The helper continues to open that channel write-only, and the desktop connection
exposes it only as a reader. Requests remain on their separate outbound channel.
Remote-client rejection, first-instance protection, helper PID checks and the
helper's desktop PID/executable validation remain in place. Mode-configuration
errors now identify the administrator connection instead of showing an
unqualified OS error alone.

## Validation

Performed on Windows using `gpt-6-astra`, 2026-09-06:

```text
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib --locked elevation::platform::tests -- --nocapture
```

- Before the permission fix, the real-pipe message-exchange test failed with
  `Could not configure the administrator connection: Access is denied. (os error 5)`.
- After the fix, both tests passed: the production channel-creation and accept
  functions successfully exchanged request/event bytes, and an unexpected PID
  was still rejected.
- These tests use actual Windows pipe handles without UAC or privileged disk
  operations. They reproduce and verify the failing connection-mode transition;
  they do not constitute an end-to-end administrator disk-inspection test.

## Corrected portable executable

- Path: `artifacts/windows-portable/2a5faa8a/Omarchy-Setup-0.1.0-x64-portable.exe`.
- Size: 184,502,100 bytes; unsigned optimized release build.
- SHA-256: `8e1a1c3c462a223573ebee33fc1141e1b2a4b931170d63d00f71c11c1b17d4b8`.
- The package passed GUI-subsystem checks, embedded-manifest comparison, all
  provider hashes, the staged native-module/SDK file-write test, and actual
  extraction/full-payload hashing.
- The visible startup test observed the extraction indicator after 0.96 seconds
  and the application window after 12.49 seconds. Normal window closure returned
  exit code 0 and removed the temporary payload. The test did not invoke disk
  inspection or UAC.
- `portable-record.json` and `startup-verification.json` beside the executable
  record these results. This replaces the earlier `012ef35a` executable for use.
