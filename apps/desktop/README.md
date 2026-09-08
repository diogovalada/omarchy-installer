# Omarchy Installer desktop

Tauri 2, Svelte and TypeScript. The download path uses native Rust commands for
real official release discovery, HTTP download/resume, size and SHA-256 checks,
and detached OpenPGP verification. The single setup screen shows a download,
its destination and progress, followed by the USB and install actions.
The webview cannot supply URLs, keys or paths.

The two setup actions now open native disk/requirements flows. USB creation uses
the pinned Etcher SDK through a separately elevated copy of this executable.
Windows direct installation builds a system locally from the signed ISO, then
deploys its ESP and LUKS2/Btrfs partitions into reviewed free, shrunk or explicitly
replaced space. Apple Silicon
uses the retained native coordinator through a private Swift companion; its image
is downloaded by that backend and does not use the x86 ISO.

The app defaults to the user's `Downloads` directory. Change opens a native
folder chooser; changing folders is blocked during an operation. A new folder
clears the previous completion state without deleting or moving saved files.
The next download reuses and verifies the local cache before exporting there.
Existing identical images are reused; different existing files are left intact.
Partial downloads remain in the application cache for resume after cancellation
or restart. The app never marks an image complete before verification and export.

## Development

From the repository root, with Rust 1.93, Node 22 or 24 and pnpm 10.2.1:

```powershell
pnpm install --frozen-lockfile
npm --prefix providers/media-etcher ci
pnpm providers:stage
pnpm desktop:check
pnpm desktop:test
pnpm desktop:build
pnpm desktop:test:native
pnpm desktop:native
```

`desktop:dev` opens the Vite browser preview; native actions are unavailable there.
`desktop:native` runs the actual Tauri app. `desktop:build:native` produces an
unsigned debug executable with embedded frontend assets and no installer bundle.
Windows requires the ordinary Tauri WebView2/MSVC build prerequisites.

The default Windows distribution is one portable executable. Run
`pnpm desktop:package:windows -UnsignedPreview` to create an optimized release
`Omarchy-Installer-0.1.0-x64-portable.exe` under `artifacts/windows-portable/`. Open
that executable directly. On first launch, an opening indicator appears while
the launcher extracts and verifies its app and provider files into
`%LOCALAPPDATA%\OmarchySetup\p\<id>`. Later launches verify and reuse that cache.
Normal exit retains the payload and removes temporary verifier files.
See [portable cache behavior](../../docs/portable-cache-2026-09-07.md).
There is no installation wizard,
manual ZIP extraction, Start menu registration or uninstaller. Downloads,
caches and protected operation records still use their normal locations.
WebView2 and the direct-install Docker prerequisite still apply.

The portable packager checks the Windows GUI subsystem and that the exact provider manifest is embedded in
the executable, verifies every copied provider file, includes the construction
runtime archive and runs the launcher's fixed `--verify-bundle` mode to check
actual extraction and all payload hashes without opening the app or accessing
host disks, networking, firmware or elevation. It also loads the staged native
modules and checks the SDK worker with a verified temporary-file write. The
locked production dependencies retain native binaries, licenses and runtime
assets; development packages, native build artifacts and declarations are omitted.
Its output includes
`portable-record.json` with hashes. The maintainer-only `-ReuseVerifiedBuild`
option additionally binds an existing executable to its previous package record.
An unsigned preview is not a signed or installation-qualified release.

`-DebugBuild` remains available for development; Windows debug builds also use
the GUI subsystem so an attached console cannot control the application's life.
For a visible launch/close check, run `scripts/verify-portable-startup.ps1
-Executable <absolute-exe-path>`. It records extraction-indicator and main-window
timings and verifies cleanup after requesting a normal close of its test instance.

The launcher uses the verified NSIS compiler from Tauri's packaging toolchain as
a self-extractor. Its script contains no installation/registry/shortcut actions
and runs as the ordinary user. App argument passthrough is deliberately absent.
Normal app exit is already blocked during active setup work; the elevated
helper independently copies verified providers into its protected workspace.
The downloaded ISO stays in place and is opened through a validated, read-only
handle; the helper verifies its release checksum and signature before use.
Abrupt process termination can leave temporary files. The build also retains an
extracted development payload for review; users only need the outer executable.

The earlier NSIS distribution remains available only through the explicit
`desktop:package:windows:installed` command. Its certificate/timestamp options
are retained for development; it is not the default user journey.

Native dev/build commands stage the host-specific provider runtime automatically.
The staging manifest hashes are compiled into Rust; privileged operations copy and
verify those exact bytes into an administrator-owned directory before execution.
Run staging again after provider changes. Keep the generated providers resources
beside a copied executable; a bare executable is not a portable distribution.

The desktop has its own `Cargo.lock` and workspace because native GUI dependency
requirements differ from the pure Rust core. SHA-256 and OpenPGP dependencies
are optimized in debug builds so multi-gigabyte verification stays practical.

## Current boundaries

- USB selection/write/verification and Windows alongside deployment are wired.
  Disk inspection decides eligibility; the native helper rechecks before writing.
- Windows direct installation requires UEFI/GPT, 512-byte logical sectors,
  Secure Boot off, about 40 GiB or more selected allocation, a pinned local Docker
  runtime, 85 GiB of working storage after source staging, and 10 GiB of free
  memory. It can use unallocated space or shrink an eligible NTFS partition within
  Windows-reported limits and a retained reserve. The final confirmation shows
  exact before/after sizes. Replacing the running Windows OS is unavailable.
- Separate preparation actions import the packaged runtime or request a reviewed
  restart into firmware settings to disable Secure Boot. The latter coordinates
  bounded protection restoration; users return to Windows and inspect again.
  Docker Desktop and any prerequisite Windows feature setup remain external.
- A separate partition-replacement option can delete an eligible unused partition
  after a warning modal, typed `Disk N Partition M` and final native plan approval.
  It protects current OS/boot/recovery/paging/installer storage and refuses unknown
  or encrypted layouts. This path has only static/build validation, not disk tests.
- Local construction uses upstream's per-install LUKS2 encryption and deferred
  owner setup. Personal protection begins when first boot replaces the bootstrap
  unlock key. The construction disk and exported partitions are encrypted under
  an operation key carried through private stdin and discarded when the helper
  ends. A larger target grows its LUKS mapping and Btrfs on first boot.
- Fully encrypted, unlocked Windows volumes are supported by the development
  path. Required active BitLocker protection is temporarily suspended and restored
  with verification; a protected SYSTEM recovery task covers interruption. Volumes
  already suspended retain their original state. Incomplete encryption and unknown
  or locked affected volumes are refused.
- Mac direct installation requires the signed packaging documented in
  [the Apple provider](../../providers/direct-apple/README.md). The pinned catalog
  admits only `apple,j314s`; it does not establish all-Mac support.
- Windows and Mac use shared storage selection, allocation and typed deletion
  controls. Mac replacement is limited to installations identified by the native
  Omarchy backend; it uses the entire installation and preserves native plan
  review, helper authorization and Recovery handling. Alongside placement uses
  the native free-space/macOS-resize recommendation.
- Windows preparation exposes preferred OS and countdown controls, defaulting to
  Omarchy / five seconds. Native approval repeats the settings; firmware starts
  the menu first. Selecting Windows briefly restarts through its existing entry.
  Mac explains Apple's power-button startup picker and Recovery completion.
  [Platform behavior](../../docs/platform-behavior.md) is the shared user/team guide.
- There is no sidebar or separate overview/download navigation.
- Host labels come from the native process. The x86 ISO target is separate from
  the machine running the app; it is not an Apple silicon installation image.
- Browser previews explicitly report unavailable native actions.
- Old unreferenced prototype components remain as historical source, and are
  not mounted or packaged into the active workflow.
- The app is not signed, packaged for distribution, or qualified on macOS/Linux.
- New disk-operation tests and physical/VM execution are deferred at the user's
  request. Implementation and successful compilation do not establish bootability.

The first implementation evidence and outstanding work are recorded in
[the execution report](../../docs/evidence/implementation-2026-09-05.md).
Current integration details are in
[the September 6 report](../../docs/evidence/native-options-2026-09-06.md) and
[the encryption and allocation update](../../docs/evidence/encryption-allocation-2026-09-06.md).
