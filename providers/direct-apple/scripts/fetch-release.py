#!/usr/bin/env python3
"""Fetch and extract only pinned resources. Never run PKG scripts or install it."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
lock = json.loads((ROOT / "release-lock.json").read_text())
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--package", type=Path, help="Use an already downloaded, hash-verified PKG")
args = parser.parse_args()
cache = ROOT / ".release-input"
cache.mkdir(mode=0o700, exist_ok=True)

def verify(path, pin):
    if path.is_symlink() or not path.is_file() or path.stat().st_size != pin["sizeBytes"]:
        raise SystemExit(f"Wrong file type or size: {path.name}")
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    if digest != pin["sha256"]:
        raise SystemExit(f"Wrong SHA-256: {path.name}")

package = args.package.resolve() if args.package else cache / "upstream.pkg"
if not package.exists():
    if args.package:
        raise SystemExit("The supplied package does not exist")
    partial = cache / "upstream.pkg.part"
    with urllib.request.urlopen(lock["package"]["url"]) as response, partial.open("wb") as output:
        if not response.url.startswith("https://"):
            raise SystemExit("Non-HTTPS package redirect refused")
        remaining = lock["package"]["sizeBytes"]
        while chunk := response.read(min(1024 * 1024, remaining + 1)):
            remaining -= len(chunk)
            if remaining < 0:
                raise SystemExit("Oversized package download")
            output.write(chunk)
    verify(partial, lock["package"])
    partial.replace(package)
verify(package, lock["package"])
tar = "/usr/bin/tar" if Path("/usr/bin/tar").is_file() else shutil.which("tar")
if not tar:
    raise SystemExit("bsdtar is required to read XAR and cpio archives")
payload = cache / "Payload"
with payload.open("wb") as output:
    subprocess.run([tar, "-xOf", str(package), "component.pkg/Payload"], stdout=output, check=True)
prefix = "./Applications/Omarchy MX Mac Installer.app/Contents/Resources/"
for name, pin in lock["resources"].items():
    target = cache / "Release" / name
    target.parent.mkdir(mode=0o700, exist_ok=True)
    with target.open("wb") as output:
        subprocess.run([tar, "-xOf", str(payload), prefix + "Release/" + name], stdout=output, check=True)
    verify(target, pin)
engine = cache / lock["engine"]["fileName"]
with engine.open("wb") as output:
    subprocess.run([tar, "-xOf", str(payload), prefix + "Engine/artifacts/" + engine.name], stdout=output, check=True)
verify(engine, lock["engine"])
print("Pinned engine and sealed release resources acquired. No installer or helper was run.")
