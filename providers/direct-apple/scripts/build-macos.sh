#!/bin/bash
set -euo pipefail

bridge_root=$(cd "$(dirname "$0")/.." && pwd)
[[ $(uname -s) == "Darwin" && $(uname -m) == "arm64" ]] || {
  echo "The native bridge build requires Apple Silicon macOS and Swift 6.2." >&2
  exit 1
}
python3 "$bridge_root/scripts/prepare-sources.py"
swift build --package-path "$bridge_root" --configuration release --arch arm64 --product omarchy-apple-bridge
swift build --package-path "$bridge_root/upstream/apps/omarchy-apple-installer" \
  --configuration release --arch arm64 --product OmarchyAppleInstallerHelper
echo "Built companion and helper. Neither was signed, installed, registered, or executed."
