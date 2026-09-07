# Governance draft

Status: provisional until repository ownership and maintainers are established

Last reviewed: 2026-09-02

Omarchy Setup is an **unofficial community project**. Governance must not imply
authority over OmaCom, Omarchy, or independently maintained providers.

## Roles

- **Repository owners** manage access, protected branches and incident response.
- **Maintainers** review ordinary changes and keep a defined component healthy.
- **Safety reviewers** are specifically approved for privileged helpers, device
  policy, trust/update, direct installation and diagnostic privacy.
- **Release authorities** approve candidate promotion through protected release
  environments and do not hold all production key material alone.
- **Provider maintainers** retain ownership of their external engines and define
  supported contracts/cells.

One person may hold several roles during incubation, but cannot single-handedly
merge and release a safety-critical change.

## Change approval

Ordinary changes require one maintainer approval and green required checks.
Safety-critical changes require two independent approvals, including a safety
reviewer, and no self-merge. Changes to a release-blocking invariant, privileged
verb, trust root, data collection, direct-install mode, or provider authority
also require an ADR or explicit update to an existing ADR.

Repository protection should require:

- reviewed pull requests and resolved conversations;
- formatting, lint, unit, integration, property and packaging checks as relevant;
- pinned CI dependencies and least-privilege workflow permissions;
- signed tags for public candidates;
- protected, human-approved production release environments;
- separate secrets for development, candidate and production roles.

## Release authority

Simulation-only builds may be produced by CI but cannot enable real devices.
Experimental, Beta and Stable promotion requires a human release authority to
verify the exact support-cell evidence. Stable additionally requires production
signing, recovery ownership, no unresolved high-severity/security/data-loss
issue, and an appropriate independent security review.

Catalog roots and production signing keys must use documented custody, rotation,
revocation and recovery procedures. The protected environment, not a developer
workstation or chat, is the normal signing interface.

## Support and incidents

Support belongs to an exact support cell and named maintainers. A cell without a
recovery path or maintainer cannot be Stable. Security and safety reports follow
`SECURITY.md`; credible wrong-target, out-of-plan, lost-data, boot-loss or trust-
bypass reports freeze the affected cell immediately.

Discord or chat may recruit testers and provide help, but GitHub discussions,
issues, advisories and immutable evidence records are the system of record.

## Contributions and conduct

Use Developer Certificate of Origin sign-off rather than a new CLA unless OmaCom
requires another arrangement for adoption. Preserve third-party notices and
provider attribution. Do not pressure contributors into risky testing or request
credentials, recovery keys, raw disks or remote access.

## Upstream adoption

Transfer, official branding or provider adoption requires explicit agreement
about repository ownership, licensing, issue/security response, signing and key
custody, release authority, support commitments and historical attribution. No
maintainer may independently declare the project official.
