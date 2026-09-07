# ADR 0004: Integrate Try and Direct engines through versioned providers

- Status: accepted for initial development
- Date: 2026-09-02

September 5 update: the provider boundary remains relevant, but the original
preference for signed app handoff below is superseded by the current
[installation strategy](../installation-plan.md): shared Tauri screens with a
retained native Apple backend, local x86 image construction/deployment, and one
Etcher SDK media backend. Try is outside v1. Exact contracts remain to be built.

## Context

VM and direct-install engines differ by platform and already have independent
projects or announced development. Reimplementing them would duplicate risky
work and compete with upstream maintainers. Hard-coding them into the desktop UI
would make security policy, support and updates inseparable.

## Decision

Define a versioned provider lifecycle for host probing, capability evaluation,
preflight, immutable planning, authorization, execution, cancellation, resume,
recovery and diagnostics. Providers are catalog-authorized for exact versions,
protocol ranges and support cells.

Prefer a verified signed handoff initially. Use deeper IPC only with maintainer
agreement, compatible licensing, reciprocal identity checks and a stable typed
contract. The shared application never claims unsupported provider capability.

## Consequences

- Existing Apple-Silicon and VM work can be integrated without source capture.
- Newly public Windows or Intel-Mac engines can be added without redesigning the
  product.
- Provider failure, update and emergency disablement can be isolated.
- UX consistency is constrained by what external providers expose.
- Protocol, support ownership and release coordination must be documented.

## Alternatives rejected

- **One universal installer engine:** ignores incompatible firmware, storage and
  recovery models and concentrates destructive risk.
- **Copy upstream source into a monolith:** obscures provenance, complicates
  updates and creates a maintenance fork.
- **Launch arbitrary executables:** provides no version, capability, identity or
  support-cell control.
