"""Read-only fixture handoff through the actual Docker input mounts."""
import hashlib
import json
from pathlib import Path
import sys

sys.path.insert(0, '/proof')
import builder
import product_builder

key, proof = product_builder.read_staging_input()
iso = Path('/input/omarchy-test.iso')
original_digest = builder.sha256

def digest(path):
    if path == iso:
        raise AssertionError('The construction reader rescanned the verified ISO')
    return original_digest(path)

builder.sha256 = digest
try:
    lock = {'version': 'test', 'sizeBytes': 3, 'sha256': hashlib.sha256(bytes([3, 2, 1])).hexdigest()}
    assert builder.verify_source(lock, Path('/output'), proof) == iso
    print(json.dumps({'verifiedSourceReused': True, 'individualReadOnlyMountsChecked': True, 'sourceHashPasses': 0}))
finally:
    key[:] = bytes(len(key))
