const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const { createHash } = require('node:crypto');
const childProcess = require('node:child_process');
const { writeVerifiedTestFile } = require('../dist/adapter.js');
const { validatePathSyntax } = require('../dist/safety.js');

const hash = bytes => createHash('sha256').update(bytes).digest('hex');
async function fixture(t, size = 2 * 1024 * 1024 + 73) {
  const dir = await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(), 'omarchy-etcher-test-')));
  t.after(() => fs.rm(dir, { recursive: true, force: true }));
  const bytes = Buffer.alloc(size);
  for (let i = 0; i < bytes.length; i++) bytes[i] = (i * 31 + (i >>> 8)) & 255;
  const sourcePath = path.join(dir, 'source.img');
  const destinationPath = path.join(dir, 'destination.img');
  await fs.writeFile(sourcePath, bytes);
  return { dir, bytes, request: { sourcePath, destinationPath, expectedSha256: hash(bytes) } };
}
const rejectsCode = (promise, code) => assert.rejects(promise, error => error.code === code);

test('extra unserializable caller metadata is omitted from IPC and worker closes', { timeout: 30000 }, async t => {
  const { request, bytes } = await fixture(t);
  const originalSpawn = childProcess.spawn;
  let worker;
  let closed = false;
  t.mock.method(childProcess, 'spawn', (...args) => {
    worker = originalSpawn(...args);
    worker.once('close', () => { closed = true; });
    return worker;
  });
  t.after(() => { if (worker && !closed) worker.kill(); });
  const extra = { ...request, metadata: 1n, toJSON() { throw new Error('Must not serialize caller object'); } };
  extra.circular = extra;
  const receipt = await writeVerifiedTestFile(extra);
  assert.equal(receipt.sha256, request.expectedSha256);
  assert.deepEqual(await fs.readFile(request.destinationPath), bytes);
  assert.equal(closed, true);
  assert.equal(worker.connected, false);
});

test('synchronous IPC send failure terminates worker before terminal error', { timeout: 30000 }, async t => {
  const { request } = await fixture(t);
  const originalSpawn = childProcess.spawn;
  let worker;
  let closed = false;
  const terminalStates = [];
  t.mock.method(childProcess, 'spawn', (...args) => {
    worker = originalSpawn(...args);
    worker.once('close', () => { closed = true; });
    t.mock.method(worker, 'send', () => { throw new TypeError('Injected synchronous serialization failure'); });
    return worker;
  });
  t.after(() => { if (worker && !closed) worker.kill(); });
  await rejectsCode(writeVerifiedTestFile(request, { onEvent: event => {
    if (['error', 'completed', 'cancelled'].includes(event.type)) terminalStates.push({ type: event.type, closed });
  } }), 'WORKER_IPC');
  assert.deepEqual(terminalStates, [{ type: 'error', closed: true }]);
  assert.equal(worker.connected, false);
  await assert.rejects(fs.stat(request.destinationPath), { code: 'ENOENT' });
});

test('actual SDK writes exact non-aligned bytes, verifies, and produces a receipt', { timeout: 30000 }, async t => {
  const { request, bytes } = await fixture(t);
  const events = [];
  const receipt = await writeVerifiedTestFile(request, { onEvent: event => events.push(event) });
  assert.deepEqual(await fs.readFile(request.destinationPath), bytes);
  assert.equal(receipt.bytesWritten, bytes.length);
  assert.equal(receipt.sha256, request.expectedSha256);
  assert.equal(receipt.engineVersion, '10.2.14');
  assert.equal(receipt.sdkVerification, true);
  assert.equal(receipt.fullReadbackVerification, true);
  assert.ok(events.some(e => e.type === 'writing'));
  assert.ok(events.some(e => e.type === 'verifying'));
  assert.ok(events.some(e => e.type === 'readback' && e.bytes === bytes.length));
  assert.equal(events.at(-1).type, 'completed');
});

test('source digest mismatch creates no destination', async t => {
  const { request } = await fixture(t);
  await rejectsCode(writeVerifiedTestFile({ ...request, expectedSha256: '0'.repeat(64) }), 'SOURCE_DIGEST_MISMATCH');
  await assert.rejects(fs.stat(request.destinationPath), { code: 'ENOENT' });
});

test('existing outputs and hard links to the source are preserved', async t => {
  const { request, bytes } = await fixture(t);
  await fs.writeFile(request.destinationPath, 'preserve me');
  await rejectsCode(writeVerifiedTestFile(request), 'DESTINATION_EXISTS');
  assert.equal(await fs.readFile(request.destinationPath, 'utf8'), 'preserve me');
  const linked = path.join(path.dirname(request.sourcePath), 'hardlink.img');
  await fs.link(request.sourcePath, linked);
  await rejectsCode(writeVerifiedTestFile({ ...request, destinationPath: linked }), 'DESTINATION_EXISTS');
  assert.deepEqual(await fs.readFile(request.sourcePath), bytes);
  await rejectsCode(writeVerifiedTestFile({ ...request, destinationPath: request.sourcePath }), 'SOURCE_DESTINATION_ALIAS');
});

test('device namespaces, pseudo-files, URLs, reserved names and ambiguous paths are rejected lexically', () => {
  for (const unsafe of ['\\\\.\\PhysicalDrive0', '\\\\?\\GLOBALROOT\\Device\\Harddisk0',
    '\\??\\C:\\disk.img', '\\\\server\\share\\disk.img', '/dev/sda', '/dev/rdisk0', '/proc/self/mem', '/sys/kernel/x',
    'file:///tmp/image', 'https://example.com/disk.img', 'C:disk.img', 'relative.img',
    'C:\\temp\\NUL.img', 'C:\\temp\\CON', 'C:\\temp\\COM1', 'C:\\temp\\disk.img:stream',
    'C:\\temp\\..\\disk.img', 'C:\\temp\\disk.img.']) {
    assert.throws(() => validatePathSyntax(unsafe), { code: 'UNSAFE_PATH' }, unsafe);
  }
});

test('Windows ordinary paths require an explicit drive', { skip: process.platform !== 'win32' }, () => {
  for (const unsafe of ['\\fixtures\\new.img', '/fixtures/new.img']) {
    assert.throws(() => validatePathSyntax(unsafe), { code: 'UNSAFE_PATH' }, unsafe);
  }
  assert.doesNotThrow(() => validatePathSyntax('C:\\fixtures\\new.img'));
  assert.doesNotThrow(() => validatePathSyntax('C:/fixtures/new.img'));
});

test('directory junctions/symlinks are rejected before output creation', async t => {
  const { dir, request } = await fixture(t);
  const actual = path.join(dir, 'actual');
  const linked = path.join(dir, 'linked');
  await fs.mkdir(actual);
  await fs.symlink(actual, linked, process.platform === 'win32' ? 'junction' : 'dir');
  await rejectsCode(writeVerifiedTestFile({ ...request, destinationPath: path.join(linked, 'output.img') }), 'SYMLINK_REJECTED');
  assert.deepEqual(await fs.readdir(actual), []);
  await fs.copyFile(request.sourcePath, path.join(actual, 'source.img'));
  await rejectsCode(writeVerifiedTestFile({ ...request, sourcePath: path.join(linked, 'source.img') }), 'SYMLINK_REJECTED');
});

test('already-cancelled request creates no destination', async t => {
  const { request } = await fixture(t);
  const controller = new AbortController();
  controller.abort();
  await rejectsCode(writeVerifiedTestFile(request, { signal: controller.signal }), 'CANCELLED');
  await assert.rejects(fs.stat(request.destinationPath), { code: 'ENOENT' });
});

test('in-flight SDK cancellation waits for worker exit and releases the output handle', { timeout: 30000 }, async t => {
  const { request, bytes } = await fixture(t, 64 * 1024 * 1024 + 73);
  const controller = new AbortController();
  const events = [];
  await rejectsCode(writeVerifiedTestFile(request, { signal: controller.signal, onEvent: event => {
    events.push(event);
    if (event.type === 'writing' && event.bytes > 0) controller.abort();
  } }), 'CANCELLED');
  assert.ok(events.some(e => e.type === 'writing' && e.bytes > 0));
  assert.equal(events.at(-1).type, 'cancelled');
  assert.ok(!events.some(e => e.type === 'completed'));
  assert.equal(hash(await fs.readFile(request.sourcePath)), hash(bytes));
  await fs.rename(request.destinationPath, request.destinationPath + '.cancelled');
});

test('missing source and destination parent produce honest errors without success', async t => {
  const { request, dir } = await fixture(t);
  const events = [];
  await rejectsCode(writeVerifiedTestFile({ ...request, sourcePath: path.join(dir, 'missing.img') },
    { onEvent: e => events.push(e) }), 'ENOENT');
  assert.equal(events.at(-1).type, 'error');
  await rejectsCode(writeVerifiedTestFile({ ...request, destinationPath: path.join(dir, 'missing', 'output.img') }), 'ENOENT');
});

test('zero length source is rejected', async t => {
  const { request } = await fixture(t, 0);
  await rejectsCode(writeVerifiedTestFile(request), 'INVALID_SOURCE');
});

test('full readback detects output corruption and suppresses success', { timeout: 30000 }, async t => {
  const { request } = await fixture(t, 32 * 1024 * 1024);
  const fsSync = require('node:fs');
  const { runFileWrite } = require('../dist/engine.js');
  let corrupted = false;
  const events = [];
  // Call the worker-internal engine directly for deterministic fault injection:
  // this callback completes before its next read, without depending on IPC timing.
  await rejectsCode(runFileWrite(request, event => {
    events.push(event);
    if (!corrupted && event.type === 'readback' && event.bytes === 0) {
      const fd = fsSync.openSync(request.destinationPath, 'r+');
      fsSync.writeSync(fd, Buffer.from([255]), 0, 1, 31 * 1024 * 1024);
      fsSync.closeSync(fd);
      corrupted = true;
    }
  }), 'READBACK_MISMATCH');
  assert.ok(corrupted);
  assert.ok(!events.some(e => e.type === 'completed'));
});
