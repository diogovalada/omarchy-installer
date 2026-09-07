# Inspected Omarchy 4.0.2 encryption contract

Source inspection on 2026-09-06 used the actual local official
`omarchy-4.0.2.iso`, at the SquashFS offset recorded in the existing signed-source
inspection evidence, and its bundled `omarchy-4.0.2-1-any.pkg.tar.zst`. No guest,
installation, disk operation, build or behavior test was run. The exact file
digests and resolved agent model `gpt-6-astra` are in
[product-source-lock.json](product-source-lock.json). Product construction still
verifies the complete ISO SHA-256 and detached signature before reading these
files; the source contract pins are an additional compatibility check.

The released `/root/configurator` explicitly defaults both free-space and
full-disk installation to `mode="encrypted"`; the unencrypted choice is behind
Ctrl+C. The product had overridden that default with an absent LUKS block and
`user_encrypt_installation=false`. It now follows the encrypted default.

The release supports the intended encrypted owner setup directly:

1. `orchestrator/context.py` sees the root `disk_config.disk_encryption` block
   and deferred provisioning, creates `secrets.token_urlsafe(24)`, and supplies
   it to archinstall. It strips account credentials. Setting only the separate
   encrypt flag does not create this disk configuration.
2. The bundled archinstall `lib/disk/luks.py` explicitly uses `luksFormat
   --type luks2 --pbkdf argon2id`. A fresh virtual disk is formatted for every
   operation; no reusable installation image or preselected volume key exists.
3. `orchestrator/phases_impl.py` stages the temporary passphrase in root-owned
   mode-0600 files `var/lib/omarchy/provisioning/luks-key` and
   `etc/omarchy/provisioning.key`, adds the key to the initramfs and writes
   `cryptkey=rootfs:/etc/omarchy/provisioning.key`. The pending owner service
   and offline Node package are also staged.
4. The runtime's `omarchy-provision-owner` collects the owner's password. Its
   `rekey_luks` adds that passphrase, rebuilds the UKI without auto-unlock,
   identifies the new slot, removes other slots, and removes the staged key.
   Failures retain retry state. This replaces passphrase slots; it does **not**
   rotate the underlying volume key.
5. Upstream `_scrub_factory_snapshot` removes the temporary keys and boot
   drop-ins from factory state. The product calls that same pinned function
   when refreshing the portable factory snapshot and carries its scrub path
   data for the installed OS. First boot refreshes the snapshot again after
   hardware setup and before owner creation, using those same scrub paths.
   Both baselines omit the direct-install pending marker; the final baseline
   also clears machine identity and SSH host keys. Its staged/backup names
   allow retry after an interruption. Upstream factory reset creates its own
   new temporary LUKS passphrase and owner pending state before rebuilding boot.

The product validator additionally requires the expected installed owner binary
and service hashes, a LUKS2 root mapping backed by the guest's second partition,
a single initial LUKS slot, a dynamically sized data segment, matching temporary
key copies and permissions, working bootstrap unlock, a pending owner service,
offline Node files, stable UUID/PARTUUID boot references, an encrypted portable
initramfs and embedded bootstrap key. It emits no key bytes or passphrase hashes.
The root export is the outer `crypto_LUKS` partition; its Btrfs UUID belongs to
the opened inner filesystem. The physical writer preserves the partition GUID
and can allocate a larger partition than the source image. First boot verifies
all three identities, enlarges the LUKS mapping using the existing temporary
key, and then grows Btrfs to that mapping's size.

The schema 2 identity reports `encryption: luks2`, `bootstrapKey:
upstream-per-install`, `ownerRekey: passphrase-slots-only`, `protectionState:
owner-setup-required`, and `firstBootGrowth: luks-mapping-and-btrfs`. LUKS2
formatting is not a claim that the provisioning window is confidential: the
stock first-boot UKI initially contains the temporary unlock key on the ESP.

Persisted construction copies require separate protection because retaining
that bootstrap key together with the old LUKS header defeats later slot removal.
The product encrypts the complete VM disk using native QCOW2 LUKS and wraps both
partition exports in authenticated encryption using a random per-operation key
held in the helper's memory. Only the derived QEMU secret file reaches a private
container tmpfs; no plaintext exported ESP/root or durable staging key is written.
The original official ISO and signature contain no per-install keys and can be
retained. Operation restart without the key must construct a fresh image.

The userspace reader follows the
[NBD fixed-newstyle protocol](https://github.com/NetworkBlockDevice/nbd/blob/master/doc/proto.md)
with simple READ replies and no kernel attachment. QEMU's
[NBD server](https://www.qemu.org/docs/master/tools/qemu-nbd.html) supports the
private read-only export, and its
[image documentation](https://www.qemu.org/docs/master/system/images.html)
describes native QCOW2 LUKS. These references explain the implementation;
runtime behavior against the pinned QEMU version remains unqualified.
