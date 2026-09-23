import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const script = join(dirname(fileURLToPath(import.meta.url)), 'finalize-windows-portable.mjs');
const names = {
  release: 'Omarchy-Installer-0.1.0-x64-portable.exe',
  testing: 'Omarchy-Installer-0.1.0-x64-testing.exe',
};

test('verified builds replace stable release and testing outputs without retaining old packages', t => {
  const root = mkdtempSync(join(tmpdir(), 'omarchy-finalize-test-'));
  t.after(() => {
    assert.equal(dirname(resolve(root)), resolve(tmpdir()));
    assert.match(basename(root), /^omarchy-finalize-test-[A-Za-z0-9]+$/);
    rmSync(root, { recursive: true, force: true });
  });
  mkdirSync(join(root, 'scripts'));
  copyFileSync(script, join(root, 'scripts/finalize-windows-portable.mjs'));
  const outputParent = join(root, 'artifacts/windows-portable');
  const payloadParent = join(root, 'artifacts/p');
  mkdirSync(outputParent, { recursive: true });
  mkdirSync(payloadParent, { recursive: true });

  function packageFixture(id, kind, contents, createdAt) {
    const directory = join(outputParent, id);
    if (existsSync(directory)) rmSync(directory, { recursive: true });
    mkdirSync(directory);
    const exe = join(directory, names[kind]);
    const bytes = Buffer.from(contents);
    writeFileSync(exe, bytes);
    writeFileSync(join(directory, 'portable-record.json'), JSON.stringify({
      format: 'portable-executable', testingBuild: kind === 'testing', createdAt,
      launcher: { path: exe, sha256: createHash('sha256').update(bytes).digest('hex'),
        sizeBytes: bytes.length, extractionAndHashesVerified: true },
    }));
    if (existsSync(join(payloadParent, id))) rmSync(join(payloadParent, id), { recursive: true });
    mkdirSync(join(payloadParent, id));
    writeFileSync(join(payloadParent, id, 'bundle-files.json'), '{}');
    return exe;
  }
  function run(id, ...args) {
    return spawnSync(process.execPath, [join(root, 'scripts/finalize-windows-portable.mjs'), id, ...args], { encoding: 'utf8' });
  }
  packageFixture('11111111', 'release', 'latest release', '2026-09-17T00:00:00Z');
  packageFixture('22222222', 'testing', 'older test', '2026-09-18T00:00:00Z');
  packageFixture('33333333', 'testing', 'previous test', '2026-09-19T00:00:00Z');
  packageFixture('testing', 'testing', 'new test', '2026-09-20T00:00:00Z');
  mkdirSync(join(outputParent, 'preview-abcdef123456-1234abcd'));
  writeFileSync(join(outputParent, 'preview-abcdef123456-1234abcd', 'portable-record.json'), '{}');
  mkdirSync(join(outputParent, 'keep-me'));

  const preview = run('testing', '--dry-run');
  assert.equal(preview.status, 0, preview.stderr);
  assert.deepEqual(JSON.parse(preview.stdout).keep, { testing: 'testing', release: '11111111' });
  assert.equal(existsSync(join(outputParent, '11111111')), true);

  const first = run('testing');
  assert.equal(first.status, 0, first.stderr);
  assert.equal(first.stdout.trim(), join(outputParent, 'testing', names.testing));
  assert.equal(readFileSync(join(outputParent, 'release', names.release), 'utf8'), 'latest release');
  assert.equal(readFileSync(join(outputParent, 'testing', names.testing), 'utf8'), 'new test');
  assert.equal(JSON.parse(readFileSync(join(outputParent, 'release/portable-record.json'))).launcher.path,
    join(outputParent, 'release', names.release));
  for (const id of ['11111111', '22222222', '33333333']) {
    assert.equal(existsSync(join(outputParent, id)), false);
    assert.equal(existsSync(join(payloadParent, id)), false);
  }
  assert.equal(existsSync(join(outputParent, 'preview-abcdef123456-1234abcd')), false);
  assert.equal(existsSync(join(payloadParent, 'testing')), false);
  assert.equal(existsSync(join(outputParent, 'keep-me')), true);

  packageFixture('release', 'release', 'new release', '2026-09-21T00:00:00Z');
  const second = run('release');
  assert.equal(second.status, 0, second.stderr);
  assert.equal(readFileSync(join(outputParent, 'release', names.release), 'utf8'), 'new release');
  assert.equal(readFileSync(join(outputParent, 'testing', names.testing), 'utf8'), 'new test');

  const bad = packageFixture('testing', 'testing', 'bad test', '2026-09-22T00:00:00Z');
  writeFileSync(bad, 'corrupt');
  const rejected = run('testing');
  assert.notEqual(rejected.status, 0);
  assert.match(rejected.stderr, /checksum mismatch/i);
  assert.equal(readFileSync(join(outputParent, 'release', names.release), 'utf8'), 'new release');
  assert.equal(existsSync(join(outputParent, 'testing')), true);
});
