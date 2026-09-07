const fs = require('node:fs/promises');
const path = require('node:path');
const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const { writeVerifiedTestFile } = require('./dist/adapter.js');
(async () => {
  const bytes = Buffer.alloc(1024 * 1024 + 123, 0xa7);
  const expectedSha256 = createHash('sha256').update(bytes).digest('hex');
  const sourcePath = path.join(__dirname, 'packaging-source.img');
  const destinationPath = path.join(__dirname, 'packaging-output.img');
  await fs.writeFile(sourcePath, bytes, { flag: 'wx' });
  const receipt = await writeVerifiedTestFile({ sourcePath, destinationPath, expectedSha256 });
  assert.deepEqual(await fs.readFile(destinationPath), bytes);
  console.log(JSON.stringify({ check: 'relocated-runtime-file-write', node: process.version,
    platform: process.platform, arch: process.arch, model: 'gpt-6-astra', receipt }, null, 2));
})().catch(error => { console.error(error); process.exitCode = 1; });
