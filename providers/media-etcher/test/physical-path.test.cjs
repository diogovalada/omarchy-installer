const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { open } = require('node:fs/promises');
const { physicalOpenPath } = require('../dist/physical-path.js');

test('Windows physical disk alias survives Node namespace normalization without a trailing slash', () => {
  for (const number of [0, 1, 12, 2147483647]) {
    const raw = `\\\\.\\PhysicalDrive${number}`;
    const actual = physicalOpenPath(raw, 'win32');
    assert.equal(actual, `\\\\?\\GLOBALROOT\\GLOBAL??\\PhysicalDrive${number}`);
    assert.equal(path.win32.toNamespacedPath(actual), actual);
    assert.equal(path.win32.resolve(actual), actual);
    assert.ok(!actual.endsWith('\\'));
  }
});

test('physical disk conversion rejects volumes, paths, suffixes and alternate namespaces', () => {
  for (const raw of ['\\\\.\\PhysicalDrive1\\', '\\\\.\\PhysicalDrive1\\file', '\\\\.\\PhysicalDrive1:stream',
    '\\\\.\\PhysicalDrive01', '\\\\.\\PhysicalDrive-1', '\\\\.\\PhysicalDrive1\0', '\\\\.\\C:',
    '\\\\?\\GLOBALROOT\\GLOBAL??\\PhysicalDrive1', 'C:\\PhysicalDrive1', '/dev/sda',
    '\\\\.\\PhysicalDrive9007199254740992']) {
    assert.throws(() => physicalOpenPath(raw, 'win32'), { code: 'INVALID_DEVICE_PATH' });
  }
});

test('POSIX device paths remain exact', () => {
  assert.equal(physicalOpenPath('/dev/sda', 'linux'), '/dev/sda');
  assert.equal(physicalOpenPath('/dev/rdisk2', 'darwin'), '/dev/rdisk2');
});

test('POSIX raw opens reject partitions, aliases and alternate disk paths', () => {
  for (const raw of ['/dev/sda1', '/dev/sda/', '/dev/disk/by-id/usb-fixture', '/dev/../dev/sda', '/tmp/sda']) {
    assert.throws(() => physicalOpenPath(raw, 'linux'), { code: 'INVALID_DEVICE_PATH' });
  }
  for (const raw of ['/dev/disk2', '/dev/rdisk2s1', '/dev/rdisk02', '/dev/rdisk2/', '/tmp/rdisk2']) {
    assert.throws(() => physicalOpenPath(raw, 'darwin'), { code: 'INVALID_DEVICE_PATH' });
  }
});

test('actual Windows fs.open preserves the constructed path to a nonexistent disk', { skip: process.platform !== 'win32' }, async () => {
  const raw = physicalOpenPath('\\\\.\\PhysicalDrive2147483647');
  // Read-only, nonexistent target: exercises the C++ fs.open path conversion.
  await assert.rejects(open(raw, 'r'), error => {
    // Node strips the extended namespace prefix when formatting fs errors.
    assert.ok([raw, raw.slice(4)].includes(error.path), error.message);
    assert.ok(!error.path.endsWith('\\'));
    assert.ok(['ENOENT', 'ENXIO', 'EACCES', 'EPERM'].includes(error.code), error.message);
    return true;
  });
});
