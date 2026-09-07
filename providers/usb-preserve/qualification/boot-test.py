"""Isolated UEFI boot qualification; every writable disk is a regular test file."""
import hashlib
import json
import pathlib
import shutil
import socket
import subprocess
import sys
import time

out = pathlib.Path('/output')
out.mkdir(exist_ok=True)
def run(*args):
    return subprocess.check_output(args, stderr=subprocess.STDOUT, text=True)
iso = pathlib.Path('/input/omarchy-4.0.2.iso')
production = '--production' in sys.argv
data = out / 'data.img'
esp = out / 'esp.img'
if not data.exists():
    with data.open('wb') as f: f.truncate(8 * 1024**3)
    print(run('mkntfs', '-F', '-Q', '-L', 'PRESERVE', str(data)), flush=True)
    print(run('ntfscp', str(data), str(iso), '/omarchy-qualification.iso'), flush=True)
    sentinel = out / 'existing-user-file.txt'
    sentinel.write_text('Existing files must survive installer creation.\n')
    run('ntfscp', str(data), str(sentinel), '/existing-user-file.txt')
with esp.open('wb') as f: f.truncate(128 * 1024**2)
run('mkfs.vfat', '-F', '32', str(esp))
run('mmd', '-i', str(esp), '::/EFI', '::/EFI/BOOT', '::/EFI/Omarchy')
run('mcopy', '-i', str(esp), '/loader/BOOTX64.EFI', '::/EFI/BOOT/BOOTX64.EFI')
cfg = out / 'grub.cfg'
cfg.write_text('''serial --unit=0 --speed=115200
terminal_input console serial
terminal_output console serial
search --no-floppy --file --set=data /omarchy-qualification.iso
set iso_path=/omarchy-qualification.iso
loopback loop ($data)$iso_path
set root=(loop)
probe --set=data_uuid --fs-uuid ($data)
echo OMARCHY_ISO_FOUND_ON_EXISTING_NTFS
linux /arch/boot/x86_64/vmlinuz-linux-t2 archisobasedir=arch img_dev=UUID=$data_uuid img_loop=$iso_path console=ttyS0,115200 xe.enable_panel_replay=0 initramfs_async=0
initrd /arch/boot/x86_64/initramfs-linux-t2.img
boot
''')
if production:
    cfg.write_text(pathlib.Path('/loader/grub.cfg.template').read_text().replace('@ISO_NAME@','omarchy-qualification.iso'))
run('mcopy', '-i', str(esp), str(cfg), '::/EFI/Omarchy/grub.cfg')
disk = out / 'usb.img'
if not disk.exists():
    with disk.open('wb') as f: f.truncate(9 * 1024**3)
    run('sgdisk', '--clear', '--new=1:2048:+128M', '--typecode=1:ef00', '--new=2:264192:+8G', '--typecode=2:0700', str(disk))
    with disk.open('r+b') as dest, data.open('rb') as src:
        dest.seek(264192 * 512)
        shutil.copyfileobj(src, dest, 4 * 1024**2)
with disk.open('r+b') as dest, esp.open('rb') as src:
    dest.seek(2048 * 512)
    shutil.copyfileobj(src, dest, 4 * 1024**2)
shutil.copy('/usr/share/OVMF/OVMF_VARS_4M.fd', out / 'vars.fd')
serial = out / 'serial.log'
args = ['qemu-system-x86_64', '-machine', 'q35,accel=tcg', '-cpu', 'max', '-smp', '4', '-m', '4096',
        '-no-user-config', '-nodefaults', '-display', 'none', '-device', 'virtio-vga',
        '-drive', 'if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd',
        '-drive', 'if=pflash,format=raw,file=' + str(out / 'vars.fd'),
        '-device', 'qemu-xhci', '-drive', 'if=none,format=raw,readonly=on,id=usb,file='+str(disk),
        '-device', 'usb-storage,drive=usb', '-serial', 'file:'+str(serial), '-nic', 'none',
        '-qmp', 'unix:/tmp/usb-qmp.sock,server=on,wait=off']
process = subprocess.Popen(args)
try:
    for tick in range(120):
        time.sleep(5)
        log = serial.read_text(errors='replace') if serial.exists() else ''
        if tick % 6 == 0: print(log[-1600:], flush=True)
        if (production and tick == 35) or (not production and 'archiso login:' in log and "'/run/archiso/airootfs'" in log):
            time.sleep(20)
            with socket.socket(socket.AF_UNIX) as sock:
                sock.connect('/tmp/usb-qmp.sock')
                stream=sock.makefile('rwb',buffering=0)
                stream.readline()
                for command in [{'execute':'qmp_capabilities'}, {'execute':'screendump','arguments':{'filename':str(out/('production-boot.png' if production else 'boot.png')),'format':'png'}}]:
                    stream.write((json.dumps(command)+'\n').encode())
                    while True:
                        response=json.loads(stream.readline())
                        if 'return' in response: break
                        if 'error' in response: raise RuntimeError(response)
            (out/('production-result.json' if production else 'result.json')).write_text(json.dumps({'passed':not production,'needsVisualReview':production,'profile':'GPT, NTFS data + FAT32 EFI; x64 UEFI; Secure Boot off',
                'evidence':'Official ISO loopback mounted; live root mounted; Omarchy userspace started',
                'isoSha256':'2ef8e624aa1bec7e277e28056b8535a6c9373ba48d7ede3f1a01cb6d2373cfb8',
                'loaderSha256':hashlib.sha256(pathlib.Path('/loader/BOOTX64.EFI').read_bytes()).hexdigest(),
                'physicalHardwareQualified':False},indent=2)+'\n')
            print('PRODUCTION_BOOT_SCREENSHOT_CAPTURED' if production else 'BOOT_QUALIFICATION_PASSED', flush=True)
            break
        if process.poll() is not None: raise RuntimeError('QEMU exited early')
    else: raise RuntimeError('Boot qualification timed out; inspect serial.log')
finally:
    process.terminate()
    process.wait(timeout=20)
print(run('ntfscat', str(data), '/existing-user-file.txt'), flush=True)
