# ADR 0001: Use a neutral standalone repository

- Status: accepted for initial development
- Date: 2026-09-02

## Context

Omarchy-related installation efforts currently span the official x86 ISO,
Apple-Silicon/Asahi work, and separate macOS and Windows VM projects. Their
platform assumptions, ownership, languages, release processes, and maturity
differ. The community project has no permission to represent itself as official
or to choose an existing maintainer's repository as the universal home.

## Decision

Develop Omarchy Installer in a neutral standalone repository with independent,
clearly unofficial branding. Define shared contracts and integrate upstream
engines through providers or signed handoffs. Do not use the history of the
SwiftUI Apple installer, an official Omarchy repository, or a VM project as the
cross-platform root.

If OmaCom later accepts the project, the repository or individual components may
be transferred or adopted without changing the provider boundary.

## Consequences

- The project can implement non-overlapping safety and media foundations now.
- Upstream provenance and licensing remain visible.
- No fork can accidentally imply endorsement or make one platform architecture
  the default for all others.
- Maintainers must coordinate branding and avoid competing with announced
  direct-install work.
- A future transfer requires deliberate governance, signing and support changes.

## Alternatives rejected

- **Fork the Apple-Silicon SwiftUI application:** useful for that platform, but
  it creates misleading ownership and poor Windows/Linux architectural pressure.
- **Start inside the official Omarchy repository:** requires upstream authority
  and would couple release and safety governance prematurely.
- **Separate unrelated applications only:** avoids coordination but duplicates
  catalog, verification, USB safety and UX work.
