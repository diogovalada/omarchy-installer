# Local x86 image construction

The Windows product recipe includes a persistent OS menu, with Omarchy / five
seconds as installer defaults. Preferences are bound into the constructed image
and deployment plan. The menu reuses upstream kernel entries and survives their
regeneration through pre/post hooks; Windows starts through its existing firmware
entry after a brief restart. See [platform behavior](../../docs/platform-behavior.md).
The official ISO's Limine and update-tool packages are explicitly source-pinned.

This directory now contains two distinct entry points. `builder.py` and
`Invoke-ImageProof.ps1` remain the disposable feasibility proof described
below. `product_builder.py` is the credential-free construction recipe used by
the [Windows direct-install provider](../direct-x86/README.md). No successful
product construction or physical/independent boot is claimed. Pure boot-menu
regressions and packaged-runtime import now pass; full encrypted construction
awaits sufficient scratch resources. See the
[completion report](../../docs/evidence/windows-direct-readiness-2026-09-06.md).

## Product construction

The application first verifies and stages the pinned official ISO and detached
signature into an operation protected by the elevated helper. The provider
then starts the pinned Linux Docker runtime without networking, physical host
devices, additional capabilities, published ports or writable source mounts.
QEMU receives only a fresh 40 GiB encrypted QCOW2 file, a read-only ISO, fresh firmware
variables and an operation-specific CIDATA image. The product recipe supplies
no account password, authorized SSH key or other build credential.

The kernel/initramfs pair is extracted directly from the verified ISO. Its
live environment boots with a guest-only serial debug shell and the ordinary
automatic console installer masked. The fixed packaged guest script calls
`omarchy-iso-install` with upstream's encrypted deferred-provisioning configuration.
The pinned installer generates a fresh LUKS2 volume key and temporary passphrase
per construction. Its own first-boot setup collects the owner's password,
replaces the temporary passphrase slot and rebuilds boot files without the
auto-unlock key. Windows never collects the owner's Linux password.
`product-source-lock.json` pins the inspected installer, encryption and owner
sources from the actual released ISO. The recipe checks the installed Btrfs
root inside LUKS2, temporary key slot, key permissions, UKI contents, portable
UUID-based unlock entries, Limine binaries and armed upstream owner
setup, removes initramfs autodetection so storage/input modules are portable,
locks root, removes SSH host keys and resets the machine ID. Every build keeps
its freshly generated filesystem/partition UUIDs; those exact partition GUIDs
are later used in the physical deployment plan.

The official ISO's offline package repository is retained in the image. On the
actual machine, a dedicated service runs upstream
`omarchy-apply-hardware --defer-provisioning` before the owner-setup service,
using that local repository. It verifies the LUKS/Btrfs/partition identities,
grows the opened LUKS mapping and then Btrfs within the already allocated
partition, rebuilds initramfs/Limine, restores normal package repositories and
records completion. The upstream first-boot service then asks for the owner.
The factory snapshot is refreshed after portability and identity preparation,
using the pinned upstream scrub routine to remove the temporary key and its
boot drop-ins from the factory image. First boot refreshes it again after real
hardware setup, before owner creation. This baseline retains the machine's
drivers, clears the machine identity, and excludes the direct-install pending
marker so a later factory reset uses upstream's own new-owner flow.

After clean guest shutdown, a read-only `qemu-nbd` process exposes the encrypted
QCOW2 through a private userspace Unix socket. The fixed client implements only
READ of the ESP and encrypted root ranges. One MiB chunks stream directly into
authenticated encrypted `.img.enc` artifacts; neither kernel NBD devices nor
plaintext partition files are created. The schema 2 manifest records each
partition's clear length and SHA-256, envelope length/hash/HMAC, signed ISO identity,
runtime/firmware/package-inventory hashes, every executable recipe's digest,
operation-specific filesystem, LUKS and GPT identifiers, explicit LUKS2 policy,
the `owner-setup-required` protection state, and qualification
flags. It is not a signed reusable image or an authorization token. The helper
must preserve the protected build-to-deploy provenance chain.

The encrypted artifact key arrives over private stdin from the elevated helper
and is never written to persistent storage. A purpose-separated QCOW2 secret
lives only in the container's required private `/tmp` tmpfs. Core dumps are
disabled. Raw guest serial text remains in a bounded memory tail; retained logs
contain fixed status messages only. The helper retains the key only for this
active build/deployment; losing that process requires a fresh construction.
Deleting files is not used as a claim of secure erasure. This matters because
upstream replaces passphrase slots, not the LUKS volume key: an old bootstrap
key plus an old LUKS header could otherwise decrypt later user data. On the
deployed machine, stock temporary auto-unlock remains until owner setup
completes. See [the inspected encryption contract](product-source-contract.md).

The base images need 40 GiB minus 2 MiB contiguous target space. The physical
root partition can be larger according to the reviewed allocation; the first
boot expands the mapping and filesystem without building a larger VM image.
Worst-case staging requires 85 GiB free after ISO download/staging (40 GiB
QCOW2, approximately 40 GiB partition exports and reserve). The 6 GiB software
emulated guest requires 10 GiB currently free memory. The two-hour build bound
is an implementation limit, not a measured construction-time promise. The
release package contains the pinned runtime archive. A separate preparation
action verifies and imports it into an already running Linux Docker engine;
the provider does not install Docker or enable Windows host features.

Construction supports the provider's fixed `cancel.requested` marker. Neither
this recipe nor its deployment path has been run to completion. The unchanged
proof results below remain the only measured VM evidence. Missing runtime,
space, release compatibility or guest completion produces an error and cannot
produce a successful deployment receipt.

## Existing disposable feasibility proof

This is a bounded local experiment behind the direct-install feasibility gate.
It is **not connected to a physical disk writer or the product UI**. No image it
produces is approved for deployment. It consumes the signed official Omarchy
4.0.2 ISO and adapts the upstream
[integration base-image recipe](https://github.com/omacom/omarchy-iso/blob/2673c613d9a71e23920e43fbb951238145e0f1e8/test/integration.d/base-test.sh).
See `NOTICE` for the source license.

The release pin is distinct from the recipe source pin. `inspect` reads the
actual released ISO before `install` may use its unattended interface. A newer
branch's documentation is not evidence that 4.0.2 implements the same contract.

## Local commands

Requirements: Windows PowerShell 7, an already running Linux Docker engine,
the locally built runtime in `runtime-lock.json`, and the exact ISO plus its
detached `.iso.sig` in `.cache/`. The parent application's release client owns
normal downloading. This proof does not install Docker, WSL, host packages, or
hypervisor features.

From the repository root:

```powershell
./providers/image-builder-x86/Invoke-ImageProof.ps1 -Action preflight
./providers/image-builder-x86/Invoke-ImageProof.ps1 -Action smoke
./providers/image-builder-x86/Invoke-ImageProof.ps1 -Action inspect
./providers/image-builder-x86/Invoke-ImageProof.ps1 -Action install
```

`smoke` boots UEFI on a 64 MiB empty virtual disk, waits up to 60 seconds for the
UEFI shell to be visible, and records QMP block-device inventory and a
screenshot. It proves runtime execution only.
`inspect` verifies the ISO length, SHA-256, detached signature, and pinned key,
then reads installer files directly from the ISO's SquashFS without mounting
or extracting a filesystem copy. ISO extents must be contiguous and within the
verified file before the reader accepts their byte offset.
`install` performs the same checks and then attempts the official unattended
installation into a fresh 40 GiB sparse QCOW2 file. Successful installation must
answer over guest SSH, shut down cleanly, and boot again with **fresh firmware
variables and neither ISO nor cidata attached**. The second boot must report an
installed Btrfs root on `/dev/vda2`, machine ID, and installed Omarchy package.
Only then can the staging disk be renamed and hashed as a successful VM proof.
Initial SSH discovery discards failed live-ISO host keys and pins a key only
after the run's unique client key authenticates an installed guest. The
independent boot must retain that key. Transient SSH timeouts are retried within
the outer wait bound; a changed installed-guest key is rejected.

The default guest has 6 GiB RAM and four virtual CPUs under software emulation
(TCG). Install preflight requires 10 GiB currently free host RAM and 50 GiB free
output space **after downloading the 5.80 GiB ISO**. This covers the virtual
disk's maximum size and reserve. Runtime
dependencies consume additional Docker storage. Inspection uses a 1 GiB memory
limit. Installation uses a 9 GiB container memory limit, no swap, four CPU quota,
128 process limit, a 30 minute install timeout, and up to 10 minutes for the
independent boot. `-MemoryMiB 4096` or `8192` changes both guest memory and the
preflight requirement; `-TimeoutSeconds` is bounded to 60–2400 seconds. Those
settings are experimental, not qualified minimum requirements. The 4 GiB
experimental guest requires 8 GiB free host memory and has a 7 GiB container
cap. An actual first run exhausted the earlier 5 GiB cap after the official
installer started; the higher overhead allowance responds to that measured
failure. Receipts record cgroup memory limits, peaks, and OOM counters.

The second measured run stayed below the 7 GiB cap but reached the 30 minute
install timeout while the official installer was still running. It retained a
2,542,927,872-byte staging file and produced no installed-system or independent-
boot proof. See `docs/evidence/image-builder/README.md` and the final receipt.
The later SSH/root-validation hardening passed twelve focused tests; a full
install with those changes remains unqualified.

The container runs as UID 1000, with all capabilities dropped, no new
privileges, read-only root/source/cache mounts, a single operation output
directory, no network, and no mapped host devices. QEMU uses restricted
user-mode networking solely for SSH on loopback **inside the container**. No
ports are published. The official installer sees `/dev/vda` backed solely by
the new file. The runner accepts no target device, extra QEMU arguments, shell
commands, forwarding rules, or custom mounts.

All runs get a fresh directory under `.runs/`. Receipts include source digest
and signer, recipe commit, exact runtime image identifier, firmware digests,
package inventory digest, builder/configuration hashes, stage outcomes, and
timings. Failure or interruption cannot produce a success receipt or promote a
staging image. QEMU is terminated and only this operation's named container is
stopped on exit. Failed files are retained for diagnosis, never treated as a
reusable base. `.cache/`, `.runs/`, and downloaded reference source are ignored.

The normal VM proof creates unique random test credentials and an SSH key per
run. It uses no personal information and no encryption. Those credentials
remain in the local disposable test image/configuration; do not deploy or
reuse it. `configuration.py` keeps those proof defaults unchanged; the product
explicitly selects `deferred=True, encrypted=True`. Product encrypted first-boot
behavior has not been qualified by this runner. Neither profile is a
distributed/shared template.

## Runtime rebuild and verification

`Dockerfile` pins the Ubuntu base by digest. `runtime-packages.lock` pins all
125 added/changed packages, including transitive dependencies. APT
checks signed repository metadata and package hashes before package execution.
The successful build's full package list is recorded at
`docs/evidence/image-builder/runtime-packages.tsv`. A later
rebuild needs a new image lock and qualification; it is not silently accepted
as identical because its Dockerfile is unchanged.

```powershell
./providers/image-builder-x86/Build-Runtime.ps1
```

The checked-in runtime lock describes the actual local qualification image.
The build script records the resulting full `sha256:` image identifier,
Dockerfile/package-lock SHA-256, base pin, timestamp, model provenance, and
complete package inventory before the proof. The launcher invokes that
immutable identifier and rejects changed build inputs. No dependency or image
is published here. Package versions becoming unavailable fail the build; the
script never silently substitutes a newer package.

Tests need only Python 3.11+ on Linux, including the existing WSL environment:

```powershell
wsl -d Ubuntu -- sh -lc 'cd /path/to/omarchy/providers/image-builder-x86; python3 -m unittest -v'
```

## Gates that remain outside the disposable proof

- Actual release compatibility and completed installation/independent boot
  must be established from receipts; a source inspection or firmware smoke
  cannot substitute for either.
- The separate product recipe now implements deferred owner setup, upstream
  real-hardware discovery with the ISO's offline repository, portable initramfs
  construction, exported partition identifiers and first-boot completion. None
  of those paths has completed qualification. Secure Boot is unsupported.
- Product construction now implements fresh upstream LUKS2 bootstrap, encrypted
  temporary artifacts, first-boot mapping growth and stock owner slot replacement.
  Qualification of encrypted construction, handoff, first-boot growth and owner
  cleanup remains deferred. No reusable encrypted template is distributed.
- The separate Windows provider implements exact alongside allocation, bounded
  verified writes and permanent boot registration. Filesystem growth and factory
  state are handled by the product recipe. Rollback remains manual and requires
  inspection; physical deployment and interruption recovery remain unqualified.
- A production helper must authenticate operation-owned output and provenance;
  these local JSON receipts are evidence, not an authorization token. Production
  cache/source immutability and secure secret storage/erasure are unqualified.
- Windows, Linux, and macOS runtime packaging, support requirements, licensing,
  acceleration availability, performance, and interrupted-operation recovery
  still require qualification. No physical-disk operation is exercised here.
