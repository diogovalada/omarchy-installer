import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { basename, dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { appendFileSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { test } from 'node:test';
import { digest, packageNames, platforms, prepareRelease } from './prepare-release.mjs';

const version = '0.1.0-preview.2';
const commit = 'a'.repeat(40);
const hash = data => createHash('sha256').update(data).digest('hex');
function fixture(t, { omit, wrongCommit } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'omarchy-publish-test-'));
  t.after(() => {
    assert.equal(dirname(resolve(root)), resolve(tmpdir()));
    assert.match(basename(root), /^omarchy-publish-test-[A-Za-z0-9]+$/);
    rmSync(root, { recursive: true, force: true });
  });
  const input = join(root, 'input');
  const output = join(root, 'output');
  for (const platform of platforms) {
    if (platform === omit) continue;
    const directory = join(input, `Omarchy-Installer-${platform}-${commit}`);
    mkdirSync(directory, { recursive: true });
    const files = new Map(packageNames(version, platform).map(name => [name, 'fixture package bytes']));
    files.set('build.json', JSON.stringify({ version, commit: wrongCommit ? 'b'.repeat(40) : commit, platform, profile: 'release', distribution: 'usb-preview' }));
    files.set('provider-lock.json', '{}');
    files.set('README.txt', 'fixture readme');
    files.set('THIRD_PARTY_NOTICES.md', 'fixture notice');
    if (platform === 'windows-x64') files.set('portable-record.json', '{}');
    for (const [name, data] of files) writeFileSync(join(directory, name), data);
    writeFileSync(join(directory, 'SHA256SUMS'), [...files].map(([name, data]) => `${hash(data)}  ${name}\n`).join(''));
  }
  return { input, output, first: join(input, `Omarchy-Installer-windows-x64-${commit}`) };
}
test('only verified runnable packages are published; metadata stays in build artifacts', async t => {
  const { input, output } = fixture(t);
  const names = await prepareRelease(input, output, version, commit);
  const expected = platforms.flatMap(platform => packageNames(version, platform));
  assert.deepEqual(names.sort(), expected.sort());
  assert.deepEqual(readdirSync(output).sort(), expected.sort());
  for (const name of names) {
    assert.equal(await digest(join(output, name)), hash('fixture package bytes'));
  }
});
test('unpublished metadata still has to pass verification', async t => {
  const { input, output, first } = fixture(t);
  appendFileSync(join(first, 'THIRD_PARTY_NOTICES.md'), 'tampered');
  await assert.rejects(prepareRelease(input, output, version, commit), /checksum mismatch/);
  assert.deepEqual(readdirSync(output), []);
});
test('tampered packages stop preparation before copying assets', async t => {
  const { input, output, first } = fixture(t);
  appendFileSync(join(first, packageNames(version, 'windows-x64')[0]), 'tampered');
  await assert.rejects(prepareRelease(input, output, version, commit), /checksum mismatch/);
  assert.deepEqual(readdirSync(output), []);
});
test('a missing platform or wrong build commit cannot produce a release', async t => {
  const missing = fixture(t, { omit: 'macos-arm64' });
  await assert.rejects(prepareRelease(missing.input, missing.output, version, commit));
  assert.deepEqual(readdirSync(missing.output), []);
  const wrong = fixture(t, { wrongCommit: true });
  await assert.rejects(prepareRelease(wrong.input, wrong.output, version, commit), /wrong commit/);
});
test('checksum path traversal is rejected', async t => {
  const { input, output, first } = fixture(t);
  appendFileSync(join(first, 'SHA256SUMS'), `${'a'.repeat(64)}  ../outside\n`);
  await assert.rejects(prepareRelease(input, output, version, commit), /Invalid checksum/);
});
