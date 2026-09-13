# Third-party notices

Project code is MIT-licensed except where a retained notice states otherwise.
Dependencies and downloaded operating-system/runtime packages retain their own
licenses. Inclusion here does not imply upstream endorsement.

| Component | Integration and exact source | License / attribution |
| --- | --- | --- |
| GNU GRUB USB UEFI loader | Assembled with grub-mkimage from Ubuntu 2.12-1ubuntu7.3; exact source archives, Ubuntu patches, build recipe and checksums are bundled | GPL-3.0-or-later; [complete notice and source information](providers/usb-preserve/boot/NOTICE.txt) |
| Omarchy ISO unattended layout | Adapted in `providers/image-builder-x86/configuration.py` from commit `2673c613d9a71e23920e43fbb951238145e0f1e8`, `test/integration.d/base-test.sh` | MIT, Copyright (c) 2026 Anton Hvornum; full notice in [provider NOTICE](providers/image-builder-x86/NOTICE) |
| Omarchy signing public key | Embedded in release-client and the image proof from the same pinned ISO repository commit, `builder/omarchy.gpg` | Upstream Omarchy/Omacom public key; source, file hash and fingerprint in [release-client README](crates/release-client/README.md) |
| Etcher SDK | Linked npm dependency, exact `10.2.14`; source is not copied or patched | Apache-2.0; dependency tree and applicable notices retained by [provider lockfile](providers/media-etcher/package-lock.json) and installed packages |
| rPGP | Linked Rust dependency `pgp 0.20.0` | MIT OR Apache-2.0; exact transitive versions in Cargo lockfiles |
| QEMU, Ubuntu, EDK II/OVMF and runtime packages | Pinned archive included in the local Windows preview; immutable image and package pins in `providers/image-builder-x86/` | Individual upstream licenses and package notices retained inside the archive; complete package inventory in [runtime evidence](docs/evidence/image-builder/runtime-packages.tsv); public distribution/source-availability review pending |
| Omarchy Limine reset template | Actual 4.0.2 ISO template retained as `testkit/windows-direct/fixtures/limine-reset.conf` | MIT, Copyright David Heinemeier Hansson; full [upstream notice](apps/desktop/src/assets/omarchy/LICENSE-Omarchy.txt) |
| Omarchy MX Mac native installer | Retained submodule at `00daf3ebef9e4fbe89fb9b32bd184e65aa75d357`, with a separate Swift bridge | Upstream MIT notice retained in [submodule LICENSE](providers/direct-apple/upstream/LICENSE); source/release attribution in [provenance](providers/direct-apple/PROVENANCE.md) |

Other Rust and JavaScript dependencies are identified by the root and desktop
Cargo lockfiles, the portable cache verifier's `scripts/portable-cache-helper/Cargo.lock`,
`pnpm-lock.yaml` and the Etcher adapter's npm lockfile. Before
shipping an application or provider bundle, generate its full dependency/license
inventory and include all required notices. The Etcher dependency audit and
unresolved packaging gates are documented in its [README](providers/media-etcher/README.md).

The Mac bridge retains upstream source and license notices. Its separately pinned
Asahi engine, embedded Python runtime and downloaded payload retain their original
licenses; they are not relicensed as part of this application's MIT code.
# Desktop visual identity

The desktop app includes Omarchy's original wordmark, icon, Tokyo Night desktop
preview and two wallpapers from the exact upstream commit recorded in the asset
lock, together with pinned JetBrains Mono font files matching those served by
the Omarchy website. Source URLs, hashes and license texts
are retained in [the asset directory](apps/desktop/src/assets/omarchy/README.md).
Omarchy's upstream copyright is David Heinemeier Hansson; JetBrains Mono is
distributed under the SIL Open Font License. The app remains a community project.

## webpki-roots 1.0.9

The application includes certificate trust-anchor data from webpki-roots.
Source: https://github.com/rustls/webpki-roots
License: CDLA-Permissive-2.0. The package's license text follows.

Other dependency notices accompany the bundled provider runtimes.

# Community Data License Agreement - Permissive - Version 2.0

This is the Community Data License Agreement - Permissive, Version
2.0 (the "agreement"). Data Provider(s) and Data Recipient(s) agree
as follows:

## 1. Provision of the Data

1.1. A Data Recipient may use, modify, and share the Data made
available by Data Provider(s) under this agreement if that Data
Recipient follows the terms of this agreement.

1.2. This agreement does not impose any restriction on a Data
Recipient's use, modification, or sharing of any portions of the
Data that are in the public domain or that may be used, modified,
or shared under any other legal exception or limitation.

## 2. Conditions for Sharing Data

2.1. A Data Recipient may share Data, with or without modifications, so
long as the Data Recipient makes available the text of this agreement
with the shared Data.

## 3. No Restrictions on Results

3.1. This agreement does not impose any restriction or obligations
with respect to the use, modification, or sharing of Results.

## 4. No Warranty; Limitation of Liability

4.1. All Data Recipients receive the Data subject to the following
terms:

THE DATA IS PROVIDED ON AN "AS IS" BASIS, WITHOUT REPRESENTATIONS,
WARRANTIES OR CONDITIONS OF ANY KIND, EITHER EXPRESS OR IMPLIED
INCLUDING, WITHOUT LIMITATION, ANY WARRANTIES OR CONDITIONS OF TITLE,
NON-INFRINGEMENT, MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.

NO DATA PROVIDER SHALL HAVE ANY LIABILITY FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING
WITHOUT LIMITATION LOST PROFITS), HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE DATA OR RESULTS,
EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGES.

## 5. Definitions

5.1. "Data" means the material received by a Data Recipient under
this agreement.

5.2. "Data Provider" means any person who is the source of Data
provided under this agreement and in reliance on a Data Recipient's
agreement to its terms.

5.3. "Data Recipient" means any person who receives Data directly
or indirectly from a Data Provider and agrees to the terms of this
agreement.

5.4. "Results" means any outcome obtained by computational analysis
of Data, including for example machine learning models and models'
insights.
