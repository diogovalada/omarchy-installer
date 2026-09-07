#!/usr/bin/python3
"""Maintain the installed PC menu after upstream kernel/snapshot updates.

The upstream entry tool remains responsible for kernels, initramfs, UUIDs,
snapshots and file hashes. This post-hook copies its current normal kernel entry
into the primary OS choice and retains the upstream tree as advanced options.
It never calls the entry tool from inside its hook and never writes EFI variables.
"""
import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import uuid

SETTINGS = Path('/etc/omarchy-boot-menu.json')
CONFIG = Path('/boot/limine.conf')
IDENTITY = Path('/etc/omarchy-boot-identity.json')
TEMPLATES = (Path('/usr/share/omarchy/install/assets/limine/limine.conf'),
             Path('/usr/share/omarchy/default/limine/limine.conf'))
ADVANCED = 'Advanced Omarchy options'
GLOBAL_SETTINGS = {'timeout', 'default_entry', 'remember_last_entry', 'quiet'}


def validate_settings(value):
    if not isinstance(value, dict) or set(value) != {'defaultOs', 'timeoutSeconds'}:
        raise ValueError('Startup settings must contain only defaultOs and timeoutSeconds')
    if value['defaultOs'] not in ('omarchy', 'windows'):
        raise ValueError('Startup default must be omarchy or windows')
    if type(value['timeoutSeconds']) is not int or value['timeoutSeconds'] not in (5, 10, 15, 30):
        raise ValueError('Startup countdown must be 5, 10, 15 or 30 seconds')
    return value


def option(line):
    match = re.match(r'^\s*([a-zA-Z_]+)\s*:\s*(.*?)\s*$', line)
    return (match[1].lower(), match[2]) if match else (None, None)


@dataclass
class Entry:
    level: int
    title: str
    expanded: bool
    body: list[str]

    def values(self, key):
        return [value for line in self.body for name, value in [option(line)] if name == key]

    def machine(self):
        return any(re.search(r'\bmachine-id=[0-9a-fA-F]{32}\b', value) for value in self.values('comment'))

    def text(self):
        return '/' * self.level + ('+' if self.expanded else '') + self.title + '\n' + '\n'.join(self.body).strip('\n')


def render(original, settings):
    validate_settings(settings)
    if '\x00' in original or len(original.encode()) > 4 * 1024 * 1024:
        raise ValueError('Unsupported boot configuration')
    header, entries = [], []
    for line in original.splitlines():
        match = re.match(r'^\s*(/+)(\+?)([^/\r\n].*?)\s*$', line)
        if match:
            entries.append(Entry(len(match[1]), match[3], bool(match[2]), []))
        elif option(line)[0] not in GLOBAL_SETTINGS:
            (entries[-1].body if entries else header).append(line)
    groups = [i for i, entry in enumerate(entries) if entry.level == 1 and entry.machine()]
    if len(groups) != 1:
        raise ValueError('Expected one upstream Omarchy kernel group')
    group_index = groups[0]
    group = entries[group_index]
    end = next((i for i in range(group_index + 1, len(entries)) if entries[i].level == 1), len(entries))
    kernels = []
    for i in range(group_index + 1, end):
        entry = entries[i]
        if entry.level != 2 or (i + 1 < end and entries[i + 1].level > entry.level):
            continue
        if re.search(r'fallback|rescue|snapshot', entry.title, re.I):
            continue
        if entry.values('protocol') in (['linux'], ['efi']):
            kernels.append(entry)
    if not kernels:
        raise ValueError('No normal upstream kernel entry is available for Omarchy')
    kernel = kernels[0]
    if not any(kernel.values(name) for name in ('path', 'kernel_path', 'image_path')):
        raise ValueError('The upstream kernel entry has no boot image path')
    kept = []
    i = 0
    while i < len(entries):
        entry = entries[i]
        if entry.level == 1 and entry.title in ('Omarchy', 'Windows') and i != group_index:
            expected = 'Start Omarchy' if entry.title == 'Omarchy' else 'Start Windows after a brief restart'
            if entry.values('comment') != [expected] or (i + 1 < len(entries) and entries[i + 1].level > 1):
                raise ValueError('An unmanaged entry conflicts with the installed OS menu')
            if entry.title == 'Windows' and (entry.values('protocol') != ['efi_boot_entry'] or entry.values('entry') != ['Windows Boot Manager']):
                raise ValueError('The Windows menu entry was changed outside its settings')
        else:
            kept.append(entry)
        i += 1
    group.title, group.expanded = ADVANCED, False
    primary = Entry(1, 'Omarchy', False, ['  comment: Start Omarchy'] + [line for line in kernel.body if option(line)[0] != 'comment'])
    windows = Entry(1, 'Windows', False, ['  comment: Start Windows after a brief restart', '  protocol: efi_boot_entry', '  entry: Windows Boot Manager'])
    default = 'Omarchy' if settings['defaultOs'] == 'omarchy' else 'Windows'
    controls = f"timeout: {settings['timeoutSeconds']}\ndefault_entry: {default}\nremember_last_entry: no\nquiet: no"
    return '\n\n'.join(part for part in ('\n'.join(header).strip('\n'), controls, primary.text(), windows.text(), *(entry.text() for entry in kept)) if part) + '\n'


def regular(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 4 * 1024 * 1024:
        raise ValueError(f'Expected a regular file: {path}')
    return path.read_text()


def is_reset_template(original, reset_template):
    return (original == reset_template and bool(original.strip()) and '\x00' not in original
            and len(original.encode()) <= 4 * 1024 * 1024
            and not any(re.match(r'^\s*/', line) for line in original.splitlines()))


def after_update(original, settings, reset_template, caller):
    # limine-update runs limine-install (EFI binaries only) before the kernel
    # generator. Its post-hook must also allow the exact temporary reset state.
    # The subsequent kernel-generator post-hook and every --check stay strict.
    if caller == 'limine-install' and is_reset_template(original, reset_template):
        validate_settings(settings)
        return original
    return render(original, settings)


def rebind_machine(original, machine_id, reset_template=None):
    if not re.fullmatch('[0-9a-f]{32}', machine_id):
        raise ValueError('The installed machine ID is unavailable')
    lines = original.splitlines(keepends=True)
    roots = [i for i, line in enumerate(lines) if re.fullmatch(r'\s*/\+?Advanced Omarchy options\s*', line)]
    # Owner rekey and factory reset deliberately restore the upstream template
    # before generating any entries. Let that generation run, then let our
    # post-hook restore the menu. Never accept arbitrary missing/corrupt groups.
    if not roots and is_reset_template(original, reset_template):
        return original
    if len(roots) != 1:
        raise ValueError('The managed advanced kernel group is unavailable')
    start = roots[0] + 1
    end = next((i for i in range(start, len(lines)) if re.match(r'^\s*/', lines[i])), len(lines))
    count = 0
    for i in range(start, end):
        if option(lines[i])[0] == 'comment':
            lines[i], changed = re.subn(r'\bmachine-id=[0-9a-fA-F]{32}\b', 'machine-id=' + machine_id, lines[i])
            count += changed
    if count != 1:
        raise ValueError('The upstream kernel group has an ambiguous machine identity')
    return ''.join(lines)


def atomic(path, text):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', newline='\n', dir=path.parent, prefix='.omarchy-menu-', delete=False) as file:
            temporary = Path(file.name)
            file.write(text)
            file.flush()
            os.fsync(file.fileno())
        os.replace(temporary, path)
        temporary = None
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def apply(check=False, prepare_update=False):
    if os.geteuid() != 0:
        raise PermissionError('Startup menu configuration requires root')
    if subprocess.check_output(['findmnt', '-n', '-o', 'FSTYPE', '--mountpoint', '/boot'], text=True).strip() != 'vfat':
        raise ValueError('The installed EFI partition must be mounted at /boot')
    identity = json.loads(regular(IDENTITY))
    device = subprocess.check_output(['findmnt', '-n', '-o', 'SOURCE', '--mountpoint', '/boot'], text=True).strip()
    part = subprocess.check_output(['blkid', '-s', 'PARTUUID', '-o', 'value', device], text=True).strip()
    fs = subprocess.check_output(['blkid', '-s', 'UUID', '-o', 'value', device], text=True).strip()
    if set(identity) != {'espPartitionGuid', 'espFilesystemUuid'} or str(uuid.UUID(part)) != identity['espPartitionGuid'] or fs != identity['espFilesystemUuid']:
        raise ValueError('The mounted EFI partition is not the one belonging to this installation')
    for shadow in (Path('/boot/EFI/limine/limine.conf'), Path('/boot/EFI/BOOT/limine.conf')):
        if shadow.exists() or shadow.is_symlink():
            raise ValueError('Another configuration shadows the managed startup menu')
    settings = validate_settings(json.loads(regular(SETTINGS)))
    original = regular(CONFIG)
    # Same first-existing template order as the upstream reset code.
    template = next((regular(path) for path in TEMPLATES if path.exists()), None)
    if prepare_update:
        updated = rebind_machine(original, regular(Path('/etc/machine-id')).strip(), template)
    else:
        updated = render(original, settings) if check else after_update(original, settings, template, os.environ.get('HOOK_CALLER'))
    if check:
        if updated != original:
            raise ValueError('The startup menu does not match its saved settings')
        return
    if updated == original:
        return
    # Hooks run under the upstream entry tool's operation lock. Keep a durable
    # previous file and replace atomically on the mounted EFI filesystem.
    atomic(CONFIG.with_name('limine.before-os-menu.conf'), original)
    atomic(CONFIG, updated)
    if regular(CONFIG) != updated:
        raise ValueError('Startup menu readback failed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--apply', action='store_true')
    mode.add_argument('--check', action='store_true')
    mode.add_argument('--prepare-update', action='store_true')
    args = parser.parse_args()
    try:
        apply(check=args.check, prepare_update=args.prepare_update)
    except Exception as error:
        print(f'Omarchy startup menu: {error}', file=sys.stderr)
        # The pinned upstream hook runner treats 100+ as fatal.
        return 101
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
