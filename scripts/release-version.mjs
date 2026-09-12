import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export function validateVersion(version) {
  assert.equal(version, version.trim(), 'Version must not contain surrounding whitespace');
  assert.match(version, /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?:preview|alpha|beta|rc)\.[1-9]\d*)?$/,
    'Use X.Y.Z or X.Y.Z-preview.N (alpha, beta and rc are also supported).');
  return version;
}

export function versionUpdates(root, version) {
  validateVersion(version);
  const updates = new Map([['VERSION', version + '\n']]);
  for (const file of ['apps/desktop/package.json', 'apps/desktop/src-tauri/tauri.conf.json']) {
    const data = JSON.parse(readFileSync(join(root, file), 'utf8'));
    assert.equal(typeof data.version, 'string', `Missing version in ${file}`);
    data.version = version;
    updates.set(file, JSON.stringify(data, null, 2) + '\n');
  }
  for (const file of ['apps/desktop/src-tauri/Cargo.toml', 'apps/desktop/src-tauri/Cargo.lock']) {
    const text = readFileSync(join(root, file), 'utf8');
    const pattern = /(^name = "omarchy-setup-desktop"\r?\nversion = ")[^"]+(".*$)/gm;
    assert.equal([...text.matchAll(pattern)].length, 1, `Expected one desktop package in ${file}`);
    updates.set(file, text.replace(pattern, (_, before, after) => before + version + after));
  }
  return updates;
}

export function checkVersion(root, ref = '') {
  const version = validateVersion(readFileSync(join(root, 'VERSION'), 'utf8').trim());
  for (const [file, expected] of versionUpdates(root, version)) {
    // Accept either checkout newline style; do not mistake CRLF for version drift.
    assert.equal(readFileSync(join(root, file), 'utf8').replaceAll('\r\n', '\n'), expected.replaceAll('\r\n', '\n'),
      `Version metadata differs in ${file}; run pnpm release:version ${version}`);
  }
  const changelog = readFileSync(join(root, 'CHANGELOG.md'), 'utf8');
  assert.ok(changelog.includes(`## [${version}]`), `Add a CHANGELOG.md entry for ${version}`);
  if (ref.startsWith('refs/tags/')) assert.equal(ref, `refs/tags/v${version}`, 'Release tag must match VERSION');
  return version;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const args = process.argv.slice(2);
  assert.equal(args.length, 1, 'Usage: node scripts/release-version.mjs --check | X.Y.Z-preview.N');
  if (args[0] === '--check') {
    console.log(`Version metadata agrees: ${checkVersion(root, process.env.GITHUB_REF)}`);
  } else {
    const version = validateVersion(args[0]);
    const tags = execFileSync('git', ['tag', '--list', `v${version}`], { cwd: root, encoding: 'utf8' }).trim();
    assert.equal(tags, '', 'This version is already tagged. Prepare a new version instead.');
    // Parse and validate all inputs before changing any file.
    const updates = versionUpdates(root, version);
    for (const [file, text] of updates) writeFileSync(join(root, file), text);
    console.log(`Prepared ${version}. Update CHANGELOG.md, then run pnpm release:check. No tag or release was created.`);
  }
}
