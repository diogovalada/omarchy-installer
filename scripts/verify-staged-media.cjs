// File-only packaging check, run with the staged Node executable. Never lists or
// writes devices. Exercises native-module loading and the actual SDK worker.
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const runtime = path.resolve(process.argv[2]);
assert.equal(path.dirname(process.execPath).toLowerCase(), runtime.toLowerCase());
require(path.join(runtime, 'dist/physical-engine.js'));
if (process.platform === 'win32') assert.equal(require(path.join(runtime, 'node_modules/@ronomon/direct-io')).omarchyGeometryVersion, 1);
const { physicalOpenPath } = require(path.join(runtime, 'dist/physical-path.js'));
const physicalPath = physicalOpenPath('\\\\.\\PhysicalDrive1', 'win32');
assert.equal(physicalPath, '\\\\?\\GLOBALROOT\\GLOBAL??\\PhysicalDrive1');
assert.equal(path.win32.toNamespacedPath(physicalPath), physicalPath);
const { writeVerifiedTestFile } = require(path.join(runtime, 'dist/adapter.js'));
for (const filename of Object.keys(require.cache)) {
  if (filename !== __filename) assert.ok(filename.toLowerCase().startsWith((runtime + path.sep).toLowerCase()), `Dependency escaped staged runtime: ${filename}`);
}
(async () => {
  const fixture = await fs.mkdtemp(path.join(os.tmpdir(), 'omarchy-packaging-check-'));
  try {
    const bytes = Buffer.alloc(2 * 1024 * 1024 + 73);
    for (let i = 0; i < bytes.length; i++) bytes[i] = (i * 31 + (i >>> 8)) & 255;
    const sourcePath = path.join(fixture, 'source.img');
    const destinationPath = path.join(fixture, 'destination.img');
    await fs.writeFile(sourcePath, bytes);
    const expectedSha256 = createHash('sha256').update(bytes).digest('hex');
    const receipt = await writeVerifiedTestFile({ sourcePath, destinationPath, expectedSha256 });
    assert.deepEqual(await fs.readFile(destinationPath), bytes);
    assert.equal(receipt.sha256, expectedSha256);
    assert.equal(receipt.sdkVerification, true);
    assert.equal(receipt.fullReadbackVerification, true);
    console.log(JSON.stringify({ stagedRuntime: runtime, nativeModulesLoaded: true, physicalPathNormalizationVerified: true, fileWriteVerified: true, bytes: bytes.length }));
  } finally {
    await fs.rm(path.join(fixture, 'source.img'), { force: true });
    await fs.rm(path.join(fixture, 'destination.img'), { force: true });
    await fs.rmdir(fixture);
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
