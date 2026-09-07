"""Build a fixed UEFI ISO bootstrap with the distro's GRUB tooling."""
import hashlib
import json
import os
import pathlib
import subprocess

out = pathlib.Path('/output')
out.mkdir(parents=True, exist_ok=True)
bootstrap = out / 'bootstrap.cfg'
# The EFI platform resolves the image's own device into prefix. Enter normal
# mode to read that device's /EFI/Omarchy/grub.cfg; never search other ESPs.
bootstrap.write_text('normal\n')
os.environ['SOURCE_DATE_EPOCH'] = '0'
efi = out / 'BOOTX64.EFI'
subprocess.run(['grub-mkimage', '--format=x86_64-efi', '--output=' + str(efi),
                '--prefix=/EFI/Omarchy', '--config='+str(bootstrap),
                *'part_gpt part_msdos fat ntfs exfat iso9660 loopback linux search search_fs_uuid search_fs_file probe regexp normal configfile all_video gfxterm echo sleep test serial terminal'.split()], check=True)
data = efi.read_bytes()
packages = subprocess.check_output(['dpkg-query', '-W', 'grub-efi-amd64-bin', 'grub-common'], text=True)
(out / 'loader-provenance.json').write_text(json.dumps({
    'schema': 1, 'platform': 'x64-uefi', 'secureBoot': False,
    'sha256': hashlib.sha256(data).hexdigest(), 'length': len(data),
    'packages': packages, 'buildCommand': 'qualification/build-loader.py',
    'source': 'https://archive.ubuntu.com/ubuntu/pool/main/g/grub2/',
}, indent=2) + '\n')
print((out / 'loader-provenance.json').read_text())
