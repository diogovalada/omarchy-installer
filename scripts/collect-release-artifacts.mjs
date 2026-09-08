// Verify packaged provider bytes and collect only complete release outputs.
// The SDK smoke check writes ordinary temporary files, never physical devices.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const platform = `${process.platform === 'win32' ? 'windows' : process.platform === 'darwin' ? 'macos' : process.platform}-${process.arch}`;
assert.ok(['windows-x64', 'linux-x64', 'macos-x64', 'macos-arm64'].includes(platform));
assert.equal(process.env.OMARCHY_DISTRIBUTION, 'usb-preview');
const config = JSON.parse(readFileSync(join(root, 'apps/desktop/src-tauri/tauri.conf.json')));
const manifestPath = join(root, 'apps/desktop/.native-providers/provider-lock.json');
const manifestBytes = readFileSync(manifestPath);
const manifest = JSON.parse(manifestBytes);
assert.equal(manifest.platform, process.platform);
assert.equal(manifest.architecture, process.arch);
assert.equal(manifest.direct_x86, null);
assert.equal(manifest.apple, null);
assert.ok(manifest.files.length > 0);
const output = join(root, 'artifacts/releases');
mkdirSync(output, { recursive: true });
assert.equal(readdirSync(output).length, 0, 'Use a fresh output directory; never merge release files.');
const stem = `Omarchy-Installer-${config.version}-${platform}`;
const bundle = join(root, 'apps/desktop/src-tauri/target/release/bundle');
const sha256 = data => createHash('sha256').update(data).digest('hex');
function one(directory, predicate) {
  const names = readdirSync(directory).filter(predicate);
  assert.equal(names.length, 1, `Expected exactly one matching package in ${directory}`);
  return join(directory, names[0]);
}
function collect(source, name) {
  assert.ok(lstatSync(source).isFile() && lstatSync(source).size > 0);
  const destination = join(output, name);
  copyFileSync(source, destination);
  assert.equal(sha256(readFileSync(destination)), sha256(readFileSync(source)));
}
function verifyProviders(directory) {
  for (const record of manifest.files) {
    assert.match(record.path, /^(media-[a-f0-9]{20}|usb-preserve)\//);
    let filename = directory;
    for (const component of record.path.split('/')) {
      assert.ok(component && !['.', '..'].includes(component) && !/[:\\\0]/.test(component));
      filename = join(filename, component);
      assert.ok(!lstatSync(filename).isSymbolicLink(), 'Packaged provider paths must not be links.');
    }
    const bytes = readFileSync(filename);
    assert.equal(bytes.length, record.length, record.path);
    assert.equal(sha256(bytes), record.sha256, record.path);
  }
  const node = join(directory, manifest.media.executable);
  execFileSync(node, [join(root, 'scripts/verify-staged-media.cjs'), dirname(node)], { stdio: 'inherit' });
}
if (process.platform === 'win32') {
  const portableParent = join(root, 'artifacts/windows-portable');
  const portable = one(portableParent, name => existsSync(join(portableParent, name, 'portable-record.json')));
  const recordPath = join(portable, 'portable-record.json');
  const record = JSON.parse(readFileSync(recordPath));
  assert.equal(record.profile, 'release');
  assert.equal(record.launcher.extractionAndHashesVerified, true);
  assert.equal(record.launcher.stagedMediaFileWriteVerified, true);
  assert.equal(record.providerManifestSha256, sha256(manifestBytes));
  assert.equal(sha256(readFileSync(record.launcher.path)), record.launcher.sha256);
  collect(record.launcher.path, `${stem}-portable.exe`);
  collect(recordPath, 'portable-record.json');
} else if (process.platform === 'darwin') {
  const app = join(bundle, 'macos', `${config.productName}.app`);
  verifyProviders(join(app, 'Contents/Resources/providers'));
  execFileSync('/usr/bin/codesign', ['--verify', '--strict', app], { stdio: 'inherit' });
  execFileSync('/usr/bin/ditto', ['-c', '-k', '--sequesterRsrc', '--keepParent', app, join(output, `${stem}.app.zip`)]);
  collect(one(join(bundle, 'dmg'), name => name.endsWith('.dmg')), `${stem}.dmg`);
} else {
  const appimage = join(bundle, 'appimage');
  const appdir = one(appimage, name => name.endsWith('.AppDir'));
  verifyProviders(join(appdir, 'usr/lib', config.productName, 'providers'));
  collect(one(appimage, name => name.endsWith('.AppImage')), `${stem}.AppImage`);
  collect(one(join(bundle, 'deb'), name => name.endsWith('.deb')), `${stem}.deb`);
}
collect(manifestPath, 'provider-lock.json');
writeFileSync(join(output, 'build.json'), JSON.stringify({ version: config.version, platform,
  commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
  profile: 'release', distribution: 'usb-preview', model: 'gpt-6-astra',
  signing: process.platform === 'darwin' ? 'ad-hoc, not notarized' : 'unsigned',
  physicalInstallationQualified: false }, null, 2) + '\n');
writeFileSync(join(output, 'README.txt'), `Omarchy Installer ${config.version} — ${platform}

Unofficial preview: download Omarchy and create bootable x86 USB drives.
Installation without USB is in development. Apple Silicon hosts create USBs
for x86 computers; the USB does not install Omarchy on Apple Silicon.

Windows: run the portable EXE; WebView2 must be available.
Linux: install the DEB, or make the AppImage executable and use
  ./Omarchy-Installer-${config.version}-linux-x64.AppImage --appimage-extract-and-run
for USB operations. Ubuntu 24.04 or a compatible newer system is required;
pkexec, util-linux, libusb and udev must be available.
macOS: use the DMG or extract the APP ZIP. macOS 15+ is required. These preview
builds use ad-hoc signing, are not notarized, and may be blocked by Gatekeeper.

See the repository README for the tested paths and remaining hardware checks.
SHA256SUMS checks download integrity; it is not a publisher signature.
`);
const checksums = readdirSync(output).sort().map(name => `${sha256(readFileSync(join(output, name)))}  ${name}`).join('\n') + '\n';
writeFileSync(join(output, 'SHA256SUMS'), checksums);
console.log(`Verified ${platform} release packages in ${output}`);
