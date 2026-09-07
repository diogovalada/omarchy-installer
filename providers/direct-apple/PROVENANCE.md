# Source and release provenance

Recorded 2026-09-06; implementation model **gpt-6-astra**.

- Repository: [maralcbr/omarchy-mx-mac](https://github.com/maralcbr/omarchy-mx-mac).
- Initial read-only inspection found main at
  `ab9464cb03243022e5885d6937326d04a0231296`. That source's validation locator
  pinned `.14`, whose published archive was not found in the inspected release
  list. It was not selected as this provider's dependency.
- Selected source:
  [`00daf3ebef9e4fbe89fb9b32bd184e65aa75d357`](https://github.com/maralcbr/omarchy-mx-mac/tree/00daf3ebef9e4fbe89fb9b32bd184e65aa75d357),
  September 2, 2026. This is the parent of the commit that changes the validation
  engine away from `.7`. The source's `ValidationEngineArtifact.swift` names the
  exact published engine size and digest recorded in `release-lock.json`.
- Distribution inspected:
  [v4.0.1-mac.2.9.090126](https://github.com/maralcbr/omarchy-mx-mac/releases/tag/v4.0.1-mac.2.9.090126).
  The tag's source does not describe the attached full-system `.7` engine.
  The selected source is a source/contract reuse pin, not a claim of reproducible
  equivalence with the released GUI or helper binary.
- The 19,402,851-byte PKG was downloaded and matched GitHub's SHA-256
  `6aea628d7ca679fb7c8d15bc0c1d97b88ee4f7e7d12080728ea4137badaab9b8`.
  Only archive members were extracted; no PKG scripts, binary, helper or engine
  were executed. macOS package/Mach-O signature validation was not performed.
- The extracted engine independently matched 17,904,504 bytes and SHA-256
  `063fd0765fb2057384d9653f7bf547b0471af31fc764e039d578d4fef6dce4d5`,
  identical to the selected source's validation locator and release asset pin.
- The extracted Ed25519 catalog signature verified over the exact embedded
  catalog bytes using the extracted public key. The public key SHA-256 is
  `ef073d71048f90295a6bfade93ab8bc258ff108d9fd7674d7a848c7836454869`.
  Catalog sequence is `1788229589`, with expiry `2026-11-30T02:26:29Z`.
  Only `apple,j314s` is enabled. The catalog binds the published `.7` engine,
  metadata, payload digest and individually hashed split payload downloads.
- The original `release.json` points its online catalog URLs at loopback HTTPS;
  the PKG includes the sealed catalog/signature pair used by the retained loader.
  Packaging must retain that pair. The adapter does not replace it with a newly
  authored catalog, remove expiry, broaden models or invent an online channel.
- The daemon's accepted client is upstream bundle ID `com.omarchy.mx.installer`
  and Apple Team `T2C384FJBD`; `release.json` reciprocally requires that team's
  `com.omarchy.mx.installer.helper`. These identities cannot authenticate our
  independently signed companion. Assembly creates explicit requirements for
  the distribution's real Team ID and builds the unchanged helper source.
- The selected source's engine source-lock overlay labels `.6`, while its
  validation locator and published sealed catalog name `.7`. The released engine
  archive remains a separately pinned binary input. Do not claim rebuilding the
  source lock reproduces that archive without separate reproducibility evidence.

The adapter's additional policy is restricted to the published catalog's single
model, refusing existing-install candidates, exact request/plan binding, signed
parent admission, serialized commands, and truthful cancellation semantics. It
retains upstream TrustCore, Inspector, InstallerSession, live environment,
execution coordinator, authenticated helper and Recovery retry policy. No
submodule files were modified, no fork was created, and only `.gitmodules` plus
the gitlink were staged as part of adding the submodule.

No test suite, Swift compilation, signing, notarization, helper registration,
elevation, installation, physical disk operation or publication occurred. Source
review and read-only package/signature inspection are the available evidence.
The public release's existence and valid embedded catalog are not proof that
this new companion builds or installs correctly. macOS compilation, protocol
qualification, signed packaging and hardware acceptance remain deferred work.
