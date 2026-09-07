const { test } = require('node:test');
const assert = require('node:assert/strict');
const { resolveHandleGeometry } = require('../dist/physical-geometry.js');
const disk = { Size: 8015314944, LogicalSectorSize: 512, PhysicalSectorSize: 512,
  BusType: 'USB', IsBoot: false, IsSystem: false, IsOffline: false, IsReadOnly: false };
const unsupported = { size: disk.Size, logicalSectorSize: 512, physicalSectorSize: 0, alignmentQueryError: 1 };

test('unsupported USB query uses fresh Windows physical size while retaining handle geometry', () => {
  for (const error of [1, 50]) for (const sector of [512, 4096]) {
    const resolved = resolveHandleGeometry({ ...unsupported, alignmentQueryError: error }, { ...disk, PhysicalSectorSize: sector });
    assert.equal(resolved.physicalSectorSize, sector);
    assert.equal(resolved.logicalSectorSize, 512);
    assert.equal(resolved.size, disk.Size);
  }
});

test('missing, changed, unsafe or invalid Windows geometry never enables fallback', () => {
  const bad = [undefined, { ...disk, Size: disk.Size + 512 }, { ...disk, LogicalSectorSize: 4096 },
    { ...disk, PhysicalSectorSize: 0 }, { ...disk, PhysicalSectorSize: 1234 }, { ...disk, BusType: 'NVMe' },
    ...['IsBoot', 'IsSystem', 'IsOffline', 'IsReadOnly'].map(key => ({ ...disk, [key]: true }))];
  for (const windows of bad) assert.throws(() => resolveHandleGeometry(unsupported, windows), { code: 'HANDLE_GEOMETRY_UNAVAILABLE' });
});

test('permission, disconnect, malformed request and I/O errors do not qualify for fallback', () => {
  for (const error of [5, 21, 31, 87, 1167]) {
    assert.throws(() => resolveHandleGeometry({ ...unsupported, alignmentQueryError: error }, disk), { code: 'HANDLE_GEOMETRY_UNAVAILABLE' });
  }
  assert.throws(() => resolveHandleGeometry({ ...unsupported, physicalSectorSize: 4096 }, disk), { code: 'HANDLE_GEOMETRY_UNAVAILABLE' });
});

test('valid native geometry is retained and never overwritten by OS fallback', () => {
  const native = { size: disk.Size, logicalSectorSize: 512, physicalSectorSize: 4096 };
  assert.deepEqual(resolveHandleGeometry(native, disk), native);
  assert.deepEqual(resolveHandleGeometry(native), native);
  for (const patch of [{ size: 0 }, { size: disk.Size + 1 }, { logicalSectorSize: 0 },
    { physicalSectorSize: 0 }, { logicalSectorSize: 4096, physicalSectorSize: 512 }]) {
    assert.throws(() => resolveHandleGeometry({ ...native, ...patch }, disk), { code: 'HANDLE_GEOMETRY_INVALID' });
  }
});
