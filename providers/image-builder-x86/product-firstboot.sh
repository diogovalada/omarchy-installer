#!/bin/bash
# Executed by the deployed OS before its upstream owner-provisioning service.
set -Eeuo pipefail
export LC_ALL=C
state=/var/lib/omarchy/direct-install
test -f "$state/pending"
test "$(findmnt -n -o FSTYPE /)" = btrfs
# Grow the already-open LUKS mapping before Btrfs. The physical partition was
# allocated by the reviewed Windows plan; this service never changes its GPT.
# Use the stock temporary key only while upstream owner setup remains pending.
python - <<'PY'
import json
from pathlib import Path
import re
import subprocess
import uuid

def out(*args):
    return subprocess.check_output(args, text=True).strip()
def require(condition, message):
    if not condition:
        raise RuntimeError(message)

state = Path('/var/lib/omarchy/direct-install')
expected = json.loads((state / 'root-identity.json').read_text())
source = out('findmnt', '-n', '-o', 'SOURCE', '/').split('[', 1)[0]
mapper = Path(source).resolve(strict=True)
require(re.fullmatch(r'dm-\d+', mapper.name), 'Root must be the installed LUKS mapping')
name = Path('/sys/class/block', mapper.name, 'dm/name').read_text().strip()
status = out('cryptsetup', 'status', name)
require(re.search(r'^\s*type:\s+LUKS2$', status, re.M), 'Root must use LUKS2')
match = re.search(r'^\s*device:\s+(\S+)$', status, re.M)
require(match, 'Cannot identify the root backing partition')
device = str(Path(match[1]).resolve(strict=True))
require(str(uuid.UUID(out('cryptsetup', 'luksUUID', device))) == expected['rootLuksUuid'], 'Root LUKS identity changed')
require(str(uuid.UUID(out('blkid', '-s', 'UUID', '-o', 'value', source))) == expected['rootFilesystemUuid'], 'Root Btrfs identity changed')
require(str(uuid.UUID(out('blkid', '-s', 'PARTUUID', '-o', 'value', device))) == expected['rootPartitionGuid'], 'Root partition identity changed')
size = int(out('blockdev', '--getsize64', device))
require(size >= expected['sourceRootPartitionBytes'], 'Target root partition is smaller than the source image')
metadata = json.loads(out('cryptsetup', 'luksDump', '--dump-json-metadata', device))
require(len(metadata['segments']) == 1, 'Unsupported LUKS data layout')
segment = next(iter(metadata['segments'].values()))
require(segment['type'] == 'crypt' and segment['size'] == 'dynamic', 'Root encryption is not expandable')
target_size = (size - int(segment['offset'])) // int(segment['sector_size']) * int(segment['sector_size'])
require(int(out('blockdev', '--getsize64', source)) <= target_size, 'Refusing to shrink the root mapping')
provisioning = Path('/var/lib/omarchy/provisioning')
require((provisioning / 'pending').is_file() and (provisioning / 'luks-key').is_file(), 'Upstream owner handoff state missing')
subprocess.run(['cryptsetup', 'resize', '--key-file', str(provisioning / 'luks-key'), name], check=True)
require(int(out('blockdev', '--getsize64', source)) == target_size, 'LUKS mapping did not grow to the allocated partition')
PY
btrfs filesystem resize max /
udevadm settle
# Run upstream's idempotent detector against actual hardware, with the same
# signed-ISO package repository the installer used. Restore normal repositories
# even when detection fails; retain pending state for a diagnosable retry.
test -x /usr/bin/omarchy-apply-hardware
test -d "$state/offline"
test -f "$state/pacman-offline.conf"
test -e "$state/pacman.conf.saved" || cp -a /etc/pacman.conf "$state/pacman.conf.saved"
mkdir -p /var/cache/omarchy/mirror/offline
mount --bind "$state/offline" /var/cache/omarchy/mirror/offline
cleanup() {
  cp -a "$state/pacman.conf.saved" /etc/pacman.conf
  umount /var/cache/omarchy/mirror/offline
}
trap cleanup EXIT
cp "$state/pacman-offline.conf" /etc/pacman.conf
pacman -Sy --noconfirm
OMARCHY_FIRST_INSTALL=1 /usr/bin/omarchy-apply-hardware --defer-provisioning
mkinitcpio -P
limine-update
/usr/local/sbin/omarchy-boot-menu --check
cleanup
trap - EXIT
# The factory baseline must include the real computer's kernel/driver setup.
# Refresh it before upstream asks for an owner, while no personal account
# exists. The staged name and backup make an interrupted refresh retryable.
python - <<'PY'
import json
from pathlib import Path
import subprocess

def run(*args):
    subprocess.run(args, check=True)
source = subprocess.check_output(['findmnt', '-n', '-o', 'SOURCE', '/'], text=True).strip().split('[', 1)[0]
state = Path('/var/lib/omarchy/direct-install')
scrub_paths = json.loads((state / 'factory-scrub.json').read_text())
if not isinstance(scrub_paths, list) or not scrub_paths:
    raise RuntimeError('Pinned upstream factory scrub data missing')
for item in scrub_paths:
    if not isinstance(item, str) or not item or Path(item).is_absolute() or '..' in Path(item).parts:
        raise RuntimeError('Invalid factory scrub path')
if any(1000 <= int(line.split(':')[2]) < 65534 for line in Path('/etc/passwd').read_text().splitlines()):
    raise RuntimeError('Cannot refresh factory state after owner creation')
top = Path('/run/omarchy-direct-factory')
top.mkdir(mode=0o700, exist_ok=True)
run('mount', '-o', 'subvolid=5', source, str(top))
try:
    fresh, factory, backup = (top / name for name in ('@factory-direct-new', '@factory', '@factory-direct-old'))
    if fresh.exists():
        run('btrfs', 'subvolume', 'delete', str(fresh))
    run('btrfs', 'subvolume', 'snapshot', str(top / '@'), str(fresh))
    for item in scrub_paths + ['var/lib/omarchy/direct-install/pending']:
        (fresh / item).unlink(missing_ok=True)
    (fresh / 'etc/machine-id').write_text('')
    (fresh / 'var/lib/dbus/machine-id').unlink(missing_ok=True)
    for key in (fresh / 'etc/ssh').glob('ssh_host_*'):
        key.unlink()
    run('btrfs', 'property', 'set', '-ts', str(fresh), 'ro', 'true')
    run('sync')
    if factory.exists():
        if backup.exists():
            run('btrfs', 'subvolume', 'delete', str(backup))
        factory.rename(backup)
    fresh.rename(factory)
    run('sync')
    if backup.exists():
        run('btrfs', 'subvolume', 'delete', str(backup))
finally:
    run('umount', str(top))
PY
sync
mv "$state/pending" "$state/completed"
