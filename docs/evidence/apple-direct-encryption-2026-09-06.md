# Apple direct-install encryption inspection

Date: 2026-09-06. Read-only source and public artifact inspection; no installer,
guest, privileged operation or hardware test was executed.

## Finding

The selected Apple Silicon direct-install path deploys an unencrypted Btrfs root
image. Its first-boot owner setup does not convert that filesystem to LUKS and
does not generate a per-install disk-encryption key. The password-slot replacement
code applies only to installations that are already encrypted. This finding is
specific to our pinned direct-install package, not all Mac installers or the
separate ARM64 ISO installation path.

## Evidence chain

1. Our `providers/direct-apple/release-lock.json` pins release
   `v4.0.1-mac.2.9.090126`, engine `.7`, and the September 1 OS package. The local
   engine archive was rehashed and matches
   `063fd0765fb2057384d9653f7bf547b0471af31fc764e039d578d4fef6dce4d5`.
   Its `osinstall.py` copies each image directly to its raw destination
   partition; `omarchy_asahi.py` verifies the written image bytes. These paths
   do not create an encrypted container.
2. The public package's ZIP directory identifies `root.img` as a 34,359,738,368-byte
   image. Bounded HTTP range reads of the selected release's split ZIP downloaded
   82,815 bytes total and decompressed only the root-image prefix. The image has
   Btrfs magic `_BHRfS_M` at offset 65,600 and no LUKS magic at offset zero.
   This confirms that the published root image exposes plain Btrfs. This partial
   read does not verify the full ZIP SHA-256 or establish reproducible builds.
3. The public September 1 builder source directly runs `mkfs.btrfs` on `root.img`.
   Its image-runtime configuration uses the mounted filesystem without a
   `disk_encryption` block or `luks_uuid`. The latest inspected build source,
   `db164502de4b14edd2531af7f9ed63908afe4fc2`, retains that behavior. The historical
   source supports the artifact observation; it is not claimed as the proven
   exact build commit of the released ZIP.
4. Runtime tag `v4.0.1-mac.2`, `bin/omarchy-provision-owner`, lines 852–870 and
   1010–1013: `rekey_luks` returns without a staged `luks-key`; its caller is
   likewise conditional on that file. Existing encrypted installations use
   `luksAddKey` and `luksKillSlot`. There is no first-boot `luksFormat` or
   `cryptsetup reencrypt` operation in that flow. The builder stages the key
   only when encryption was configured, which the direct image build omits.

Primary sources:

- [Pinned release and assets](https://github.com/maralcbr/omarchy-mx-mac/releases/tag/v4.0.1-mac.2.9.090126)
- [September 1 base-image creation](https://github.com/maralcbr/omarchy-iso/blob/73118b450150e035e884ea915c39ad38436449cb/builder/asahi-stages/base-images.sh#L23)
- [Image-runtime configuration](https://github.com/maralcbr/omarchy-iso/blob/73118b450150e035e884ea915c39ad38436449cb/builder/asahi-stages/image-runtime.sh#L67)
- [Conditional provisioning-key staging](https://github.com/maralcbr/omarchy-iso/blob/73118b450150e035e884ea915c39ad38436449cb/configs/airootfs/usr/share/omarchy-iso/orchestrator/phases_impl.py#L1220)
- [Released first-boot owner setup](https://github.com/maralcbr/omarchy-mx-mac/blob/v4.0.1-mac.2/bin/omarchy-provision-owner#L852)

## Product implication

The Mac path avoids local OS construction by distributing a prepared image, but
it does not supply our Windows path's per-install LUKS2 behavior. Do not claim
that entering the first-boot account password encrypts this Mac installation, or
that macOS FileVault establishes LUKS protection for the separate Linux root.
Source-supported behavior remains distinct from hardware qualification.

The local inspection script, source snapshots and prefix report are retained
under `artifacts/apple-encryption-inspection/`. No downloaded code was executed.
