# September 5 implementation evidence

Status: native download and file-backed Etcher work implemented and verified.
Local x86 image construction remains unproven after two bounded VM attempts.

## Scope and dependency decisions

The user authorized implementation after accepting the local-build/deploy
strategy. This first work package implements downloads and tests the selected
engines against files and virtual disks. It does not enable physical USB writes,
partitions, boot changes, native installation, publication or upstream messages.

- Mac backend source will be a Git submodule pinned to an inspected commit, with
  the Tauri/native adapter outside it. The submodule has not been added yet.
- Etcher SDK is a pinned npm dependency (`10.2.14`) with its own lockfile.
- Asahi/Mac payloads remain separately pinned release artifacts when integrated.
- The official ISO is downloaded and verified, never added to Git.

## Native download slice

`crates/release-client` implements official HTTPS release discovery, exact size
and SHA-256 checks, native detached OpenPGP verification with the pinned upstream
key, resumable HTTP and cancellation. It extends the existing downloader with
cancellable cache/partial rehash and pre-promotion verification. Torn resume
metadata and corrupt complete data can recover on retry.

Four Tauri commands connect that library to a shared Svelte DownloadPanel. Native
state owns the release and destination. The webview supplies no URL, key, path or
release metadata. The final copy is rehashed, synced and published without
replacing an existing destination. Existing identical files can be reused.

The active UI removes simulated Try/Manage, invented devices, fake preflight
results and simulated completion. Host labels are native facts. Install and
physical USB writing are visibly unavailable. Download and USB image preparation
share one operation across navigation. Browser preview actions are unavailable.

Observed on Windows x64, Rust 1.93.0, Node 22.13.1:

| Check | Result |
| --- | --- |
| Release-client fixtures | 12 passed; live metadata test separately passed |
| Existing downloader plus new callbacks | 11 passed |
| Native state/export tests | 3 passed |
| Svelte/controller tests | 8 passed |
| Svelte type checking | 0 errors; 4 existing warnings in unreferenced prototype files |
| Frontend production build | Passed |
| Native debug executable with embedded frontend | Built and launched successfully |
| Targeted crate formatting and release/downloader Clippy | Passed |
| Native desktop Clippy, all targets with warnings denied | Passed |
| Complete official ISO authentication in native Rust | Passed, all 6,227,752,960 bytes |
| Native UI release lookup | Resolved actual Omarchy 4.0.2, 5.80 GiB |
| Native UI cancellation/restart/resume | Cancelled after 1,716,673,792 bytes; app closed and reopened; same partial resumed through the USB page |
| Native UI final verification/export | Passed; exact 6,227,752,960-byte ISO saved to `C:\Users\example\Downloads\Omarchy\omarchy-4.0.2.iso`; UI reports verification passed |

Official ISO SHA-256:
`2ef8e624aa1bec7e277e28056b8535a6c9373ba48d7ede3f1a01cb6d2373cfb8`.
Pinned signing fingerprint:
`40DFB630FF42BCFFB047046CF0134EE680CAC571`.
See [the client contract](../../crates/release-client/README.md) for immutable
source attribution, HTTP policy and trust limitations.

The native executable is a local unsigned debug build, not a distribution
package. Windows UI testing uses the actual Tauri/WebView2 app. Other host builds,
key rotation/revocation, cryptographic freshness and protected handles for future
privileged consumers remain unqualified.

## Etcher SDK adapter

`providers/media-etcher` wraps the actual SDK with a typed file-only API. It
validates source content, exclusively creates a new regular output file, requires
SDK verification and full SHA-256 readback, and reports cancellation only after
the hidden worker closes. It does not enumerate or open physical drives.

All 13 tests passed, including failure/corruption, existing-file preservation,
path/link rejection, cancellation, extra IPC properties and worker cleanup.
Native SDK modules loaded on Windows. A relocated Node/runtime directory wrote
and verified a 1,048,699-byte fixture successfully. See [provider evidence](../../providers/media-etcher/README.md).

The dependency audit has four moderate affected-package entries and no
high/critical entries. Signed packaging, full dependency/license review, other
host runtimes and physical media policy remain production gates.

## Image construction

The isolated Docker/QEMU harness consumes the signed official ISO and adapts its
upstream unattended base-image recipe. Actual release inspection found cidata
and deferred-provisioning support. The runtime and all 125 changed/added packages
are pinned; UEFI smoke and all 12 focused harness tests passed.

The first real installation reached the official installing screen, then Docker
killed QEMU for exceeding its 5 GiB memory cap. The staging image was never
promoted. A retry used the same 4 GiB guest with a 7 GiB container limit after an
8 GiB host-free preflight passed. It reached its configured 1,800-second install
timeout while the official installer was still running. The staging file reached
2,542,927,872 bytes; peak container memory was 5,744,263,168 bytes (5.35 GiB),
with zero memory-limit or OOM events. No output was promoted and no proof
container remains running.

The actual release can start unattended and write the virtual disk. Completion
and independent boot remain unproven. The next experiment needs better installer
stage diagnostics and qualification of an accelerated runtime, retaining the
file-only target boundary. This software-emulated timeout does not establish
that local construction is impossible or that physical deployment is ready.

Review also found an SSH probe defect: failed live-ISO authentication could pin
the live system's host key and prevent later installed-guest authentication.
A separately recorded continuation handled discovery during the run; it never
authenticated an installed guest before the timeout. The permanent fix now
discards failed discovery keys, pins only an authenticated installed guest, and
requires that pin on independent boot. Transient SSH timeouts are retried within
the outer bound, and root-device validation rejects similarly named partitions.
The 12-test result includes those regressions. These post-run changes have not
been exercised by another full installation; exact before/after hashes and
original source snapshots are retained in the evidence directory.

See [image-build evidence](image-builder/README.md) for receipts and exact limits.
An independent boot with fresh NVRAM and no ISO/cidata must succeed before an
installed VM proof is claimed. No such test qualifies physical deployment,
encryption, hardware finalization, partition extraction or filesystem growth.

## Review and provenance

No more than four workers were active at once, including the coordinator. Exact
resolved models for delegated work:

| Worker | Model | Effort | Scope |
| --- | --- | --- | --- |
| verified_release_client | gpt-6-astra | xhigh | Release verification, HTTP/cache integration |
| usb_sdk_integration | gpt-6-astra | high | Etcher file-only adapter |
| local_image_feasibility | gpt-6-astra | xhigh | Isolated image-build proof |
| review_etcher_adapter | gpt-6-astra | xhigh | Independent Etcher review |
| review_download_slice | gpt-6-astra | xhigh | Independent native download review |

Reviewed defects fixed: synchronous IPC worker leakage, drive-dependent Windows
paths, corrupt-complete download retry, torn resume metadata, and cancellation
during cache/partial hashing. The coordinator inspected the returned changes
and exercised the actual app.

A workspace-wide format check exposed existing formatting differences in
unrelated core/provider files. Targeted formatting for changed crates passes;
those baseline files were left unchanged. Cargo-deny is not installed locally.
CI now includes the Windows native download tests and pinned Etcher file tests;
no remote CI run is claimed.

Automatic approval review rejected deletion of generated mobile icon files.
They remain locally and are excluded from Git; the desktop icons are retained.
No permission question or destructive retry was issued.
