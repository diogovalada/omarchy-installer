import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { test } from 'node:test';
import { checkVersion, validateVersion, versionUpdates } from './release-version.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'omarchy-version-test-'));
  t.after(() => {
    assert.equal(dirname(resolve(root)), resolve(tmpdir()));
    assert.match(basename(root), /^omarchy-version-test-[A-Za-z0-9]+$/);
    rmSync(root, { recursive: true, force: true });
  });
  const files = {
    VERSION: '0.1.0-preview.2\n',
    'CHANGELOG.md': '## [0.1.0-preview.2] - Unreleased\n',
    'apps/desktop/package.json': '{"name":"desktop","version":"0.1.0"}',
    'apps/desktop/src-tauri/tauri.conf.json': '{"version":"0.1.0","build":{"beforeBuildCommand":"keep me"}}',
    'apps/desktop/src-tauri/Cargo.toml': '[package]\nname = "omarchy-setup-desktop"\nversion = "0.1.0"\n',
    'apps/desktop/src-tauri/Cargo.lock': 'version = 4\n\n[[package]]\nname = "omarchy-setup-desktop"\nversion = "0.1.0"\n\n[[package]]\nname = "unrelated"\nversion = "0.1.0"\n'
  };
  for (const [file, content] of Object.entries(files)) {
    mkdirSync(dirname(join(root, file)), { recursive: true });
    writeFileSync(join(root, file), content);
  }
  return root;
}
test('prepare synchronizes app metadata without changing unrelated package versions', t => {
  const root = fixture(t);
  for (const [file, text] of versionUpdates(root, '0.1.0-preview.2')) writeFileSync(join(root, file), text);
  assert.equal(checkVersion(root, 'refs/tags/v0.1.0-preview.2'), '0.1.0-preview.2');
  assert.match(readFileSync(join(root, 'apps/desktop/src-tauri/Cargo.lock'), 'utf8'), /name = "unrelated"\nversion = "0.1.0"/);
  assert.equal(JSON.parse(readFileSync(join(root, 'apps/desktop/src-tauri/tauri.conf.json'))).build.beforeBuildCommand, 'keep me');
  assert.throws(() => checkVersion(root, 'refs/tags/v0.1.0-preview.3'), /tag must match/);
});
test('drift and missing changelog entries stop release checks', t => {
  const root = fixture(t);
  assert.throws(() => checkVersion(root), /metadata differs/);
  for (const [file, text] of versionUpdates(root, '0.1.0-preview.3')) writeFileSync(join(root, file), text);
  assert.throws(() => checkVersion(root), /CHANGELOG/);
});
test('malformed versions or ambiguous lockfile records fail before writes', t => {
  const root = fixture(t);
  for (const version of ['v0.1.0', '0.01.0', '0.1.0-preview.0', '0.1.0-preview.02', '../other', '0.1.0\n']) {
    assert.throws(() => validateVersion(version), undefined, version);
  }
  const lock = join(root, 'apps/desktop/src-tauri/Cargo.lock');
  writeFileSync(lock, readFileSync(lock, 'utf8').replace('omarchy-setup-desktop', 'missing-desktop'));
  assert.throws(() => versionUpdates(root, '0.1.0-preview.3'), /Expected one desktop/);
  assert.equal(readFileSync(join(root, 'VERSION'), 'utf8'), '0.1.0-preview.2\n');
});
