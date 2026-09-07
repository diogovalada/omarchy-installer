const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const { spawn } = require('node:child_process');
const { constants } = require('node:fs');
const io = require('@ronomon/direct-io');

// Exercise the real CLI's cancellation/lifetime protocol. Its engine dependency
// is replaced before loading with a file-only slow writer, so even malformed
// test requests cannot reach device discovery, elevation, or raw-device I/O.
const bootstrap = `
const fs = require('node:fs/promises');
const Module = require('node:module');
const path = require('node:path');
const [enginePath, cliPath, output] = process.argv.slice(2);
const engine = new Module(enginePath); engine.filename = enginePath; engine.loaded = true;
engine.exports.runPhysicalWrite = async (_request, emit, modified) => {
  const handle = await fs.open(output, 'wx');
  const bytes = Buffer.alloc(4096, 0x5a);
  for (let position = 0; ; position += bytes.length) {
    modified(); await handle.write(bytes, 0, bytes.length, position);
    emit({protocol:1,type:'event',stage:'writing',bytes:position+bytes.length,totalBytes:1048576});
    await new Promise(resolve => setTimeout(resolve, 100));
  }
};
require.cache[enginePath] = engine;
require(cliPath);
`;

for (const method of ['abort', 'parent-pipe-close']) {
  test(`physical CLI ${method} terminates the write before returning cancellation`, { timeout: 10000 }, async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'omarchy-cli-lifetime-'));
    const script = path.join(directory, 'bootstrap.cjs');
    const output = path.join(directory, 'output.img');
    await fs.writeFile(script, bootstrap);
    const child = spawn(process.execPath, [script, path.resolve(__dirname, '../dist/physical-engine.js'),
      path.resolve(__dirname, '../dist/physical-cli.js'), output], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    const events = []; let pending = ''; let stopped = false; let stderr = ''; let closed = false;
    child.once('close', () => { closed = true; });
    const timeout = setTimeout(() => child.kill(), 5000);
    child.stderr.on('data', chunk => { stderr += chunk; });
    child.stdin.on('error', () => {});
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', chunk => {
      pending += chunk;
      let newline;
      while ((newline = pending.indexOf('\n')) >= 0) {
        const event = JSON.parse(pending.slice(0, newline)); pending = pending.slice(newline + 1);
        events.push(event);
        if (!stopped && event.type === 'event' && event.stage === 'writing') {
          stopped = true;
          if (method === 'abort') child.stdin.write('{"protocol":1,"action":"abort"}\n');
          else child.stdin.end();
        }
      }
    });
    try {
      child.stdin.write('{"protocol":1,"action":"write","sourcePath":"fixture","length":1048576,"sha256":"fixture","target":{}}\n');
      const code = await new Promise((resolve, reject) => { child.once('close', resolve); child.once('error', reject); });
      assert.equal(code, 2, stderr);
      assert.equal(stopped, true); assert.equal(events.some(event => event.type === 'result'), false);
      const error = events.find(event => event.type === 'error');
      assert.equal(error?.code, 'CANCELLED'); assert.equal(error?.possiblyModified, true);
      // Process exit must release its file descriptor, not merely send a frame.
      const handle = await fs.open(output, constants.O_RDWR | (process.platform === 'win32' ? io.O_EXLOCK : 0));
      const before = (await handle.stat()).size; await handle.close();
      assert.ok(before > 0 && before < 1048576);
      assert.equal((await fs.stat(output)).size, before);
    } finally {
      clearTimeout(timeout);
      if (!closed) { child.kill(); await new Promise(resolve => child.once('close', resolve)); }
      for (const filename of [script, output]) await fs.rm(filename, { force: true });
      await fs.rmdir(directory);
    }
  });
}
