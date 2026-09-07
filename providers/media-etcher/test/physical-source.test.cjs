const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { openStableSource, verifyPhysicalSource } = require('../dist/physical-source.js');

async function fixture(t) {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'omarchy-source-'));
  const sourcePath = path.join(directory, 'source.iso');
  const bytes = Buffer.alloc(1024 * 1024 + 73, 0x5a);
  await fs.writeFile(sourcePath, bytes);
  const request = { sourcePath, length: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
  const source = await openStableSource(request);
  t.after(async () => { await source.handle.close(); await fs.rm(directory, { recursive: true, force: true }); });
  return { source, request, directory, bytes };
}
function binding(source) {
  return { kind: 'windows-held-iso-v1', parentPid: process.ppid,
    volumeSerial: String(source.stat.dev & 0xffffffffn), fileIndex: String(source.stat.ino) };
}

test('without a parent handoff each verification reads the complete image and rejects a bad digest', async t => {
  const { source, request } = await fixture(t);
  for (let pass = 0; pass < 2; pass++) {
    const progress = [];
    await verifyPhysicalSource(source, request, bytes => progress.push(bytes));
    assert.equal(progress[0], 0);
    assert.equal(progress.at(-1), request.length);
  }
  await assert.rejects(verifyPhysicalSource(source, { ...request, sha256: '0'.repeat(64) }, () => {}), { code: 'SOURCE_DIGEST' });
});

test('a current Windows parent binding uses identity checks without reading the ISO again', { skip: process.platform !== 'win32' }, async t => {
  const { source, request } = await fixture(t);
  const handoff = { ...request, sourceVerification: binding(source) };
  t.mock.method(source.handle, 'read', () => { throw new Error('Unexpected duplicate full-source scan'); });
  const unexpectedProgress = () => { throw new Error('No hashing stage should be emitted for a held source'); };
  await verifyPhysicalSource(source, handoff, unexpectedProgress);
  await verifyPhysicalSource(source, handoff, unexpectedProgress);
});

test('a handoff cannot be reused for a different file with identical bytes', { skip: process.platform !== 'win32' }, async t => {
  const { source, request, directory, bytes } = await fixture(t);
  const otherPath = path.join(directory, 'other.iso');
  await fs.writeFile(otherPath, bytes);
  const other = await openStableSource({ ...request, sourcePath: otherPath });
  try {
    await assert.rejects(verifyPhysicalSource(other, { ...request, sourceVerification: binding(source) }, () => {}), { code: 'SOURCE_IDENTITY_CHANGED' });
  } finally { await other.handle.close(); }
});

test('invalid handoffs fail closed instead of enabling a hash bypass', async t => {
  const { source, request } = await fixture(t);
  const good = binding(source);
  for (const invalid of [null, true, {}, { ...good, parentPid: 0 }, { ...good, parentPid: process.pid },
    { ...good, parentPid: String(process.ppid) }, { ...good, kind: 'cached-verified' },
    { ...good, fileIndex: Number(good.fileIndex) }, { ...good, fileIndex: '0' },
    { ...good, fileIndex: '-1' }, { ...good, volumeSerial: '01' }, { ...good, extra: true }]) {
    await assert.rejects(verifyPhysicalSource(source, { ...request, sourceVerification: invalid }, () => {}), { code: 'INVALID_SOURCE_BINDING' });
  }
  if (process.platform !== 'win32') {
    await assert.rejects(verifyPhysicalSource(source, { ...request, sourceVerification: good }, () => {}), { code: 'INVALID_SOURCE_BINDING' });
  }
});

test('file identities are compared as 64-bit integers, without number rounding', { skip: process.platform !== 'win32' }, async t => {
  const { source, request } = await fixture(t);
  const exactIndex = 9007199254740993n;
  const stat = { ...source.stat, ino: exactIndex };
  const synthetic = { stat, handle: { stat: async () => stat } };
  const proof = { ...binding(source), fileIndex: String(exactIndex) };
  await verifyPhysicalSource(synthetic, { ...request, sourceVerification: proof }, () => {});
  await assert.rejects(verifyPhysicalSource(synthetic, { ...request,
    sourceVerification: { ...proof, fileIndex: String(Number(exactIndex)) } }, () => {}), { code: 'SOURCE_IDENTITY_CHANGED' });
});

test('source changes and loss of the retaining parent prevent successful reuse', { skip: process.platform !== 'win32' }, async t => {
  const { source, request } = await fixture(t);
  const handoff = { ...request, sourceVerification: binding(source) };
  const probe = t.mock.method(process, 'kill', () => { throw Object.assign(new Error('parent gone'), { code: 'ESRCH' }); });
  await assert.rejects(verifyPhysicalSource(source, handoff, () => {}), { code: 'SOURCE_PARENT_LOST' });
  probe.mock.restore();
  // Unit fixture has no retaining native parent. Real Windows lock protection
  // and the cross-language file identity are covered by the native guard tests.
  await fs.utimes(request.sourcePath, new Date(), new Date(Date.now() + 2000));
  await assert.rejects(verifyPhysicalSource(source, handoff, () => {}), { code: 'SOURCE_CHANGED' });
});
