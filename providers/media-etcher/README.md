# Etcher SDK media providers

The bounded physical USB implementation added on 2026-09-06 is documented in
[PHYSICAL-PROVIDER.md](PHYSICAL-PROVIDER.md). Its CLI is
`dist/physical-cli.js`, and its separate TypeScript API is
`dist/physical-adapter.js`. Physical-device tests and qualification are deferred.

The remainder of this document records the earlier **file-only** feasibility
adapter and its 2026-09-05 evidence. Its API and behavior remain unchanged.

## Original file-only feasibility record

This isolated package demonstrates a headless Etcher SDK engine behind a typed
Node API. It is an implementation spike for the shared Windows/macOS/Linux media
provider; it is not connected to the Tauri frontend and cannot write physical
media. The normal npm dependency is pinned to **etcher-sdk 10.2.14**, with its
resolved dependency tree in `package-lock.json`. No SDK source is copied or patched.

## Run

Use Node 22 or 24, then run these commands from this directory:

```powershell
npm ci --no-audit --no-fund
npm test
npm run probe
npm run qualify:runtime
```

This package deliberately has its own npm lockfile and is outside the current
root pnpm workspace. Native dependency lifecycle scripts must run during install.
`qualify:runtime` stages a separate Node executable, compiled adapter and complete
dependency directory under ignored `.qualification-runtime/`, then runs an actual
source-to-file test from that directory with `NODE_PATH` and `NODE_OPTIONS` empty.
The operation leaves its uniquely named stage directory for inspection. It is a
relocatable directory experiment, not a signed installer or single executable.

## Public API

```ts
import { writeVerifiedTestFile } from '@omarchy-setup/media-etcher-feasibility';

const receipt = await writeVerifiedTestFile({
  sourcePath: 'C:\\fixtures\\verified-source.img',
  destinationPath: 'C:\\fixtures\\new-output.img',
  expectedSha256: artifact.sha256,
}, {
  signal: abortController.signal,
  onEvent: event => console.log(event),
});
```

The package is private and not published; a future integration must first wire
it into the repository's build and package the Node/native runtime. Its public
entry is `dist/adapter.js`, with generated TypeScript declarations.

Events cover validation, writing, SDK verification, full SHA-256 readback, and
exactly one terminal completion/error/cancellation event. A success receipt
requires worker exit with code zero, mandatory SDK verification, destination
flush, exact-length SHA-256 readback, and a final source digest check. Raw bytes
are used directly; the adapter never decompresses, trims, configures an image,
downloads, discovers drives, unmounts, ejects, or elevates. The expected digest
is required but its authenticity is the caller's responsibility.

The SDK `File` class is adapted to already-opened handles. Its path-opening
method is bypassed; neither SDK writing nor readback reopens the caller's path.
The adapter owns and closes both handles. SDK verification remains hardcoded
to `true`, with no public switch to disable it. Observer callback exceptions
are ignored so UI code cannot throw inside a worker stream callback.

## File-only boundary and cancellation

- Both paths must be absolute ordinary local paths. Reject Windows device/NT
  namespaces, UNC paths, reserved device names, alternate data streams, URLs,
  POSIX `/dev`, `/proc`, `/sys`, traversal, and ambiguous trailing dots/spaces.
- Inspect every existing path component with `lstat`; reject symbolic links
  and directory junctions. Check opened source/destination handles are regular
  files. Reject empty sources and unsafe byte lengths.
- Reject identical source/output paths and **every existing output**, including
  hard links, so existing data is never intentionally overwritten. Create the
  output with exclusive creation (`O_EXCL`) and use `O_NOFOLLOW` where available.
- Run the SDK in a hidden Node child process. Abort terminates that process and
  reports cancellation only after its `close` event, releasing file handles.
  The SDK currently has no AbortSignal parameter on its multi-write API.
- A cancelled or failed operation may leave an incomplete newly-created file.
  It is never described as verified and is not automatically deleted. Retrying
  requires a new output path or explicit handling of that known test artifact.

These checks limit this ordinary-process test harness. They are not a privileged
authorization protocol and do not solve hostile concurrent ancestor-directory
replacement, device identity, system/source-disk exclusion, exclusive media
locking, physical sector bounds, ejection, or power-loss durability. The parent
directory must be controlled by the caller. File readback may be served by the
OS cache. No physical-media qualification can be inferred from these tests.

## Evidence: 2026-09-05

Resolved implementation model: **gpt-6-astra**. Host: **Windows x64**, Node
**22.13.1**, npm **11.12.0**, TypeScript **5.9.3**. Only generated regular files
were written. No physical disk operation, elevation, visible helper window,
publication, or upstream message occurred.

| Check | Observed result |
| --- | --- |
| Ordinary pinned npm install | Passed; 222 packages installed, native scripts enabled |
| TypeScript build | Passed |
| `npm test` | 13 passed, 0 failed, 0 skipped, including review regressions |
| Raw image exact-byte write | 2 MiB + 73 bytes; SDK verification events and independent readback passed |
| Source digest mismatch | Rejected before output creation |
| Existing output, hardlink and direct alias | Rejected; original bytes preserved |
| Device paths and junctions | Rejected without opening physical media |
| Windows rooted paths without a drive | Both slash styles rejected |
| Unserializable extra request metadata | Omitted from IPC; verified operation completed and worker closed |
| Synchronous IPC send failure | Worker terminated before the error event/rejection; no output created |
| Pre-start and in-flight cancellation | Passed; output handle could be renamed after cancellation |
| Missing paths and zero-length source | Errors reported without success |
| Deliberate output corruption during readback | SHA-256 mismatch; no completion receipt |
| SDK and native module loading probe | All selected modules loaded |
| Relocated runtime smoke test | Passed, 1,048,699 exact bytes; receipt SHA-256 `e53d003c9f84e8b877a57bac89461bd4b144ccf7be473b09687c23ecf3dfe6a5` |
| macOS/Linux runtime, signing and packaged Tauri invocation | Not tested |

The probe loads the public SDK entry, multi-write entry, `drivelist` 12.0.2,
`mountutils` 2.0.8, `@ronomon/direct-io` 3.0.1, `lzma-native` 8.0.6,
`xxhash-addon` 2.1.0, and `@balena/node-crc-utils` 3.1.0. Loading these packages
does not call their device discovery/writing methods. Native `.node` binaries
were present and resolved on this host. `npm run probe` prints the concrete
native-file inventory and runtime provenance for a new run.

The installed tree includes optional USB tooling and unused network/compression
features. Production packaging needs dependency/license inventory and per-host
native rebuild/signing qualification. Preserve each component's own license
notices; Etcher SDK is Apache-2.0, which does not relicense its dependencies.

`npm audit --omit=dev --json` returned four **moderate affected-package entries**
and no high/critical entries. The underlying advisories concern
[file-type's ASF parser](https://github.com/advisories/GHSA-5v7r-6r5c-r473) and
[follow-redirects authentication headers](https://github.com/advisories/GHSA-r4q5-vmmm-2653).
Network requests and automatic type detection are outside this adapter API;
the dependency findings remain an unresolved production qualification item.
No forced downgrade or speculative override was applied. npm also warned that
`file-disk`, `blockmap`, `partitioninfo`, and `prebuild-install` are deprecated,
and about the resolved `glob` version. The SDK itself being maintained does not
remove these transitive maintenance concerns. Its `unbzip2-stream` dependency
is pinned to a Git commit; npm warned that tarball integrity checking is skipped
for that Git dependency. Clean installs therefore also require Git access.

## Upstream basis and remaining gate

- [SDK v10.2.14 package](https://github.com/balena-io-modules/etcher-sdk/blob/v10.2.14/package.json):
  selected version, dependencies, Apache-2.0 license, upstream Node range `>18 <25`.
- [SDK sample writer](https://github.com/balena-io-modules/etcher-sdk/blob/v10.2.14/examples/multi-destination.ts):
  file and device sources are distinct; sample verification defaults to false.
  This adapter uses only file handles and always enables verification.
- [SDK multi-write](https://github.com/balena-io-modules/etcher-sdk/blob/v10.2.14/lib/multi-write.ts)
  and [File implementation](https://github.com/balena-io-modules/etcher-sdk/blob/v10.2.14/lib/source-destination/file.ts):
  pinned internal entry points, progress/failure reporting and file-handle behavior.
  These deep imports must be requalified when upgrading the dependency.
- [Etcher helper packaging](https://github.com/balena-io/etcher/blob/master/forge.sidecar.ts):
  upstream packages a separate Node helper and rebuilds native dependencies.
  This is packaging precedent; its control protocol is not adopted here.

Before enabling physical USB creation, implement and test the app-owned verified
artifact/approved-plan protocol, target reidentification, privileged per-host
device policy, locking/unmount, bounded writes, flush/readback, cancellation,
ejection and receipts. Then qualify a signed Tauri-distributed runtime on every
supported architecture. The SDK supplies an engine; it does not supply our
device identity or authorization policy.
