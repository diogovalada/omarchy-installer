# Local image construction evidence

Date: 2026-09-05. Model: `gpt-6-astra`.

Scope: a local Windows/Docker Linux/QEMU experiment with file-backed virtual
disks. No physical devices, elevation, host package installation, hypervisor
changes, partition/firmware changes, publication, or upstream communication.
The runner lives in `providers/image-builder-x86/`.

## Verified inputs and runtime

- Official [Omarchy 4.0.2 ISO](https://iso.omarchy.org/omarchy-4.0.2.iso):
  6,227,752,960 bytes; SHA-256
  `2ef8e624aa1bec7e277e28056b8535a6c9373ba48d7ede3f1a01cb6d2373cfb8`.
- Detached EdDSA signature validated against primary fingerprint
  `40DFB630FF42BCFFB047046CF0134EE680CAC571`; see
  `iso-signature-verification.txt`. The key was fetched from the pinned
  [upstream source commit](https://github.com/omacom/omarchy-iso/blob/2673c613d9a71e23920e43fbb951238145e0f1e8/builder/omarchy.gpg).
- Final runtime image:
  `sha256:020be814fec7b78e7f9baa3a49a53227cd10a98d210d621f9d4fd4f4457a3c8b`.
  Ubuntu base is pinned by digest; all 125 added/changed packages, including
  transitive dependencies, have exact version pins. APT verified signed
  repository metadata and package hashes. `runtime-packages.tsv` records the
  complete installed inventory. QEMU is 8.2.2, Ubuntu package
  `1:8.2.2+ds-0ubuntu1.18`; OVMF is `2024.02-2ubuntu0.9`.

## Achieved checks

`tests.txt` records twelve passing focused tests: fixed virtual-disk target and
partition bounds; unique run identities; credential-free deferred profile;
read-only install media; independent-boot media omission; linked/escaped input
rejection; checksum rejection before signature execution; bounded contiguous
ISO extents; exact installed-root/identity/package evidence validation; live-to-
installed SSH trust transition; rejection without the installed-system marker;
and retry after a transient SSH timeout. The last three tests and stricter root
validation were added after the bounded installation below.

`firmware-smoke.json` and `firmware-screen.png` record an actual software-emulated
UEFI boot. OCR saw the UEFI shell, QMP reported the running VM, and the recorded
block inventory contains only the firmware files and operation-owned 64 MiB
QCOW2 file. The runtime used a non-root, read-only, network-none container with
all capabilities dropped and no host device mappings.

The signed released ISO was inspected directly. Its unattended `cidata` loader
and installer context/phases are present, including deferred-provisioning
handling. This is evidence about **4.0.2's actual contents**, separate from the
current upstream branch. Presence of those files does not establish successful
installation or first-boot behavior.

`install-preflight.json` records an initial temporary resource rejection:
4,067 MiB host RAM free versus 8,192 MiB for the default 6 GiB guest plus
overhead. After other builds finished, a 4 GiB experimental guest passed the
unchanged 6 GiB host-free gate with 7,222 MiB free. The actual installation
attempt began at 22:09:21 UTC in local run
`20260905T220921Z-39afbdfd`. It reached the official "Installing Omarchy" screen
automatically, then QEMU exceeded the 5 GiB container memory cap. Docker's OOM
event is retained in `install-container-oom.jsonl`; `install-first-attempt.json`
and `installer-started.png` record the failed run. Its staging disk was not
promoted, and installation/independent-boot proof remained false.

That measured failure led to a 7 GiB container cap for the same 4 GiB guest and
a stricter 8 GiB host-free requirement, preserving 1 GiB outside the container.
At 22:20:43 UTC a fresh retry passed with 10,070 MiB host RAM free, under run
`20260905T222043Z-f8574f3b`. The retry **failed at the configured 1,800-second
installation timeout**, finishing at 22:51:13 UTC. `install-second-attempt.json`
records the final outcome. `install-final-screen.png` and its OCR text still
show "Installing Omarchy". The retained staging file is 2,542,927,872 bytes;
there is no promoted `image.qcow2`. Peak container memory was 5,744,263,168
bytes (5.35 GiB), below the 7 GiB cap; all memory limit/OOM counters were zero.
The VM was terminated and no proof container remained. An installed system and
independent boot both remain **unproven**. The evidence does not establish
which installer stage needed more time or whether installation would finish.

During this run, review found that the initial SSH probe could pin the live
ISO's host key before authenticating, preventing later installed-guest trust.
The running builder was left unchanged. A separate, recorded continuation
probed with a disposable host-key file and required authentication with the
run's unique client key plus installed-root checks before promoting a key.
It never qualified an installed key before the container ended; its last
record remains `waiting-for-authenticated-installed-key`, with probe exit 255.
`ssh-qualification-observation.json` records that terminal observation without
rewriting the continuation's incomplete receipt. The continuation only wrote
operation-local SSH trust/evidence files; it did not change guest configuration
or device attachments. Its original source and the exact builder used by the
run are archived as `.py.txt` files with matching recorded hashes.

After the run, the permanent builder was hardened to discard each failed
discovery key, pin only an authenticated installed guest, enforce the pin on
independent boot, retry transient SSH subprocess timeouts within the outer
wait bound, and reject `/dev/vda20` or `/dev/vda2-other` as root-device proof.
`post-run-hardening.json` records before/after hashes and test results. These
changes passed the twelve focused tests but have **not** been qualified by a
new full installation. A next experiment needs installer-stage diagnostics and
qualification of an accelerated runtime, rather than assuming that increasing
the timeout proves the design.

## What these results do not prove

Firmware boot and installer-source presence are not an installed-system proof.
Only a completed install, clean shutdown, and boot with fresh firmware variables
and neither ISO nor cidata attached can satisfy the VM proof. No evidence here
qualifies a physical deployment or alongside installation.

The experiment uses an unencrypted, uniquely credentialed disposable VM.
Deferred provisioning, encryption key lifecycle, real hardware setup, portable
initramfs/boot files, partition extraction and identifiers, Btrfs growth and
snapshots, native writes/recovery, secure production output attestation, and
cross-platform runtime packaging remain open. No shared encrypted template or
deployable image is produced by this work.
