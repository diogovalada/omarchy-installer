# Draft: Shared cross-platform setup and USB foundation for Omarchy

Status: ready-to-post draft; **do not post without repository-owner approval**

Suggested venue: Omarchy Suggestions discussion

---

## Summary

I would like to incubate an **unofficial community project** provisionally named
Omarchy Installer: one safe front door for trying Omarchy, downloading a verified
image, creating installation media, and handing off to qualified platform-
specific installation engines.

This is a request for alignment before substantial public work. It is not a
claim to be an official installer, a request to replace existing projects, or an
attempt to race the announced Windows and Intel-Mac efforts.

## Problem it would solve

Today the relevant journeys live in different places:

- the official x86 ISO covers conventional installation media;
- Apple-Silicon work uses an Asahi-aware direct path;
- macOS and Windows VM projects offer ways to try Omarchy;
- direct Windows and Intel-Mac work has been publicly discussed;
- Windows and macOS users still benefit from a trusted Omarchy-specific USB
  download/write/verify experience.

Users should not have to understand those implementation boundaries before they
can choose “Try,” “Install,” “Create USB,” or “Download image.” The proposed app
would explain what is possible on the current host and delegate to the correct
project. One product experience would not mean one universal disk engine.

## Non-competing scope

The standalone repository would begin with work that is useful regardless of
which direct-install engine wins:

- signed release-catalog and artifact-verification contracts;
- resumable verified downloads;
- removable-media discovery, safety policy, write and full read-back verify;
- an unprivileged GUI with just-in-time, narrowly scoped platform helpers;
- provider/handoff contracts for independent Try and Direct projects;
- operation journaling, redacted diagnostics and an evidence-based support
  matrix;
- simulator, synthetic disks and fault-injection tests before real hardware.

It would **not** reimplement the Apple-Silicon engine or independently build a
competing Windows/Intel direct installer while upstream work is taking shape.
The preference is to integrate maintained engines through signed handoff or a
small agreed protocol. The official Omarchy ISO remains the source for ordinary
x86 installation media.

All public development builds would prominently say “community preview — not an
official Omarchy release.” No Omarchy logo or implication of endorsement would
be used without permission.

## Proposed architecture

- Neutral standalone repository rather than a fork of a platform-specific app.
- Shared Rust core for host capabilities, catalog/download verification, device
  policy, immutable plans, journals and typed provider contracts.
- Tauri desktop shell that always runs as the ordinary user.
- Small native privileged adapters: UAC helper on Windows, signed XPC helper on
  macOS, and packaged Polkit service on Linux.
- Replaceable providers for VM trials, Apple-Silicon installation, forthcoming
  Windows/Intel-Mac engines, and later Linux/QEMU integration.
- No default telemetry; locally generated, redacted, previewable diagnostics are
  uploaded only by an explicit user action.

Nothing would be advertised as physically supported based only on simulation or
CI. Exact hardware/storage/encryption support cells would require independent
sacrificial-device testing and recovery evidence.

## Four coordination questions

1. **Repository and overlap:** Is there already a planned public home or shared
   core for the Windows/Intel-Mac setup work, and which foundation areas would be
   useful without duplicating that work?
2. **Provider boundary:** Would the maintainers of the Apple-Silicon, Windows,
   Intel-Mac and Try projects prefer a signed application handoff, a versioned
   command/IPC contract, or another integration boundary?
3. **Artifacts and trust:** Which canonical release metadata, signatures and
   mirrors should a community downloader/USB writer consume, and is there an
   upstream catalog format or key-rotation plan it should align with?
4. **Name and branding:** May an explicitly unofficial project use the working
   name “Omarchy Installer” and text-only Omarchy references; which name, artwork and
   disclaimer rules should apply now and if the work is later adopted?

## Proposed first milestone

The first milestone would remain entirely non-destructive:

1. publish the threat model, architecture decisions and exact support matrix;
2. implement simulated Try/Install/Create USB/Download journeys;
3. validate signed-catalog rollback, expiry and corrupt-artifact behavior;
4. exercise a file-backed block-device writer with two sentinel non-target disks;
5. demonstrate that release packages cannot accidentally select a real backend.

Only after review would separately enabled Windows/macOS USB candidates be
offered to opt-in testers with disposable drives. Direct-install integrations
would wait for explicit provider coordination and platform-specific evidence.

## Desired outcome

The ideal outcome is a small set of shared safety and user-experience components
that upstream projects can integrate, or a neutral application that faithfully
hands off to them without taking over their ownership. If OmaCom later wants to
adopt some or all of it, repository transfer and release/signing governance can
be discussed separately.

Feedback on overlap, provider interfaces, artifact trust and branding would help
ensure the work contributes to the Omarchy ecosystem rather than fragmenting it.

---

Draft references to add before posting:

- public repository URL;
- architecture and threat-model permalinks;
- a permalink to the exact roadmap revision;
- maintainer/project links with verified current status.
