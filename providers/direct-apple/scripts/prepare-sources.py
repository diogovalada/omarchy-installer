#!/usr/bin/env python3
"""Materialize an exact nonvisual source target; never edit the submodule."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
UPSTREAM = ROOT / "upstream"
lock = json.loads((ROOT / "source-lock.json").read_text())
head = subprocess.check_output(["git", "-C", str(UPSTREAM), "rev-parse", "HEAD"], text=True).strip()
if head != lock["commit"]:
    raise SystemExit("The Apple submodule does not match source-lock.json")
if subprocess.check_output(["git", "-C", str(UPSTREAM), "status", "--porcelain"], text=True).strip():
    raise SystemExit("The Apple submodule must be clean before materializing build sources")

destination = ROOT / ".build-input" / "Sources"
destination.mkdir(parents=True, exist_ok=True)
sources = {path.name: path for path in (ROOT / "Sources").glob("*.swift")}
app_sources = UPSTREAM / "apps/omarchy-apple-installer/Sources/OmarchyAppleInstallerApp"
for name in lock["appSourceFiles"]:
    sources[name] = app_sources / name
if {path.name for path in destination.iterdir()} - sources.keys():
    raise SystemExit("Unexpected old source in .build-input/Sources; inspect it before rebuilding")
manifest = {"sourceCommit": head, "model": lock["model"], "files": {}}
for name, source in sorted(sources.items()):
    data = source.read_bytes()
    (destination / name).write_bytes(data)
    manifest["files"][name] = hashlib.sha256(data).hexdigest()
(ROOT / ".build-input" / "provenance.json").write_text(json.dumps(manifest, indent=2) + "\n")
print("Prepared pinned Apple bridge sources; no build or installation was run.")
