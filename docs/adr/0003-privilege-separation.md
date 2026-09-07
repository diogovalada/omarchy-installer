# ADR 0003: Separate ordinary-user UI from narrow privileged helpers

- Status: accepted; release-blocking invariant
- Date: 2026-09-02

## Context

Raw removable-media writing and direct installation require administrator/root
authority. Running the complete GUI elevated would expose downloads, rendering,
webview, update and provider code at that authority. Elevating only after a UI
decision is insufficient if the helper blindly trusts the UI.

## Decision

The desktop application always runs as the ordinary user. It displays an exact
immutable plan and requests native elevation only when execution requires it.
A small platform helper then independently authenticates the client, verifies
the signed artifact and plan, re-identifies the target, enforces the operation
allowlist, performs bounded mutation and returns typed progress/receipts.

The privileged API has no shell, arbitrary command, generic file-write, network,
update, plugin-loading or policy-override operation. Messages are versioned,
strictly bounded and replay-protected. Unknown input cannot expand privilege.

## Consequences

- A UI/webview compromise does not automatically become generic root execution.
- Elevation happens just in time for USB or installation mutation; downloading
  and planning require no elevation.
- Helper packaging, identity, IPC and compatibility become critical platform
  responsibilities.
- Safety checks are intentionally duplicated across the unprivileged preview and
  privileged enforcement layers.

## Alternatives rejected

- **Require the whole application to run as administrator:** unnecessarily large
  privileged attack surface and poor user experience.
- **Invoke ad-hoc `sudo`/PowerShell/shell commands:** difficult to authenticate,
  bound and audit; creates injection and path risks.
- **Trust a pre-approved GUI plan:** fails under compromised UI, hotplug and
  time-of-check/time-of-use races.
