#!/bin/bash
# Fixed guest-only construction recipe. Never execute on a host or accept shell input.
set -Eeuo pipefail
export LC_ALL=C
exec > >(tee /tmp/omarchy-product-build.log /dev/ttyS0) 2>&1
trap 'echo OMARCHY_PRODUCT_FAILED; sync; systemctl poweroff --no-block' ERR
test -d /run/archiso
test "$(lsblk -dn -o TYPE /dev/vda)" = disk
test "$(blockdev --getsize64 /dev/vda)" = 42949672960
test ! -e /dev/vdb
test -f /run/omarchy-cidata/defer-provisioning
test ! -e /run/omarchy-cidata/user_credentials.json
test "$(cat /run/omarchy-cidata/user_encrypt_installation.txt)" = true
echo OMARCHY_PRODUCT_INSTALLING
/usr/local/bin/omarchy-iso-install \
  --config /run/omarchy-cidata/user_configuration.json \
  --creds /run/omarchy-cidata/user_credentials.json \
  --defer-provisioning-file /run/omarchy-cidata/defer-provisioning \
  --encrypt-file /run/omarchy-cidata/user_encrypt_installation.txt
test "$(findmnt -n -o FSTYPE /mnt)" = btrfs
root_source=$(findmnt -n -o SOURCE /mnt | cut -d '[' -f 1)
cryptsetup isLuks --type luks2 /dev/vda2
test "$(lsblk -dn -o TYPE "$root_source")" = crypt
test -f /mnt/var/lib/omarchy/provisioning/pending
test -x /mnt/usr/bin/omarchy-provision-owner
test -x /mnt/usr/bin/omarchy-apply-hardware
test -L /mnt/etc/systemd/system/multi-user.target.wants/omarchy-provision-owner.service
test -s /mnt/boot/EFI/BOOT/BOOTX64.EFI
test -s /mnt/boot/EFI/limine/limine_x64.efi
echo OMARCHY_PRODUCT_FINALIZING

# The VM must not determine which physical storage/USB/input modules can boot.
# Remove autodetect from the stock hook list; retain all upstream remaining hooks.
mkdir -p /mnt/etc/mkinitcpio.conf.d
cat > /mnt/etc/mkinitcpio.conf.d/zz-omarchy-portable.conf <<'EOF'
HOOKS=(${HOOKS[@]/autodetect/})
MODULES+=(nvme ahci xhci_pci usbhid hid_generic)
EOF
install -m 0755 /run/omarchy-cidata/product-firstboot.sh /mnt/usr/local/sbin/omarchy-direct-firstboot
cat > /mnt/etc/systemd/system/omarchy-direct-firstboot.service <<'EOF'
[Unit]
Description=Finalize locally constructed Omarchy on this computer
After=local-fs.target omarchy-system-factory-reset-finish.service
Before=omarchy-provision-owner.service display-manager.service
ConditionPathExists=/var/lib/omarchy/direct-install/pending
[Service]
Type=oneshot
ExecStart=/usr/local/sbin/omarchy-direct-firstboot
RemainAfterExit=yes
[Install]
WantedBy=multi-user.target
EOF
mkdir -p /mnt/etc/systemd/system/omarchy-provision-owner.service.d
cat > /mnt/etc/systemd/system/omarchy-provision-owner.service.d/direct-install.conf <<'EOF'
[Unit]
Requires=omarchy-direct-firstboot.service
After=omarchy-direct-firstboot.service
EOF
mkdir -p /mnt/var/lib/omarchy/direct-install
cp /run/omarchy-cidata/operation.json /mnt/var/lib/omarchy/direct-install/operation.json
# Build the OS menu from the upstream-generated kernel entry, then keep its
# advanced group as the explicit target for future kernel and snapshot updates.
install -m 0755 /run/omarchy-cidata/boot_menu.py /mnt/usr/local/sbin/omarchy-boot-menu
python - <<'PY'
import json
import subprocess
import uuid
from pathlib import Path
value = json.loads(Path('/run/omarchy-cidata/operation.json').read_text())['bootMenu']
Path('/mnt/etc/omarchy-boot-menu.json').write_text(json.dumps(value) + '\n')
def field(name):
    return subprocess.check_output(['blkid', '-s', name, '-o', 'value', '/dev/vda1'], text=True).strip()
Path('/mnt/etc/omarchy-boot-identity.json').write_text(json.dumps({
    'espPartitionGuid': str(uuid.UUID(field('PARTUUID'))), 'espFilesystemUuid': field('UUID'),
}) + '\n')
PY
arch-chroot /mnt /usr/local/sbin/omarchy-boot-menu --apply
cat >> /mnt/etc/default/limine <<'EOF'

# Omarchy Setup keeps upstream kernels and snapshots in this group.
TARGET_OS_NAME="Advanced Omarchy options"
# The constructed image has its own ESP and explicit Windows firmware route.
FIND_BOOTLOADERS=no
EOF
mkdir -p /mnt/etc/boot/hooks/post.d /mnt/etc/boot/hooks/pre.d
cat > /mnt/etc/boot/hooks/pre.d/15-omarchy-boot-identity <<'EOF'
#!/bin/sh
exec /usr/local/sbin/omarchy-boot-menu --prepare-update
EOF
chmod 0755 /mnt/etc/boot/hooks/pre.d/15-omarchy-boot-identity
cat > /mnt/etc/boot/hooks/post.d/85-omarchy-boot-menu <<'EOF'
#!/bin/sh
exec /usr/local/sbin/omarchy-boot-menu --apply
EOF
chmod 0755 /mnt/etc/boot/hooks/post.d/85-omarchy-boot-menu
# Preserve the signed ISO's offline repository for real hardware discovery.
# Its packages and repository metadata are covered by the ISO signature; this
# is local per-install content, never a separately downloaded root filesystem.
test -d /var/cache/omarchy/mirror/offline
cp -a /var/cache/omarchy/mirror/offline /mnt/var/lib/omarchy/direct-install/offline
cp /etc/pacman.conf /mnt/var/lib/omarchy/direct-install/pacman-offline.conf
touch /mnt/var/lib/omarchy/direct-install/pending
arch-chroot /mnt systemctl enable omarchy-direct-firstboot.service
arch-chroot /mnt mkinitcpio -P
arch-chroot /mnt limine-update
arch-chroot /mnt /usr/local/sbin/omarchy-boot-menu --check

# The validator allows upstream's stable /dev/mapper/root, and checks its
# cryptdevice= UUID/PARTUUID against this fresh LUKS container. The exported
# range is still /dev/vda2; only filesystem operations use the opened mapping.
python /run/omarchy-cidata/product-validate.py
test -z "$(awk -F: '$3 >= 1000 && $3 < 65534 {print $1}' /mnt/etc/passwd)"
test -z "$(find /mnt/root /mnt/home /mnt/var/lib/omarchy/provisioning -name authorized_keys -print)"
arch-chroot /mnt passwd -l root
rm -f /mnt/etc/ssh/ssh_host_* /mnt/var/lib/dbus/machine-id
: > /mnt/etc/machine-id

# Refresh the factory state after portability/identity changes, so factory reset
# cannot restore VM-specific configuration or a build-machine identity.
mkdir -p /run/omarchy-top
mount -o subvolid=5 "$root_source" /run/omarchy-top
if btrfs subvolume show /run/omarchy-top/@factory >/dev/null 2>&1; then
  btrfs subvolume delete /run/omarchy-top/@factory
fi
btrfs subvolume snapshot /run/omarchy-top/@ /run/omarchy-top/@factory
# Use the inspected ISO's own scrub routine. A factory image must not retain
# the temporary slot key or drop-ins after the owner retires that key slot.
python - <<'PY'
import json, sys
from pathlib import Path
sys.path.insert(0, '/usr/share/omarchy-iso')
from orchestrator.phases_impl import _scrub_factory_snapshot, FACTORY_SCRUB_PATHS
factory = Path('/run/omarchy-top/@factory')
_scrub_factory_snapshot(factory)
(factory / 'var/lib/omarchy/direct-install/pending').unlink(missing_ok=True)
# First boot runs on the installed OS, which has no live-ISO orchestrator.
# Carry only its inspected scrub path data for the physical factory refresh.
scrub_data = json.dumps(FACTORY_SCRUB_PATHS) + '\n'
Path('/mnt/var/lib/omarchy/direct-install/factory-scrub.json').write_text(scrub_data)
(factory / 'var/lib/omarchy/direct-install/factory-scrub.json').write_text(scrub_data)
if any((factory / path).exists() for path in FACTORY_SCRUB_PATHS):
    raise RuntimeError('Upstream factory credential scrub was incomplete')
PY
btrfs property set -ts /run/omarchy-top/@factory ro true

python - <<'PY'
import base64
from pathlib import Path
print('OMARCHY_PRODUCT_METADATA '+base64.b64encode(Path('/run/omarchy-product-identity.json').read_bytes()).decode(), flush=True)
PY
umount /run/omarchy-top
umount -R /mnt
sync
echo OMARCHY_PRODUCT_COMPLETE
systemctl poweroff --no-block
