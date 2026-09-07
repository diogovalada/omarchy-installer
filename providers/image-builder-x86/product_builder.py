#!/usr/bin/env python3
"""Product construction, from pinned official ISO, on file-backed guest disks only."""
import argparse
import base64
import contextlib
import hashlib
import hmac
import json
import os
from pathlib import Path
import re
import shutil
import socket
import sys
import time
import uuid

import builder
import artifact_crypto
import product_nbd
from boot_menu import validate_settings
from configuration import configuration, DISK_BYTES, GIB, MIB

ROOT_BYTES = DISK_BYTES - 2 * GIB - 2 * MIB
RECIPE_FILES = ('product_builder.py', 'product-guest.sh', 'product-firstboot.sh', 'boot_menu.py',
                'product-validate.py', 'product-source-lock.json', 'product_nbd.py', 'artifact_crypto.py',
                'builder.py', 'configuration.py', 'release-lock.json',
                'runtime-lock.json', 'Dockerfile', 'runtime-packages.lock',
                'omarchy-signing-key.gpg')


class BuildCancelled(Exception):
    pass


def read_staging_key():
    # One private stdin frame from the elevated operation. No secret in command
    # arguments, environment, manifest, log, ISO, CIDATA, or persistent files.
    frame = sys.stdin.buffer.readline(1025)
    if len(frame) > 1024 or not frame.endswith(b'\n'):
        raise ValueError('Missing bounded private staging-key frame')
    value = json.loads(frame)
    if set(value) != {'protocolVersion', 'stagingKey'} or value['protocolVersion'] != 2:
        raise ValueError('Invalid private staging-key protocol')
    key = bytearray(base64.b64decode(value['stagingKey'], validate=True))
    if len(key) != 32:
        raise ValueError('Staging key must contain 32 random bytes')
    return key


def inspect_encrypted_contract(iso, inspection, lock, run):
    source = json.loads((builder.PROOF / 'product-source-lock.json').read_text(encoding='utf-8-sig'))
    if source['isoVersion'] != lock['version'] or source['isoSha256'] != lock['sha256']:
        raise ValueError('Deferred encryption contract has not been inspected for this release')
    for path, expected in source['isoFiles'].items():
        data = builder.command(['unsquashfs', '-offset', str(inspection['squashfsOffsetBytes']),
                                '-cat', iso, path], timeout=120, text_mode=False).stdout
        if hashlib.sha256(data).hexdigest() != expected:
            raise ValueError('Pinned deferred encryption source changed: ' + path)
        check_cancel()
    result = {'sourceLockSha256': builder.sha256(builder.PROOF / 'product-source-lock.json'),
              'sourceFilesVerified': True, 'installedOwnerContractRequiredInGuest': True,
              'ownerRekey': 'passphrase-slots-only', 'behaviorQualified': False}
    builder.write_json(run / 'encryption-source-inspection.json', result)
    return result


def check_cancel():
    if Path('/output/cancel.requested').exists():
        raise BuildCancelled('Local construction was cancelled; no host disk was changed')


def emit(kind, operation, stage, **fields):
    print(json.dumps(dict(protocolVersion=1, type=kind, operationId=operation,
                          stage=stage, cancelAvailable=True, **fields)), flush=True)


def extract_boot(iso, run):
    # Paths are discovered from the verified ISO, then restricted to the exact
    # single kernel/initramfs pair. No ISO-supplied boot command is executed.
    listing = builder.command(['xorriso', '-indev', iso, '-ls', '/arch/boot/x86_64'])
    kernels = re.findall(r"^'(vmlinuz-[a-zA-Z0-9._-]+)'$", listing.stdout, re.M)
    if len(kernels) != 1:
        raise ValueError('Signed ISO must contain one unambiguous live kernel')
    kernel = kernels[0]
    initrd = 'initramfs-' + kernel.removeprefix('vmlinuz-') + '.img'
    report = builder.command(['xorriso', '-indev', iso, '-pvd_info'])
    label = re.search(r"Volume [Ii]d\s*:\s*'([A-Z0-9_]{1,32})'", report.stdout + report.stderr)
    if not label:
        raise ValueError('Signed ISO volume label is unsupported')
    for name in (kernel, initrd):
        builder.command(['xorriso', '-osirrox', 'on', '-indev', iso,
                         '-extract', '/arch/boot/x86_64/' + name, run / name], timeout=90)
    return kernel, initrd, label[1]


class ProductVM(builder.VM):
    def __init__(self, run, disk, iso, memory, boot, secret_file):
        super().__init__(run, disk, iso, memory)
        self.args += ['-object', 'secret,id=product-build,file=' + str(secret_file)]
        drive = next(i + 1 for i, arg in enumerate(self.args[:-1])
                     if arg == '-drive' and 'id=drive0' in self.args[i + 1])
        if ',format=qcow2' not in self.args[drive]:
            self.args[drive] += ',format=qcow2'
        self.args[drive] += ',encrypt.key-secret=product-build'
        # No guest network or SSH; the build recipe is driven on the guest's
        # explicitly enabled serial debug shell. Nothing is enabled on Windows.
        for flag in ('-netdev',):
            i = self.args.index(flag)
            del self.args[i:i + 2]
        for i in range(len(self.args) - 2, -1, -1):
            if self.args[i] == '-device' and self.args[i + 1].startswith('virtio-net-pci'):
                del self.args[i:i + 2]
        i = self.args.index('-serial')
        self.args[i + 1] = 'unix:/tmp/product-serial.sock,server=on,wait=off'
        kernel, initrd, label = boot
        self.args += ['-kernel', str(run / kernel), '-initrd', str(run / initrd),
                      '-append', 'archisobasedir=arch archisolabel=' + label +
                      ' console=ttyS0,115200 systemd.debug_shell=ttyS0'
                      ' systemd.mask=getty@tty1.service systemd.mask=serial-getty@ttyS0.service'
                      ' systemd.unit=multi-user.target']


def run_guest(vm, run, operation, timeout):
    deadline = time.monotonic() + timeout
    ready = False
    started = False
    complete = False
    metadata = None
    tail = ''
    with socket.socket(socket.AF_UNIX) as serial, (run / 'guest.log').open('wb') as log:
        serial.settimeout(1)
        serial.connect('/tmp/product-serial.sock')
        next_progress = 0
        while time.monotonic() < deadline:
            check_cancel()
            try:
                chunk = serial.recv(65536)
            except socket.timeout:
                chunk = None
            if chunk:
                # Raw upstream serial text can contain installer debug data.
                # Retain it only in this bounded RAM tail, never on Windows.
                tail = (tail + chunk.decode('utf-8', errors='replace'))[-262144:]
                if not ready and re.search(r'(?:sh|bash)-[0-9.]+#\s|root@[^\r\n]+#\s', tail):
                    serial.sendall(b"stty -echo; echo OMARCHY_SERIAL_READY\n")
                    ready = True
                    log.write(b'Guest serial shell ready\n')
                if ready and not started and re.search(r'\r?\nOMARCHY_SERIAL_READY\r?\n', tail):
                    # Fixed shipped script only. CIDATA carries config/data and
                    # these packaged scripts; no user supplied commands exist.
                    serial.sendall(b"mkdir -p /run/omarchy-cidata; mount -o ro /dev/disk/by-label/CIDATA /run/omarchy-cidata && /bin/bash /run/omarchy-cidata/product-guest.sh\n")
                    started = True
                    log.write(b'Pinned upstream deferred installer started\n')
                if 'OMARCHY_PRODUCT_FAILED' in tail:
                    log.write(b'Guest construction failed; no raw guest log retained\n')
                    raise RuntimeError('Guest construction failed; see guest.log')
                match = re.search(r'OMARCHY_PRODUCT_METADATA ([A-Za-z0-9+/=]+)\r?\n', tail)
                if match:
                    metadata = json.loads(base64.b64decode(match[1]))
                complete = complete or bool(re.search(r'\r?\nOMARCHY_PRODUCT_COMPLETE\r?\n', tail))
            if vm.process.poll() is not None:
                if complete and metadata and vm.process.returncode == 0:
                    log.write(b'Guest completed and shut down cleanly\n')
                    return metadata
                raise RuntimeError('Guest stopped without a clean construction receipt')
            if chunk == b'':
                time.sleep(0.2)
            if time.monotonic() >= next_progress:
                emit('progress', operation, 'constructing', message='Official installer is running in the isolated local VM')
                next_progress = time.monotonic() + 30
        raise TimeoutError('Construction timed out; no deployable manifest was produced')


def build(operation, memory, timeout, staging_key, secret_file, boot_menu):
    validate_settings(boot_menu)
    run = Path('/output')
    builder.RUNS = run
    check_cancel()
    if list(run.iterdir()):
        raise ValueError('Output directory must be newly created and empty')
    lock = json.loads((builder.PROOF / 'release-lock.json').read_text(encoding='utf-8-sig'))
    emit('progress', operation, 'verifying', message='Checking signed official source and runtime inputs')
    with contextlib.redirect_stdout(__import__('sys').stderr):
        iso = builder.verify_source(lock, run)
        inspection = builder.inspect_iso(iso, run)
        runtime = builder.preflight()
    check_cancel()
    if not inspection['deferredProvisioningSourcePresent']:
        raise ValueError('Pinned ISO lacks the deferred provisioning contract')
    encryption_contract = inspect_encrypted_contract(iso, inspection, lock, run)
    if shutil.disk_usage(run).free < 85 * GIB:
        raise ValueError('85 GiB free output space is required after staging the ISO')
    cidata = run / 'cidata'
    cidata.mkdir(mode=0o700)
    builder.write_json(cidata / 'operation.json', {'operationId': operation, 'bootMenu': boot_menu})
    builder.write_json(cidata / 'user_configuration.json',
                       configuration(deferred=True, encrypted=True, hostname='omarchy-' + operation[:8]))
    (cidata / 'defer-provisioning').touch()
    (cidata / 'user_encrypt_installation.txt').write_text('true\n')
    for name in ('product-guest.sh', 'product-firstboot.sh', 'product-validate.py', 'product-source-lock.json', 'boot_menu.py'):
        shutil.copyfile(builder.PROOF / name, cidata / name)
    with (run / 'cidata.img').open('xb') as f:
        f.truncate(4 * MIB)
    builder.command(['mkfs.vfat', '-n', 'CIDATA', run / 'cidata.img'])
    builder.command(['mcopy', '-i', run / 'cidata.img', *sorted(cidata.iterdir()), '::/'])
    disk = run / 'image.building.qcow2'
    builder.command(['qemu-img', 'create', '--object', 'secret,id=product-build,file=' + str(secret_file),
                     '-f', 'qcow2', '-o', 'encrypt.format=luks,encrypt.key-secret=product-build',
                     disk, str(DISK_BYTES)], timeout=120)
    shutil.copyfile(builder.VARS, run / 'OVMF_VARS.fd')
    boot = extract_boot(iso, run)
    with ProductVM(run, disk, iso, memory, boot, secret_file) as vm:
        identity = run_guest(vm, run, operation, timeout)
    check_cancel()
    if (identity['operationId'] != operation or identity.get('buildCredentials') is not False
            or identity.get('encryption') != 'luks2' or identity.get('bootstrapKey') != 'upstream-per-install'
            or identity.get('protectionState') != 'owner-setup-required'
            or identity.get('ownerRekey') != 'passphrase-slots-only'
            or identity.get('firstBootGrowth') != 'luks-mapping-and-btrfs'):
        raise ValueError('Guest metadata is not for this credential-free operation')
    uuid.UUID(identity['rootLuksUuid'])
    outputs = []
    export_deadline = time.monotonic() + 1800
    def export_cancel():
        check_cancel()
        if time.monotonic() > export_deadline:
            raise TimeoutError('Encrypted partition export exceeded its 30 minute bound')
    with product_nbd.open_image(run, disk, secret_file, export_cancel) as reader:
        for role, start, size in [('esp', MIB, 2 * GIB), ('root', 2 * GIB + MIB, ROOT_BYTES)]:
            label = 'boot files' if role == 'esp' else 'Omarchy system'
            emit('progress', operation, 'exporting', completedBytes=0, totalBytes=size,
                 message='Preparing encrypted ' + label + '…')
            last_progress = [0.0]
            def progress(done):
                if time.monotonic() - last_progress[0] >= 30:
                    emit('progress', operation, 'exporting', completedBytes=done, totalBytes=size,
                         message='Preparing encrypted ' + label + '…')
                    last_progress[0] = time.monotonic()
            item = artifact_crypto.encrypt_image(run / (role + '.img.enc'), staging_key,
                                                reader.chunks(start, size), size, operation, role,
                                                export_cancel, progress)
            item.update(role=role, filesystem='fat32' if role == 'esp' else 'crypto_LUKS',
                        partitionGuid=identity[role + 'PartitionGuid'])
            outputs.append(item)
    manifest = {'schemaVersion': 2, 'kind': 'omarchy-local-direct-image', 'operationId': operation,
                'model': 'gpt-6-astra', 'source': lock,
                'runtimeLock': json.loads((builder.PROOF / 'runtime-lock.json').read_text(encoding='utf-8-sig')),
                'recipeFiles': {name: builder.sha256(builder.PROOF / name) for name in RECIPE_FILES},
                'firmwareSha256': {p.name: builder.sha256(p) for p in (builder.CODE, builder.VARS)},
                'runtimePackagesSha256': builder.sha256(Path('/runtime-packages.tsv')),
                'runtime': runtime, 'identity': identity, 'outputs': outputs, 'bootMenu': boot_menu,
                'encryption': 'luks2', 'encryptionContract': encryption_contract,
                'stagingProtection': 'ephemeral-key-encrypted-artifacts',
                'logicalSectorBytes': 512, 'alignmentBytes': MIB,
                'minimumUnallocatedBytes': 2 * GIB + ROOT_BYTES,
                'bootPath': '\\EFI\\limine\\limine_x64.efi',
                'qualification': {'constructionCompleted': True, 'independentBootTested': False,
                                  'physicalHardwareTested': False, 'ownerProvisioningTested': False},
                'createdAt': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}
    check_cancel()
    builder.write_json(run / 'manifest.json', manifest)
    emit('result', operation, 'built', manifestFile='manifest.json',
         manifestSha256=builder.sha256(run / 'manifest.json'), result=manifest)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--operation-id', required=True)
    parser.add_argument('--boot-default', required=True, choices=('omarchy', 'windows'))
    parser.add_argument('--boot-timeout', required=True, type=int, choices=(5, 10, 15, 30))
    parser.add_argument('--memory', type=int, default=6144, choices=(6144, 8192))
    parser.add_argument('--timeout', type=int, default=7200, choices=range(60, 14401))
    args = parser.parse_args()
    operation = str(uuid.UUID(args.operation_id))
    os.umask(0o077)
    import resource
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    key = None
    secret_file = Path('/tmp/product-qcow.key')
    try:
        key = read_staging_key()
        # /tmp is a bounded container tmpfs supplied by the provider. Refuse a
        # disk-backed location: the native QCOW2 unlock secret must not persist.
        if builder.command(['findmnt', '-n', '-o', 'FSTYPE', '-T', '/tmp']).stdout.strip() != 'tmpfs':
            raise ValueError('Construction requires a private tmpfs for the QEMU secret')
        qcow_key = hmac.new(key, b'omarchy-qcow2-v1:' + operation.encode('ascii'), hashlib.sha256).digest()
        with secret_file.open('xb') as secret:
            secret.write(base64.b64encode(qcow_key))
        build(operation, args.memory, args.timeout, key, secret_file,
              {'defaultOs': args.boot_default, 'timeoutSeconds': args.boot_timeout})
    except BuildCancelled as error:
        emit('error', operation, 'build', code='build_cancelled', message=str(error))
        return 1
    except (Exception, KeyboardInterrupt) as error:
        emit('error', operation, 'build', code='construction_failed', message=str(error))
        return 1
    finally:
        secret_file.unlink(missing_ok=True)
        if key is not None:
            key[:] = bytes(len(key))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
