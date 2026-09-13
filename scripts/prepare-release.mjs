import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createReadStream, copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { checkVersion, validateVersion } from './release-version.mjs';

export const platforms = ['windows-x64', 'linux-x64', 'macos-x64', 'macos-arm64'];
export async function digest(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}

export function packageNames(version, platform) {
  const stem = `Omarchy-Installer-${version}-${platform}`;
  if (platform === 'windows-x64') return [`${stem}-portable.exe`];
  if (platform === 'linux-x64') return [`${stem}.AppImage`];
  assert.ok(platforms.includes(platform));
  return [`${stem}.app.zip`];
}

export async function prepareRelease(input, output, version, commit) {
  validateVersion(version);
  assert.match(commit, /^[a-f0-9]{40}$/);
  assert.notEqual(resolve(input), resolve(output));
  mkdirSync(output, { recursive: true });
  assert.equal(readdirSync(output).length, 0, 'Use an empty release output directory');
  // Verify every platform and every checksum before copying any release assets.
  const plan = [];
  let noticeHash;
  for (const platform of platforms) {
    const directory = join(input, `Omarchy-Installer-${platform}-${commit}`);
    assert.ok(lstatSync(directory).isDirectory() && !lstatSync(directory).isSymbolicLink());
    const expected = [...packageNames(version, platform), 'build.json', 'provider-lock.json', 'README.txt', 'THIRD_PARTY_NOTICES.md'];
    if (platform === 'windows-x64') expected.push('portable-record.json');
    assert.deepEqual(readdirSync(directory).sort(), [...expected, 'SHA256SUMS'].sort(), `${platform}: unexpected or missing files`);
    for (const name of [...expected, 'SHA256SUMS']) {
      const info = lstatSync(join(directory, name));
      assert.ok(info.isFile() && !info.isSymbolicLink() && info.size > 0, `${platform}/${name}: invalid file`);
    }
    const checksums = new Map();
    for (const line of readFileSync(join(directory, 'SHA256SUMS'), 'utf8').trim().split(/\r?\n/)) {
      const match = /^([a-f0-9]{64})  ([A-Za-z0-9._-]+)$/.exec(line);
      assert.ok(match, 'Invalid checksum entry');
      assert.ok(!checksums.has(match[2]), 'Duplicate checksum entry');
      checksums.set(match[2], match[1]);
    }
    assert.deepEqual([...checksums.keys()].sort(), expected.sort(), `${platform}: incomplete checksum manifest`);
    for (const name of expected) assert.equal(await digest(join(directory, name)), checksums.get(name), `${platform}/${name}: checksum mismatch`);
    const record = JSON.parse(readFileSync(join(directory, 'build.json'), 'utf8'));
    assert.equal(record.version, version, `${platform}: wrong version`);
    assert.equal(record.commit, commit, `${platform}: wrong commit`);
    assert.equal(record.platform, platform);
    assert.equal(record.profile, 'release');
    assert.equal(record.distribution, 'usb-preview');
    const thisNotice = checksums.get('THIRD_PARTY_NOTICES.md');
    if (noticeHash) assert.equal(thisNotice, noticeHash, 'Platforms disagree on third-party notices');
    noticeHash = thisNotice;
    for (const name of expected) {
      const destination = name.startsWith('Omarchy-Installer-') || name === 'THIRD_PARTY_NOTICES.md' ? name : `${platform}-${name}`;
      plan.push({ source: join(directory, name), destination, hash: checksums.get(name) });
    }
  }
  const hashes = new Map();
  for (const item of plan) {
    const path = join(output, item.destination);
    if (existsSync(path)) assert.equal(hashes.get(item.destination), item.hash, 'Conflicting release filename');
    else copyFileSync(item.source, path);
    assert.equal(await digest(path), item.hash);
    hashes.set(item.destination, item.hash);
  }
  writeFileSync(join(output, 'SHA256SUMS'), [...hashes].sort(([a], [b]) => a.localeCompare(b)).map(([name, hash]) => `${hash}  ${name}\n`).join(''));
  return [...hashes.keys(), 'SHA256SUMS'];
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const version = checkVersion(root, process.env.GITHUB_REF);
  const input = join(root, 'artifacts/release-input');
  const output = join(root, 'artifacts/publish');
  const assets = await prepareRelease(input, output, version, process.env.GITHUB_SHA);
  const changelog = readFileSync(join(root, 'CHANGELOG.md'), 'utf8');
  const entry = changelog.split(`## [${version}]`)[1].split('\n## ')[0];
  assert.match(entry, /^ - \d{4}-\d{2}-\d{2}/, 'Date the changelog before publishing');
  const labels = ['Windows x64 — portable EXE', 'Linux x64 — AppImage', 'Mac Intel — APP ZIP', 'Mac Apple Silicon — APP ZIP'];
  const downloads = platforms.map((platform, i) => `- [${labels[i]}](https://github.com/diogovalada/omarchy-installer/releases/download/v${version}/${packageNames(version, platform)[0]})`).join('\n');
  writeFileSync(join(root, 'artifacts/release-notes.md'), `**Experimental preview — hardware and boot testing remain incomplete.**\n\nUnofficial Omarchy Installer community preview.\n\n${downloads}\n\n${entry.slice(entry.indexOf('\n')).trim()}\n\nDownloads include Windows portable EXE, Linux AppImage, and macOS Intel/Apple Silicon APP ZIP. Extract the Mac ZIP and open the app; copying it to Applications is optional.\n\nThe preview supports download and USB preparation. Direct installation remains disabled. Apple Silicon creates USB media for an x86 computer. macOS packages are ad-hoc signed and not notarized; Windows packages are unsigned. Hardware qualification remains limited as described in the README.\n\nCommit: ${process.env.GITHUB_SHA}\n\nSee SHA256SUMS, per-platform build records and THIRD_PARTY_NOTICES.md.\n`);
  console.log(`Verified ${assets.length} release assets for v${version}.`);
}
