#!/usr/bin/env python3
"""Assemble an unsigned companion with pinned resources and explicit identities.

Does not install, sign, notarize, register launchd, or execute either binary.
"""
import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import re
import shutil

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--team-id", required=True, help="Actual Apple signing Team ID for this distribution")
parser.add_argument("--parent-bundle-id", default="community.omarchy.setup")
parser.add_argument("--parent-app-path", default="/Applications/Omarchy Installer.app")
parser.add_argument("--bridge-binary", type=Path, default=ROOT / ".build/release/omarchy-apple-bridge")
parser.add_argument("--helper-binary", type=Path,
                    default=ROOT / "upstream/apps/omarchy-apple-installer/.build/release/OmarchyAppleInstallerHelper")
args = parser.parse_args()
if not re.fullmatch(r"[A-Z0-9]{10}", args.team_id):
    raise SystemExit("Supply the actual ten-character Apple Team ID")
if not re.fullmatch(r"[A-Za-z0-9]+(?:[.-][A-Za-z0-9]+)+", args.parent_bundle_id):
    raise SystemExit("Invalid parent bundle identifier")
parent = Path(args.parent_app_path)
if not parent.is_absolute() or parent.suffix != ".app" or ".." in parent.parts:
    raise SystemExit("The installed parent path must be an absolute .app path")
for binary in [args.bridge_binary, args.helper_binary]:
    if binary.is_symlink() or not binary.is_file():
        raise SystemExit(f"Missing regular compiled binary: {binary.name}")
    # A source file, Windows binary, or archive cannot accidentally become a helper.
    with binary.open("rb") as stream:
        if stream.read(4) not in [bytes.fromhex("cffaedfe"), bytes.fromhex("feedfacf")]:
            raise SystemExit(f"Expected a compiled 64-bit Mach-O executable: {binary.name}")

release = json.loads((ROOT / "release-lock.json").read_text())
source = json.loads((ROOT / "source-lock.json").read_text())
inputs = ROOT / ".release-input"

def verify(path, pin):
    if path.is_symlink() or not path.is_file() or path.stat().st_size != pin["sizeBytes"]:
        raise SystemExit(f"Pinned resource type or size mismatch: {path.name}")
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    if digest != pin["sha256"]:
        raise SystemExit(f"Pinned resource SHA-256 mismatch: {path.name}")

for name, pin in release["resources"].items():
    verify(inputs / "Release" / name, pin)
verify(inputs / release["engine"]["fileName"], release["engine"])
build_manifest = ROOT / ".build-input/provenance.json"
if not build_manifest.is_file():
    raise SystemExit("Run prepare-sources.py and build the native binaries first")

destination = ROOT / "dist" / "Omarchy Apple Bridge.app"
if destination.exists():
    raise SystemExit("The output bundle already exists; preserve or remove that exact disposable output before reassembling")
contents = destination / "Contents"
resources = contents / "Resources"
(contents / "MacOS").mkdir(parents=True)
(resources / "Release").mkdir(parents=True)
(resources / "Engine/artifacts").mkdir(parents=True)
(resources / "Notices").mkdir(parents=True)
shutil.copyfile(ROOT / "upstream/LICENSE", resources / "Notices/omarchy-mx-mac-LICENSE.txt")

bridge_id = "community.omarchy.setup.apple-bridge"
helper_id = "com.omarchy.mx.installer.helper"

def requirement(identifier):
    return f'anchor apple generic and identifier "{identifier}" and certificate leaf[subject.OU] = "{args.team_id}"'

for name in release["resources"]:
    shutil.copyfile(inputs / "Release" / name, resources / "Release" / name)
descriptor_path = resources / "Release/release.json"
descriptor = json.loads(descriptor_path.read_text())
descriptor["helper_code_signing_requirement"] = requirement(helper_id)
descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n")
shutil.copyfile(inputs / release["engine"]["fileName"], resources / "Engine/artifacts" / release["engine"]["fileName"])
shutil.copyfile(args.bridge_binary, contents / "MacOS/omarchy-apple-bridge")
shutil.copyfile(args.helper_binary, resources / "omarchy-apple-installer-helper")
(contents / "MacOS/omarchy-apple-bridge").chmod(0o755)
(resources / "omarchy-apple-installer-helper").chmod(0o755)
admission = {
    "schemaVersion": 1, "sourceCommit": source["commit"],
    "parentCodeSigningRequirement": requirement(args.parent_bundle_id),
}
(resources / "bridge-admission.json").write_text(json.dumps(admission, indent=2) + "\n")
info = {
    "CFBundleExecutable": "omarchy-apple-bridge", "CFBundleIdentifier": bridge_id,
    "CFBundleName": "Omarchy Apple Bridge", "CFBundleDisplayName": "Omarchy Apple Bridge",
    "CFBundlePackageType": "APPL", "CFBundleInfoDictionaryVersion": "6.0",
    "CFBundleShortVersionString": "0.1.0", "CFBundleVersion": "1",
    "LSMinimumSystemVersion": "15.0", "LSUIElement": True,
}
(contents / "Info.plist").write_bytes(plistlib.dumps(info))
installed_bridge = parent / "Contents/Resources/direct-apple/Omarchy Apple Bridge.app"
daemon = {
    "Label": helper_id,
    "Program": str(installed_bridge / "Contents/Resources/omarchy-apple-installer-helper"),
    "MachServices": {helper_id: True}, "UserName": "root", "KeepAlive": False,
    "EnvironmentVariables": {"OMARCHY_CLIENT_CODE_SIGNING_REQUIREMENT": requirement(bridge_id)},
}
(ROOT / "dist" / (helper_id + ".plist")).write_bytes(plistlib.dumps(daemon))
provenance = {
    "source": source, "release": release, "model": source["model"],
    "buildInputs": json.loads(build_manifest.read_text()),
    "parentBundleIdentifier": args.parent_bundle_id, "teamIdentifier": args.team_id,
    "bridgeRequirement": requirement(bridge_id), "helperRequirement": requirement(helper_id),
    "binarySha256": {
        "bridge": hashlib.sha256(args.bridge_binary.read_bytes()).hexdigest(),
        "helper": hashlib.sha256(args.helper_binary.read_bytes()).hexdigest(),
    },
    "upstreamCatalogUnchanged": True, "signed": False, "installed": False,
    "qualification": "macOS build and physical acceptance must be recorded separately",
}
(ROOT / "dist/provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
print(f"Assembled unsigned native companion: {destination}")
print("The helper daemon plist is staged separately; it has not been registered or installed.")
