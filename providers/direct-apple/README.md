# Apple native installation bridge

This provider puts the Omarchy MX Mac native installer behind the shared Tauri
interface. It supplies a long-lived NDJSON companion, real read-only host/layout
inspection, retained plan preparation and approval, authenticated helper execution,
verified journal progress, and Recovery instructions. It does not open the upstream
SwiftUI app. The source is implemented but has not been compiled on macOS or tested
on hardware; testing was explicitly deferred for this work.

```text
Tauri process (signed parent)
  -> private inherited stdin/stdout pipes
  -> signed Omarchy Apple Bridge.app
  -> upstream InstallerSession + LiveInstallerEnvironment
  -> InstallerExecutionCoordinator + authenticated XPC
  -> signed, preinstalled root helper
  -> pinned Asahi .7 engine + signed catalog/payloads
  -> journal/read-back + human Recovery authorization
```

The submodule is pinned to
`00daf3ebef9e4fbe89fb9b32bd184e65aa75d357`. `Package.swift` links the upstream
TrustCore and UXCore products. `scripts/prepare-sources.py` copies two nonvisual
application files, byte for byte, into an ignored build target alongside our
adapter sources. The submodule is unchanged. The source revision's validation
locator names exactly the `.7` release engine's digest and size; we do not claim
the released GUI executable was built from this revision. The release tag itself
contains older engine source. Source and asset pins are therefore separate.

The published package's sealed catalog enables **only `apple,j314s`**. The bridge
requires macOS 15 or later and a native arm64 process. Intel Macs, Rosetta, other
Apple models are refused. This is release
catalog eligibility, not a new physical compatibility claim. Upstream still
enforces catalog expiry/signature, engine support, storage/layout identity,
candidate-bound approvals and reinspection inside the privileged boundary.
`apple,j614s` remains blocked. The shared desktop storage controls expose the
native session's alongside and detected-Omarchy replacement choices. Replacement
requires typing its displayed installation identifier and then reviewing and
approving the exact native plan. The complete detected installation is replaced;
arbitrary foreign partitions and repair operations are not selectable. The native
backend selects the suitable free-space or macOS-resize candidate for alongside
installation. These paths still need a macOS build and hardware qualification.

See [PROTOCOL.md](PROTOCOL.md) for the interface and
[PROVENANCE.md](PROVENANCE.md) for acquisition and limitations.
The desktop process/client and native credential flow are documented in
[TAURI-INTEGRATION.md](TAURI-INTEGRATION.md).

## Build and package on an Apple Silicon Mac

Prerequisites are Xcode's Swift 6.2 toolchain, macOS 15+, Python 3.11+, Git, and
the system `bsdtar`. Commands below create local source/build artifacts. The build
script does not run tests, sign, register a daemon, or execute the bridge/helper.

```sh
git submodule update --init -- providers/direct-apple/upstream
python3 providers/direct-apple/scripts/fetch-release.py
bash providers/direct-apple/scripts/build-macos.sh
python3 providers/direct-apple/scripts/assemble-bundle.py --team-id YOURTEAMID
```

`YOURTEAMID` must be the actual ten-character Apple signing Team ID selected for
the distribution. Supply `--parent-bundle-id` or `--parent-app-path` if the Tauri
identity differs from `community.omarchy.setup` or its installed path differs
from `/Applications/Omarchy Installer.app`.

The acquisition command downloads the separately pinned 19.4 MB PKG, verifies
its exact SHA-256 and size, and extracts only known members to `.release-input`.
It does not install the PKG or execute its scripts. `--package /path/to/file.pkg`
reuses an existing download after identical verification. The embedded engine,
catalog, signature and public trust root are independently hash/size checked.
Large OS payloads are downloaded by the retained coordinator during
`prepare_plan`, using the unchanged catalog's URLs and digest/size/part checks.

Assembly stages `dist/Omarchy Apple Bridge.app`,
`dist/com.omarchy.mx.installer.helper.plist` and `dist/provenance.json`. It embeds
the original catalog/signature/key unchanged and changes only the unsigned
`release.json` helper requirement to the selected distribution identity. The
companion admission resource pins the parent Tauri signing requirement. The
runtime verifies this parent before submitting credentials or helper execution.

## Distribution integration still required

The released upstream daemon trusts bundle ID `com.omarchy.mx.installer` signed
by Team `T2C384FJBD`. Our differently identified companion cannot use it. Copying
that installed helper or changing a caller-side string does not grant access.
This implementation instead builds the same unmodified helper source for the
distribution's own signing identity and stages reciprocal requirements:

- Parent: `community.omarchy.setup`, selected Apple Team ID.
- Companion: `community.omarchy.setup.apple-bridge`, same Team ID.
- Helper: `com.omarchy.mx.installer.helper`, same Team ID; its daemon environment
  accepts only the companion requirement.

The release assembly must sign the helper with that explicit helper identifier,
sign the companion with hardened runtime, embed the complete companion at the
Tauri app's `Contents/Resources/direct-apple/Omarchy Apple Bridge.app`, and sign the enclosing
Tauri application. Preserve the sealed release resources. Authenticate and
verify all reciprocal code-signing requirements before any qualification run.
Notarization/stapling and an installer package that installs the daemon plist as
root-owned, non-writable-by-users state remain distribution work. The helper
must be installed through that package; the bridge does not register it.
Because the upstream helper service identifier is retained, an existing upstream
daemon must be handled deliberately by that installer package; do not overwrite
or register a second service casually.

Launch the companion's actual executable directly from the Tauri process with
`--stdio`, piped stdin/stdout and a drained stderr. Do not use `open`, a shell
launcher, `sudo`, or the old GUI. In packaged Tauri resources the path is:

```text
direct-apple/Omarchy Apple Bridge.app/Contents/MacOS/omarchy-apple-bridge
```

No signed bundle, installed helper or physical run is produced by this source
change. Those are the remaining platform packaging/qualification gaps, not
missing public engine downloads. A plain development build can inspect/probe
once release resources are assembled; execution correctly reports that the
signed parent/companion or helper is unavailable until packaging is completed.

The `.7` source has no helper cancellation or ping API. Active operations cannot
be safely cancelled through this bridge. EOF keeps the native process alive
until its submitted coordinator call settles; it never kills the root engine
or reports a rollback. A crash can leave an uncertain outcome, so callers must
not retry automatically. The upstream trusted journal and Recovery procedure
govern recovery; the bridge exposes only the upstream approved retry path.

## Licensing

The upstream repository is MIT licensed; retain its `LICENSE` with distributions.
Its engine includes separate Asahi, m1n1, Python and other components with their
own licenses and notices. Preserve the archive's bundled notices and upstream
source-lock provenance. This adapter does not relicense those components.
