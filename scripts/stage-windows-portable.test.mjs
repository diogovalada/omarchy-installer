import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const scripts = dirname(fileURLToPath(import.meta.url));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');

test('Windows preview staging produces a complete, verifiable payload', { skip: process.platform !== 'win32' }, t => {
  const root = mkdtempSync(join(tmpdir(), 'omarchy-stage-test-'));
  t.after(() => {
    assert.equal(dirname(resolve(root)), resolve(tmpdir()));
    assert.match(basename(root), /^omarchy-stage-test-[A-Za-z0-9]+$/);
    rmSync(root, { recursive: true, force: true });
  });
  function put(path, bytes) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), bytes);
  }
  put('scripts/stage-windows-portable.mjs', readFileSync(join(scripts, 'stage-windows-portable.mjs')));
  copyFileSync(join(scripts, 'verify-portable-bundle.mjs'), join(root, 'scripts/verify-portable-bundle.mjs'));
  const provider = Buffer.from('fixture provider, never executed');
  const manifest = Buffer.from(JSON.stringify({
    schema: 1, platform: 'win32', architecture: 'x64', direct_x86: null, apple: null,
    media: { executable: 'media/node.exe' },
    files: [{ path: 'media/node.exe', length: provider.length, sha256: hash(provider) }],
  }));
  put('apps/desktop/.native-providers/provider-lock.json', manifest);
  put('apps/desktop/.native-providers/bundle/media/node.exe', provider);
  // Only the header and embedded manifest are needed; this fixture is never run.
  const header = Buffer.alloc(256);
  header.writeUInt16LE(0x5a4d, 0);
  header.writeUInt32LE(64, 0x3c);
  header.writeUInt32LE(0x4550, 64);
  header.writeUInt16LE(2, 64 + 92);
  put('apps/desktop/src-tauri/target/release/omarchy-setup-desktop.exe', Buffer.concat([header, manifest]));
  for (const path of ['LICENSE', 'THIRD_PARTY_NOTICES.md',
    'apps/desktop/src/assets/omarchy/LICENSE-Omarchy.txt',
    'apps/desktop/src/assets/omarchy/OFL-JetBrainsMono.txt']) {
    put(path, readFileSync(join(scripts, '..', path)));
  }
  const staged = spawnSync(process.execPath, [join(root, 'scripts/stage-windows-portable.mjs'), 'release', 'built'], {
    encoding: 'utf8', env: { ...process.env, OMARCHY_DISTRIBUTION: 'usb-preview' },
  });
  assert.equal(staged.status, 0, staged.stderr);
  const output = JSON.parse(staged.stdout);
  const record = JSON.parse(readFileSync(output.recordPath, 'utf8'));
  assert.equal(new Set(record.files.map(file => file.path)).size, record.files.length);
  for (const name of ['Omarchy Installer.exe', 'providers/media/node.exe', 'LICENSE.txt',
    'THIRD_PARTY_NOTICES.md', 'LICENSE-Omarchy.txt', 'OFL-JetBrainsMono.txt',
    'README.txt', 'verify-bundle.mjs', 'bundle-files.json']) {
    assert.ok(record.files.some(file => file.path === name), `Missing ${name}`);
  }
  for (const file of record.files) {
    assert.equal(hash(readFileSync(join(output.payloadDirectory, file.path))), file.sha256);
  }
  const verify = () => spawnSync(process.execPath, [join(output.payloadDirectory, 'verify-bundle.mjs')], { encoding: 'utf8' });
  assert.equal(verify().status, 0);
  writeFileSync(join(output.payloadDirectory, 'THIRD_PARTY_NOTICES.md'), 'tampered');
  assert.notEqual(verify().status, 0, 'Modified payload must fail verification');
});
