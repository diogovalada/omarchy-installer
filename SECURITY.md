# Security policy

Status: draft; private reporting address pending repository setup

Omarchy Setup is an **unofficial community project** and is not an OmaCom or
Omarchy release. Do not send reports about this project to upstream Omarchy
maintainers unless an upstream component is independently affected.

## Reporting privately

Until the public repository enables GitHub private vulnerability reporting, do
not publish exploitable details. Contact the repository owner privately and ask
for a secure reporting channel without including secrets or proof-of-concept
details in the initial message.

Once repository hosting is established, this section must be replaced with the
canonical private-vulnerability URL and an encrypted-email fallback. A stable
or public hardware-testing release is blocked until that route exists and has
been exercised.

Use private reporting for:

- wrong-disk selection or any out-of-plan mutation;
- data loss or an unrecoverable host/boot environment;
- artifact, catalog, signature, rollback or update verification bypass;
- arbitrary administrator/root behavior or helper/IPC authentication failure;
- provider escape or unauthorized capability expansion;
- sensitive information in diagnostics, journals or uploads;
- release, CI, signing-key or distribution compromise;
- instructions or binaries impersonating an official Omarchy release.

For an active incident, stop using the affected feature, do not retry the
operation, preserve the local operation journal, disconnect the suspect media if
safe, and avoid uploading raw disks, recovery keys, memory dumps or unreviewed
logs.

## What to include

- immutable application build ID and package source;
- catalog, provider and artifact versions where relevant;
- host OS/architecture and the declared support cell;
- expected versus observed behavior and the last completed stage;
- a minimal safe reproduction using simulated/file-backed devices if possible;
- the locally generated diagnostic bundle after preview and redaction;
- whether data, bootability, a non-target disk, or secrets were affected.

Do not include passwords, encryption/recovery keys, disk contents, Wi-Fi details,
full serial numbers, persistent device identifiers or personal screenshots.

## Response targets

These are initial targets, not a warranty:

- acknowledge a credible report within 3 business days;
- triage severity and affected release/support cells within 7 business days;
- immediately freeze public promotion for a credible data-loss, wrong-target,
  boot-loss, privileged-execution, signature-bypass or key-compromise report;
- use signed emergency policy to disable affected catalog/provider cells when
  available;
- coordinate disclosure and credit with the reporter;
- publish an advisory and recovery guidance after a fix is available.

Maintainers will not ask a reporter to repeat a destructive failure on valuable
hardware. Qualification of a fix requires simulation regression tests and fresh
sacrificial-hardware evidence appropriate to the affected cell.

## Supported versions

There are currently no stable public releases. Development snapshots are not
security-supported and must not enable real-device operations by default. This
table will name exact supported release lines when signed candidates exist.

| Version/channel | Supported |
| --- | --- |
| Local development/simulator | Best effort; no real-device claim |
| Experimental hardware candidate | Only for its published support cells and test window |
| Stable | Not yet available |

## Disclosure and upstream coordination

If a report affects an upstream project, maintainers will coordinate privately
with its security contact and avoid disclosing the upstream flaw prematurely.
Project advisories must distinguish Omarchy Setup defects from upstream defects
and must not imply that OmaCom reviewed or endorsed the response.
