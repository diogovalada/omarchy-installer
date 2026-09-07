const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const { constants } = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { createHash } = require('node:crypto');
const { EventEmitter } = require('node:events');
const { PassThrough, Writable } = require('node:stream');
const childProcess = require('node:child_process');
const discovery = require('../dist/physical-discovery.js');
const io = require('@ronomon/direct-io');
const mountutils = require('mountutils');
const { runPhysicalWrite } = require('../dist/physical-engine.js');
const { physicalOpenPath } = require('../dist/physical-path.js');

// Run the actual USB orchestration, BlockDevice writer and SDK verifier against
// ordinary temporary files. Every native disk boundary is intercepted; an
// unexpected path or child command fails the test instead of reaching hardware.
async function fixture(t, { length = 2 * 1024 * 1024 + 73, sector = 512, fault, alignmentUnsupported = false } = {}) {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'omarchy-physical-engine-'));
  const sourcePath = path.join(directory, 'source.img');
  const targetPath = path.join(directory, 'target.img');
  const bytes = Buffer.alloc(length);
  for (let i = 0; i < bytes.length; i++) bytes[i] = (i * 31 + (i >>> 8)) & 255;
  const span = Math.ceil(length / sector) * sector;
  const capacity = span + 4096;
  await fs.writeFile(sourcePath, bytes);
  await fs.writeFile(targetPath, Buffer.alloc(capacity, 0xa5));
  const target = { device: '\\\\.\\PhysicalDrive2147483647', raw: '\\\\.\\PhysicalDrive2147483647',
    devicePath: null, hardwareId: 'fixture-only', size: capacity, blockSize: sector,
    logicalBlockSize: 512, description: 'Temporary file USB surrogate', busType: 'USB', fingerprint: '' };
  target.fingerprint = discovery.fingerprint(target);
  const entry = { drive: { ...target, mountpoints: [], isReadOnly: false }, public: { identity: target, reasons: [], eligible: true },
    windows: { Number: 2147483647, UniqueId: 'fixture', SerialNumber: 'fixture', Path: 'fixture', Size: capacity,
      LogicalSectorSize: 512, PhysicalSectorSize: sector, BusType: 'USB', IsBoot: false, IsSystem: false,
      IsReadOnly: false, IsOffline: false } };
  const events = []; const writes = []; const order = [];
  let handle; let volumeReleased = false; let ejected = false; let modifications = 0; let currentStage;
  const realOpen = fs.open;
  t.mock.method(fs, 'open', async (filename, flags, ...rest) => {
    if (filename === sourcePath) return realOpen(filename, flags, ...rest);
    assert.equal(filename, physicalOpenPath(target.raw));
    assert.ok(flags & io.O_DIRECT); assert.ok(flags & io.O_EXLOCK);
    // Keep unbuffered aligned I/O, but the descriptor names this fresh regular
    // file. It cannot name a raw disk even if orchestration regresses.
    handle = await realOpen(targetPath, constants.O_RDWR | io.O_DIRECT | io.O_SYNC | io.O_EXLOCK);
    const write = handle.write.bind(handle); const read = handle.read.bind(handle); const close = handle.close.bind(handle);
    handle.write = async (buffer, offset, count, position) => {
      writes.push({ position, count }); order.push('write');
      if (fault === 'write') throw Object.assign(new Error('Injected disconnect'), { code: 'ENODEV' });
      return write(buffer, offset, count, position);
    };
    handle.read = async (...args) => {
      const result = await read(...args);
      if (fault === 'corrupt' && result.bytesRead) args[0][args[1]] ^= 0xff;
      if (fault === 'readback-short' && currentStage === 'readback') result.bytesRead = 0;
      return result;
    };
    if (fault === 'flush') handle.sync = async () => { throw new Error('Injected flush failure'); };
    handle.close = async () => { order.push('close'); return close(); };
    return handle;
  });
  t.mock.method(io, 'getBlockDevice', (fd, callback) => {
    assert.equal(fd, handle.fd);
    callback(null, { size: fault === 'geometry' ? capacity + 512 : capacity, logicalSectorSize: 512,
      physicalSectorSize: alignmentUnsupported ? 0 : sector, alignmentQueryError: alignmentUnsupported ? 1 : 0 });
  });
  t.mock.method(discovery, 'reidentify', async (identity, source) => {
    assert.deepEqual(identity, target); assert.equal(source, sourcePath);
    if (fault === 'identity' && handle) throw new Error('Injected target identity change');
    return entry;
  });
  t.mock.method(discovery, 'inventory', async () => ejected ? [] : [entry]);
  t.mock.method(mountutils, 'eject', (device, callback) => {
    assert.equal(device, target.device); assert.equal(handle.fd, -1); assert.equal(volumeReleased, true);
    order.push('eject');
    if (fault === 'eject') { callback(new Error('Injected ejection failure')); return; }
    ejected = true; callback(null);
  });
  t.mock.method(childProcess, 'spawn', (executable, args) => {
    assert.equal(executable, discovery.powershellPath());
    assert.ok(args.includes('2147483647'));
    const child = new EventEmitter(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
    child.stdin = new Writable({ write(_chunk, _encoding, cb) { cb(); }, final(cb) {
      volumeReleased = true; order.push('volume-release'); cb(); queueMicrotask(() => child.emit('close', 0));
    } });
    child.kill = () => { volumeReleased = true; queueMicrotask(() => child.emit('close', 1)); };
    queueMicrotask(() => child.stdout.write('{"ready":true}\n'));
    return child;
  });
  try {
    const request = { protocol: 1, action: 'write', sourcePath, length,
      sha256: createHash('sha256').update(bytes).digest('hex'), target };
    const promise = runPhysicalWrite(request, event => { events.push(event); currentStage = event.stage; }, () => { modifications++; }, error => { throw error; });
    if (fault && fault !== 'eject') {
      await assert.rejects(promise);
      assert.equal(ejected, false);
    } else {
      const receipt = await promise;
      assert.equal(receipt.bytesWritten, length); assert.equal(receipt.writeSpanBytes, span);
      assert.equal(receipt.paddingBytes, span - length); assert.equal(receipt.sha256, request.sha256);
      assert.equal(receipt.sdkVerification, true); assert.equal(receipt.fullReadbackVerification, true);
      assert.equal(receipt.eject.status, fault === 'eject' ? 'failed' : 'ejected');
      const actual = await fs.readFile(targetPath);
      assert.deepEqual(actual.subarray(0, length), bytes);
      assert.ok(actual.subarray(length, span).every(b => b === 0));
      assert.ok(actual.subarray(span).every(b => b === 0xa5));
      assert.equal(writes.at(-1).position, 0, 'Windows writes the held first buffer last');
      assert.ok(order.lastIndexOf('write') < order.indexOf('close'));
      assert.ok(order.indexOf('close') < order.indexOf('volume-release'));
      assert.ok(events.some(event => event.stage === 'readback' && event.bytes === length));
    }
    assert.equal(handle.fd, -1); assert.equal(volumeReleased, true);
    if (['geometry', 'identity'].includes(fault)) { assert.equal(modifications, 0); assert.deepEqual(writes, []); }
    else assert.ok(modifications > 0);
  } finally {
    t.mock.restoreAll();
    if (handle?.fd !== undefined && handle.fd !== -1) await handle.close();
    for (const filename of [sourcePath, targetPath]) await fs.rm(filename, { force: true });
    await fs.rmdir(directory);
  }
}

test('full Windows USB pipeline writes, pads, verifies and closes in order', { skip: process.platform !== 'win32' }, async t => {
  await fixture(t);
});
test('USB pipeline handles 4K sectors and a sub-sector image', { skip: process.platform !== 'win32' }, async t => {
  await fixture(t, { length: 73, sector: 4096 });
});
test('USB write failure releases the descriptor and volumes without ejection or success', { skip: process.platform !== 'win32', timeout: 10000 }, async t => {
  await fixture(t, { fault: 'write' });
});
test('USB verification rejects corrupted readback and releases the descriptor and volumes', { skip: process.platform !== 'win32', timeout: 10000 }, async t => {
  await fixture(t, { fault: 'corrupt' });
});
test('unsupported alignment query completes the full USB pipeline using fresh Windows geometry', { skip: process.platform !== 'win32' }, async t => {
  await fixture(t, { sector: 4096, alignmentUnsupported: true });
});
for (const fault of ['geometry', 'identity', 'flush', 'readback-short']) {
  test(`USB ${fault} failure rejects completion and closes all handles`, { skip: process.platform !== 'win32', timeout: 10000 }, async t => {
    await fixture(t, { fault });
  });
}
test('failed ejection retains a verified image receipt with a manual-removal warning', { skip: process.platform !== 'win32' }, async t => {
  await fixture(t, { fault: 'eject' });
});
