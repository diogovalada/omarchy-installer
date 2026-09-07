"""Collect the exact Ubuntu source package and licenses for the GRUB binary."""
import hashlib
import json
import pathlib
import shutil
import subprocess

sources = pathlib.Path('/etc/apt/sources.list.d/ubuntu.sources')
sources.write_text(sources.read_text().replace('Types: deb\n', 'Types: deb deb-src\n'))
subprocess.run(['apt-get', 'update'], check=True)
out = pathlib.Path('/output/sources')
out.mkdir(exist_ok=True)
subprocess.run(['apt-get', 'source', '--download-only', 'grub2=2.12-1ubuntu7.3'], cwd=out, check=True)
shutil.copy('/usr/share/doc/grub-common/copyright', '/output/GRUB-COPYRIGHT.txt')
shutil.copy('/usr/share/common-licenses/GPL-3', '/output/GPL-3.txt')
records = []
for path in sorted(out.iterdir()):
    if path.is_file():
        records.append({'path': 'sources/'+path.name, 'length': path.stat().st_size,
                        'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
(out / 'source-lock.json').write_text(json.dumps({'sourcePackage': 'grub2', 'version': '2.12-1ubuntu7.3',
    'authentication': 'Ubuntu APT signed source indexes; apt-get source --download-only',
    'files': records}, indent=2)+'\n')
print(json.dumps(records, indent=2), flush=True)
