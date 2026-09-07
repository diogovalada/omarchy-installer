# Windows portable executable

The latest build includes the subsequent [administrator-connection fix](windows-elevation-2026-09-06.md)
and is recorded there under `artifacts/windows-portable/2a5faa8a/`.

The user rejected installing the setup application and then requested bundling
its files into one executable. This supersedes both the installed NSIS package
and the intermediate portable ZIP as the default Windows distribution.

The default `desktop:package:windows` command now produces a single ordinary-user
launcher. It contains the desktop application and its provider/runtime
files, unpacks them into its own temporary directory, opens the app, and waits
for it to exit. There is no installation wizard, registry registration, shortcut
creation, uninstaller or manual extraction. App caches, downloads and protected
operation records remain in their normal locations. WebView2 and Docker Desktop
requirements are unchanged.

The application already prevents normal closure during an active setup operation.
The elevated helper verifies and copies providers into a separate protected
workspace before privileged work. Abrupt termination can leave temporary files.
The launcher's private build staging uses short paths because the NSIS compiler
rejected a deeply nested dependency at the earlier, longer staging path.

Initial build, superseded by the optimized build below, produced and verified on
2026-09-06 using `gpt-6-astra`:

- Executable: `artifacts/windows-portable/85013da5/Omarchy-Setup-0.1.0-x64-portable.exe`
- Size: 223,502,480 bytes.
- SHA-256: `8e21ae0e111992c6d62beefc7515ba404e90488daa68c99c760d00374669a408`.
- Record: `artifacts/windows-portable/85013da5/portable-record.json`.
- Existing application hash and embedded provider manifest were checked before
  reuse; all 6,151 provider files and all copied payload files were verified.
- The actual executable passed its fixed `--verify-bundle` mode, which extracts
  and hashes the bundled files using its own Node runtime, without opening the
  app, networking, inspecting host disks, changing firmware or requesting elevation.
- PowerShell 5.1 and JavaScript syntax checks passed. The binary is unsigned.

Normal GUI startup was not tested for this initial build. End-to-end Omarchy
installation remains unqualified. The packaging test does not change the construction,
hardware/boot/recovery or signing limits in the
[Windows readiness report](windows-direct-readiness-2026-09-06.md).

## Optimized build and startup fixes

Final executable: `artifacts/windows-portable/012ef35a/Omarchy-Setup-0.1.0-x64-portable.exe`.
Produced and checked on 2026-09-06 using `gpt-6-astra`.

- Size: **184,500,775 bytes**, down from 223,502,480 (17.5% smaller).
- SHA-256: `1b14f178c89cb04ead240d99acd24dad9050d27fb5514928e5bf874487ed56ac`.
- Extracted payload: **279,865,342 bytes**, down from 433,394,703 (35.4% smaller).
- Release application: 9,878,016 bytes; original debug application: 26,735,616.
- Authenticated provider files: 3,348, down from 6,151.
- Records: `portable-record.json` and `startup-verification.json` beside the executable.

The media stage excludes lockfile-marked development packages, native compiler
outputs and headers, source maps and TypeScript declarations. Native binaries,
package metadata, runtime assets and licenses remain. It includes its own
package.json so compiled CommonJS files do not inherit an enclosing application's
ES-module setting. The Linux construction runtime remains bundled.

The application uses the Windows GUI subsystem in both release and debug builds.
The packager rejects console-subsystem application binaries. The launcher shows
“Extracting application files. Please wait...” before extracting the payload;
the banner plugin precedes the payload in the solid archive. After the app closes,
the launcher changes its working directory out of the payload so Windows permits
NSIS to remove the entire temporary directory.

Checks on the final executable and copied runtime:

- Embedded provider manifest and all staged file hashes matched.
- Staged Node loaded the physical engine's native modules using dependencies
  within the staged runtime. The actual SDK worker wrote a 2,097,225-byte temporary
  file and passed SDK verification, full readback and SHA-256 comparison.
- `--verify-bundle` extracted and hashed the actual payload successfully in 23.70
  seconds. This includes hashing and cleanup and is not an app-startup measurement.
- In the visible launch test, the extraction label was observed after **0.83
  seconds** and the main application window after **11.98 seconds**. The test
  selected only the application belonging to its own launcher, requested normal
  window closure, observed exit code 0 and confirmed removal of the temporary
  payload. These are single-run observations on this machine, not cold-start or
  cross-machine performance guarantees; window appearance does not measure full
  UI rendering or installation readiness.

The final preview remains unsigned. These packaging and startup checks did not
exercise physical USB writing, host-disk deployment, elevation, firmware, boot or
recovery, and do not change their qualification status.
# Subsequent performance update

The latest portable build and USB/existing-ISO measurements are recorded in
[the September 7 report](windows-inspection-existing-iso-2026-09-07.md).
