// linuxdeploy rewrites ELF files and scans foreign SDK binaries. Build the GUI
// AppImage first, then add the authenticated USB runtime and pack it unchanged.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, cpSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { homedir } from 'node:os';
import { fileURLToPath } from 'node:url';

assert.equal(process.platform, 'linux');
assert.equal(process.arch, 'x64');
assert.equal(process.env.OMARCHY_DISTRIBUTION, 'usb-preview');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const tauri = join(root, 'apps/desktop/src-tauri');
const config = JSON.parse(readFileSync(join(tauri, 'tauri.conf.json')));
const preview = JSON.parse(readFileSync(join(tauri, 'tauri.release-preview.conf.json')));
const env = { ...process.env, APPIMAGE_EXTRACT_AND_RUN: '1', NO_STRIP: '1' };
function run(command, args, extraEnv = {}) {
  execFileSync(command, args, { cwd: root, env: { ...env, ...extraEnv }, stdio: 'inherit' });
}
run('pnpm', ['--dir', 'apps/desktop', 'exec', 'tauri', 'build', '--config', 'src-tauri/tauri.release-preview.conf.json', '--bundles', 'deb']);

const parent = join(root, 'artifacts/linux-packaging');
mkdirSync(parent, { recursive: true });
const scratch = mkdtempSync(join(parent, 'build-'));
const shellConfig = join(scratch, 'appimage-shell.json');
preview.bundle.resources = [];
writeFileSync(shellConfig, JSON.stringify(preview, null, 2) + '\n');
// Tauri retains this intermediate directory when an earlier AppImage build
// fails. Archive it so old resources cannot leak into the GUI-only package.
const intermediate = join(tauri, 'target/release/bundle/appimage_deb');
if (existsSync(intermediate)) renameSync(intermediate, join(scratch, 'previous-appimage-deb'));
// Keep the DEB's application binary before Tauri adds another bundle marker.
const application = join(tauri, 'target/release/omarchy-setup-desktop');
copyFileSync(application, join(scratch, 'application-before-appimage'));
run('pnpm', ['--dir', 'apps/desktop', 'exec', 'tauri', 'bundle', '--verbose', '--config', shellConfig, '--bundles', 'appimage']);

const output = join(tauri, 'target/release/bundle/appimage');
const appdirs = readdirSync(output).filter(name => name.endsWith('.AppDir'));
const images = readdirSync(output).filter(name => name.endsWith('.AppImage'));
assert.equal(appdirs.length, 1);
assert.equal(images.length, 1);
const appdir = join(output, appdirs[0]);
const providers = join(appdir, 'usr/lib', config.productName, 'providers');
assert.ok(!existsSync(providers), 'The GUI packaging step must not process providers.');
cpSync(join(root, 'apps/desktop/.native-providers/bundle'), providers, { recursive: true, errorOnExist: true, force: false });
const manifest = JSON.parse(readFileSync(join(root, 'apps/desktop/.native-providers/provider-lock.json')));
assert.equal(manifest.platform, 'linux');
for (const record of manifest.files) {
  assert.ok(!record.path.split('/').some(part => !part || ['.', '..'].includes(part) || /[\\:\0]/.test(part)));
  const file = join(providers, record.path);
  assert.ok(lstatSync(file).isFile() && !lstatSync(file).isSymbolicLink());
  const bytes = readFileSync(file);
  assert.equal(bytes.length, record.length, record.path);
  assert.equal(createHash('sha256').update(bytes).digest('hex'), record.sha256, record.path);
}
const node = join(providers, manifest.media.executable);
run(node, [join(root, 'scripts/verify-staged-media.cjs'), dirname(node)]);
const image = join(output, images[0]);
renameSync(image, join(scratch, 'gui-only.AppImage'));
const plugin = join(process.env.XDG_CACHE_HOME || join(homedir(), '.cache'), 'tauri/linuxdeploy-plugin-appimage.AppImage');
assert.ok(lstatSync(plugin).isFile(), 'The Tauri AppImage output plugin is required.');
run(plugin, ['--appdir', appdir], { OUTPUT: image, ARCH: 'x86_64' });
assert.ok(lstatSync(image).isFile() && lstatSync(image).size > 0);
console.log(`Built Linux packages with unchanged provider files: ${image}`);
