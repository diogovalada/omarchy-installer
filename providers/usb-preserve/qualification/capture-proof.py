"""Capture a running isolated VM only after observing a mounted live root and login."""
import hashlib
import json
import pathlib
import socket

out=pathlib.Path('/output')
log=(out/'serial.log').read_text(errors='replace')
assert 'archiso login:' in log
assert "'/run/archiso/airootfs'" in log
assert 'OMARCHY_ISO_FOUND_ON_EXISTING_NTFS' in log
with socket.socket(socket.AF_UNIX) as sock:
    sock.connect('/tmp/usb-qmp.sock')
    stream=sock.makefile('rwb',buffering=0)
    stream.readline()
    for command in [{'execute':'qmp_capabilities'},{'execute':'screendump','arguments':{'filename':str(out/'boot.png'),'format':'png'}}]:
        stream.write((json.dumps(command)+'\n').encode())
        while True:
            response=json.loads(stream.readline())
            if 'return' in response: break
            if 'error' in response: raise RuntimeError(response)
result={'passed':True,'profile':'GPT, NTFS data + FAT32 EFI; x64 UEFI; Secure Boot off',
        'evidence':'Official ISO loopback mounted; live root mounted; Omarchy serial login reached',
        'isoSha256':'2ef8e624aa1bec7e277e28056b8535a6c9373ba48d7ede3f1a01cb6d2373cfb8',
        'loaderSha256':hashlib.sha256(pathlib.Path('/loader/BOOTX64.EFI').read_bytes()).hexdigest(),
        'physicalHardwareQualified':False}
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result),flush=True)
