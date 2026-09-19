# USB engine and portable packaging options

Recorded: 2026-09-19 (Europe/Lisbon), consolidating the September 18–19 discussion.
Status: alternatives under consideration; no replacement engine selected or
implemented. Etcher remains the current provider. This records options, not a
commitment to migrate or a claim that another engine has passed our tests.

## Problem and scope

The user reports that preparing the portable executable at launch takes too
long. They want a smaller application, less extraction, and continued upstream
maintenance of OS compatibility rather than an unnecessary private rewrite.

The discussion concerns the whole-device USB image writer and its discovery
dependencies. Replacing that writer does not implement the keep-files USB route,
partition resizing, staged-ISO direct installation, or Apple Silicon installation.
Those have separate providers and requirements. Host OS/architecture support also
does not establish that the image boots on that host's hardware.

## Corrections to earlier explanations

- We bundle **Node.js**, not a TypeScript runtime. The provider's TypeScript is
  compiled to JavaScript at build time; TypeScript is a development dependency.
- The webview already has a JavaScript engine, but it does not supply Node's
  filesystem/process APIs or load Node native addons. Etcher cannot run there
  unchanged. A browser engine executes the language; its host supplies capabilities.
- WebAssembly in the webview does not grant raw disk access. A WASM port would
  still need a native bridge and adaptation of the original dependencies.
- Native libraries **do exist**. Earlier searches missed relevant crates, notably
  `flashkraft-core` and `imi-core`. Do not turn “not yet assessed as a replacement”
  into “no C/C++/Rust library exists.”
- A Rust implementation is not inherently more vulnerable to OS updates than
  Node code calling the same OS APIs. Maintenance depends on who owns the platform
  integration, its testing, and how upstream fixes reach us.
- Invoking Raspberry Pi Imager versus extracting USBImager was a comparison of
  integration methods, not a claim that only Raspberry Pi maintains its engine.
  An upstream-supported USBImager CLI/library could offer similar update benefits.
- These proposals are not all mutually exclusive: trimming, JS bundling, and
  preparing the provider on demand can be combined.

## What the current provider uses

The adapter uses pinned `etcher-sdk` 10.2.14 through `File`, `BlockDevice`, and
`pipeSourceToDestinations`, with SDK verification enabled. Native dependencies
include `@ronomon/direct-io`, `mountutils`, and `drivelist`.

Our code already owns substantial behaviour around the SDK: authenticated source
handling, target exclusions and reidentification, Windows volume locking, handle
geometry checks, bounded final-sector padding, cancellation through a child-process
boundary, flushing, and an additional full SHA-256 readback. We override the SDK's
normal device opening rather than accepting its default Windows disk preparation.

Etcher offers a reusable upstream implementation rather than requiring us to
extract application internals. That is useful, but shipping Node and a broad
dependency tree is a cost of this integration, not a fundamental USB requirement.
Removing Node requires replacing or adapting discovery and helper code as well
as the write loop.

Local references: [physical engine](../providers/media-etcher/src/physical-engine.ts),
[discovery](../providers/media-etcher/src/physical-discovery.ts),
[runtime staging](../providers/media-etcher/scripts/stage-physical-runtime.cjs), and
[provider behaviour/test record](../providers/media-etcher/PHYSICAL-PROVIDER.md).

## Packaging measurements and estimates

Baseline: local Windows **debug/testing** package `c324e21a`, executable
`Omarchy-Installer-0.1.0-x64-testing.exe`. Its ignored local
`artifacts/windows-portable/c324e21a/portable-record.json` records the exact files,
hashes and launcher. These figures are not measurements of every release/platform.
All sizes below are MiB (1,048,576 bytes).

| Component | Extracted size | Files |
| --- | ---: | ---: |
| Node executable | 79.36 | 1 |
| Dependencies, including the separate inspection dependency tree | 62.56 | 3,339 |
| Application executable | 25.84 | 1 |
| Other media-provider files | 0.40 | 48 |
| Remaining files | 9.32 | 28 |
| **Total payload** | **177.48** | **3,417** |

The downloadable executable is **75.34 MiB**. Its packaging record measured
16.44 seconds for extraction plus hash verification, not normal GUI startup.

Five large pruning candidates are:

| Package | Extracted size | Separately recompressed size |
| --- | ---: | ---: |
| `node-raspberrypi-usbboot` | 26.77 | 13.26 |
| `lzma-native` | 7.76 | 3.46 |
| `winusb-driver-generator` | 7.75 | 6.10 |
| `usb` | 6.32 | 2.33 |
| `@balena/node-beaglebone-usbboot` | 0.61 | 0.36 |
| **Total, approximately** | **49.2** | **25.5** |

They did not appear in `require.cache` after loading the staged physical engine
and discovery dependencies. That is initialization evidence only: lazy imports,
error paths and other platforms still need checking. Their removal is not yet
qualified. In particular, format/decompression support must match the actual
source types we promise, rather than be removed solely because one probe did not
load it.

The compression experiment applied Node `zlib.deflateRawSync` at level 9 to each
payload file. It estimated 78.01 MiB for all files, versus the actual 75.34 MiB
launcher. It is an approximation of compressed contributions, not a rebuilt NSIS
package; packaging/deduplication differences prevent exact subtraction.

The discussion's working estimate for trimming and bundling was:

- Download: about **45–55 MiB**, saving roughly **20–30 MiB**.
- Extracted payload: about **120–130 MiB**, saving roughly **50 MiB**.
- Node's 79.36 MiB extracted executable remains. Additional consolidation mainly
  reduces file count; no measured startup-speed improvement is available yet.

These are estimates, not promised results. A replacement engine's final size is
also unknown; native compilation alone does not prove a small dependency footprint.

### Extraction versus verification

The launcher already caches the exact payload in
`%LOCALAPPDATA%/OmarchySetup/p/<payload-id>`. An unchanged valid cache skips full
payload extraction, but still checks the manifest and hashes files. New builds
with changed payloads get a new cache; a missing/damaged cache is rebuilt.

Measure cold extraction, warm verification and time to the visible interface
separately before blaming all delay on extraction. Consolidating thousands of
files can affect both extraction and verification. Historical timings in
[the cache record](portable-cache-2026-09-07.md) came from different builds and
must not be presented as measurements of this testing executable.

## Possible approaches

| Approach | Advantages | Costs and unresolved questions |
| --- | --- | --- |
| **Keep Etcher/Node; trim runtime dependencies and platform binaries** | Smallest architectural change; keeps upstream SDK behaviour and updates. | Node remains. Audit transitive/dynamic imports and preserve required notices, metadata and native addons; validate each host build. |
| **Bundle required JavaScript into fewer files** | Reduces filesystem/extraction/hash overhead; can combine with pruning. | Native addons and runtime assets still need packaging. CommonJS/dynamic imports and addon path discovery complicate bundling; size savings are not proportional to file-count savings. |
| **Prepare/load the USB provider on demand** | Show the interface without first preparing all USB dependencies. | Moves some waiting to the USB flow; does not by itself shrink the download. Discovery also currently needs the provider. Integrity checks must still complete before execution. |
| **Use an existing native Rust/C/C++ library** | Potentially removes Node while retaining upstream ownership of an engine; no need to recreate every operation. | Need to assess API, coverage, safeguards, dependencies, release history and hardware evidence. A published library is not automatically an equivalent replacement. |
| **Invoke an upstream native CLI/helper** | Maintainers keep their engine intact; our app owns the protocol adapter and packaging. | Verify machine-readable progress/errors, cancellation, source authentication, device identity and privileges. Dependencies may still be large. Upgrading still requires our tests. |
| **Wrap/extract an application's native disk layer** | Reuses working platform code; C/C++ can be called from Rust without rewriting it. | We own the wrapper and any maintained patch/fork; upstream changes may require merges. Prefer an accepted upstream library/CLI interface where feasible. |
| **Implement only our required functionality in Rust** | Removes Node and unnecessary SDK features; fits the current backend. | We own the writer and platform compatibility. The core copy/hash loop is simpler than robust locking, topology, geometry, cancellation and physical verification across three OSes. |
| **Keep Etcher JavaScript in the webview and add a native bridge** | Reuses the existing JS engine and compatible upstream JS logic. | Must replace/adapt Node filesystem, streams/process interactions and native addon integration. Platform fixes in upstream native modules do not automatically transfer to our bridge. Scope needs a prototype, not a presumed full Node emulator. |
| **Port relevant code to WebAssembly plus a native bridge** | Could reuse suitable compiled components within a WASM environment. | Etcher is not automatically made browser-compatible by compiling to WASM. The native disk bridge remains necessary; no demonstrated advantage over ordinary webview JS or native Rust for this workload. |

A bridge is not ruled out. If the required interface is small and stable, it
could retain enough upstream logic to be worthwhile. The earlier ranking of a
Rust rewrite ahead of a browser bridge was provisional; neither has a prototype
or measured engineering cost here.

## Candidate libraries and engines found

Research status is as of this discussion. Documentation/source inspection is
separate from compilation, integration tests and physical qualification; none of
the alternatives below has been qualified in this app.

| Candidate | Form and useful scope | Limits / follow-up |
| --- | --- | --- |
| **[Etcher SDK](https://github.com/balena-io-modules/etcher-sdk)** | Current Node SDK, cross-platform image pipeline and native dependencies. | Keep as the baseline; trimming/bundling are options, not a decision to abandon it. |
| **[FlashKraft Core](https://github.com/sorinirimies/flashkraft/tree/main/crates/flashkraft-core)** | Published MIT Rust library, separate from GUI/TUI; discovery, write pipeline, verification, progress and cancellation. Source includes Windows/Linux/macOS branches. | Especially relevant to direct library reuse. Review actual behaviour on each OS and privilege assumptions; platform branches/docs alone do not establish safe equivalent support. |
| **[imi-core](https://docs.rs/imi-core/latest/imi_core/)** | Rust library with a complete phased write/verify pipeline, exclusive locking, cancellation and GUI callbacks. | Linux only; possible Linux provider or reference rather than one replacement for all hosts. |
| **[Argos](https://github.com/jp-guimaraes/argos)** / [argos-core](https://docs.rs/argos-core/latest/argos_core/) | Rust core with write/verify logic and separate platform crates; explicitly structured for reuse. | Linux/macOS hosts; Windows-as-host is explicitly out of scope. Its Windows installer-media support must not be mistaken for Windows host support. Young project; assess release/qualification evidence. |
| **[AgenticBlockTransfer / abt](https://github.com/nervosys/AgenticBlockTransfer)** | Rust library and optional CLI/UI features; advertises cross-platform device enumeration, writing and verification. | Additional candidate found in the later search, not reviewed for adoption. Broad feature claims need source/test validation; repository is AGPL-3.0. |
| **[Armbian Imager](https://github.com/armbian/imager/blob/main/DEVELOPMENT.md)** | Native Rust/Tauri application with Windows, macOS and Linux discovery/write/verify backends. | Relevant application backend, not an independently packaged flashing SDK. Extraction or upstream library work needed. Package declares GPL-2.0-or-later. |
| **[Raspberry Pi Imager](https://github.com/raspberrypi/rpi-imager)** | C++ application with [an upstream CLI](https://github.com/raspberrypi/rpi-imager/blob/main/src/cli.cpp) and explicit `BUILD_CLI_ONLY` build option. | [CLI-only build](https://github.com/raspberrypi/rpi-imager/blob/main/src/CMakeLists.txt) still links Qt Core/Network and other libraries. Measure its packaged size and integration contract; do not assume it is dependency-free. |
| **[USBImager](https://gitlab.com/bztsrc/usbimager)** | Small MIT C application, platform-specific disk code, image writing and verification on Windows/macOS/Linux. | No complete headless image+target write interface in its documented CLI flags. Consider a thin native wrapper or proposing a library/CLI upstream. Published app size is not a measurement of our integration. |
| **[Rufus](https://github.com/pbatard/rufus)** | Native C application and useful Windows storage implementation/reference. | Windows host only, not a ready-made three-platform SDK; GPL codebase. |
| **[Caligula](https://github.com/ifd3f/caligula)** | Rust imaging application with Linux and limited macOS support. | Windows support remains planned according to its README; not a replacement for the current Windows path. |
| **[libblockdev](https://github.com/storaged-project/libblockdev)** | C storage-management library with partition/filesystem/device plugins. | Linux storage stack, not a complete cross-platform image-flashing SDK. |

Other building blocks found are useful to distinguish from a complete engine:

- **[livedisk-core](https://docs.rs/crate/livedisk-core)**: cross-platform disk and
  partition enumeration, not the complete write/verify workflow.
- **[whichdisk](https://docs.rs/whichdisk/latest/whichdisk/)**: resolves a path's
  backing volume/disk; potential topology building block, not a flasher.
- **[disk-types](https://github.com/pop-os/disk-types)** and **[gpt](https://docs.rs/gpt)**:
  storage types / partition-table operations. They do not supply the full writer
  or safely shrink the filesystem inside a partition.
- **[nusb](https://docs.rs/nusb)**, `rusb`/libusb, and
  **[usbh-fatfs](https://docs.rs/usbh-fatfs)** / `usbh-scsi`: USB protocol and
  userspace storage/FAT access. These are a different integration layer from
  writing an OS-managed physical disk; driver ownership, permissions and device
  compatibility would require assessment. Cross-platform USB transfers alone
  do not provide our disk safety workflow.
- **[fstool](https://github.com/KarpelesLab/fstool)**: image/filesystem creation and
  manipulation, with some host device support; not established here as a complete
  Windows/macOS/Linux USB-provider replacement.

### Raspberry Pi branding and general imaging

Raspberry Pi Imager primarily prepares boot media for Raspberry Pi computers and
offers Pi-specific image catalogues and customisation. Its underlying image
writer can accept custom source images. Our proposed use would write the Omarchy
image for its intended PC without Pi customisation; no Raspberry Pi hardware is
required. This is a reuse proposal, not a claim that our ISO/provider integration
has already been tested. See [custom-image documentation](https://www.raspberrypi.com/documentation/computers/getting-started.html#install-using-imager).

## Maintenance and upstream updates

| Integration | Maintenance we retain |
| --- | --- |
| Upstream SDK/library dependency | Adapter, version updates, packaging, compatibility/regression tests and our additional protections. Upstream fixes arrive only when we adopt a release containing them. |
| Upstream executable/CLI | Process/protocol integration, version pinning, packaging, privileges, progress/cancellation and testing. Upstream retains its engine implementation. |
| Copied/extracted engine or private patches | All of the above plus tracking and merging upstream changes. A thin wrapper may keep this manageable; copying source does not make it an automatically updating dependency. |
| Browser/WASM native bridge | Bridge APIs and native behaviour in addition to compatible upstream JS/WASM components; OS fixes may need manual adaptation. |
| Own Rust engine | Our implementation and its OS/hardware behaviour, while avoiding Node/native-addon runtime compatibility work. |

Neither native code nor an upstream dependency guarantees future OS compatibility.
Preserve the practical benefit of other maintainers' fixes where possible. A
small reusable interface accepted upstream could be preferable to a long-lived
private fork. No upstream messages or proposals have been sent in this work.

## How to compare before choosing

1. Keep the current provider as the reference. The latest recommendation is to
   evaluate native library reuse (especially `flashkraft-core`) before assuming
   a private Rust rewrite or browser bridge is necessary. Raspberry Pi's CLI and
   USBImager's small native layer remain separate candidates.
2. Compare required behaviour, not advertised feature count: target/system/source
   disk exclusions; stable source/device identity; sector alignment and bounded
   writes; retained volume/device locks; flushing/readback; cancellation and
   error reporting; progress; ejection; privilege separation.
3. Assess API stability, current platform implementations, licences/notices,
   dependency size and evidence of maintained hardware support. Preserve our
   source authentication and confirmation boundaries through any replacement.
4. Prototype with ordinary image files and simulated failures first, then
   qualify physical devices per host/architecture. A successful build or
   dependency-load probe alone is insufficient.
5. Measure actual release package size, extracted size/file count, cold/warm
   startup and USB preparation time. Avoid turning the current estimates into
   benchmark results. Retain Etcher during evaluation; do not ship multiple
   production writers indefinitely without a reason.

Trimming/bundling remains the lowest architectural-risk improvement to the
existing implementation. Native library reuse may better address both runtime
size and long-term maintenance, but no winner has been established.
