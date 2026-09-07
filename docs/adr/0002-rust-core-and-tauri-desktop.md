# ADR 0002: Rust core with a Tauri desktop shell

- Status: accepted for initial development
- Date: 2026-09-02

## Context

The application needs shared, testable safety policy and binary/parsing code on
Windows, macOS and Linux, plus an accessible product UI. Its most sensitive code
handles untrusted catalogs, disk topology, integer bounds, operation journals
and typed IPC. Platform-native privilege APIs still require native adapters.

## Decision

Use a pinned stable Rust workspace for domain policy, catalog/download logic,
device policy, writer primitives, provider contracts, journaling, IPC types and
diagnostics. Use Tauri 2 with Svelte and TypeScript for the unprivileged desktop
shell. Keep every release-blocking decision in Rust rather than the webview.

Use native platform components where security or lifecycle integration requires
them: Swift/XPC and Disk Arbitration on macOS; Windows storage/UAC APIs; and
UDisks2/Polkit packaging on Linux.

## Consequences

- Most safety logic has one implementation and common property/fuzz tests.
- Memory-safe code reduces, but does not eliminate, unsafe I/O and logic risks.
- The UI can evolve without granting it policy authority.
- Native helpers/adapters remain separate deliverables and need platform review.
- Contributors need Rust and frontend toolchains; pinned versions and simple
  task commands are required.

## Alternatives rejected

- **All platform-native applications:** excellent integration but duplicates the
  most safety-critical policy and test corpus.
- **Electron/Node for disk and policy code:** larger trusted surface and weaker
  fit for privileged binary/I/O boundaries.
- **One portable Rust GUI toolkit:** reduces webview surface, but currently adds
  product/accessibility tradeoffs without removing native helper work.
