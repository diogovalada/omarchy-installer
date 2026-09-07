import { createHash, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, lstatSync } from 'node:fs';
import { resolve, dirname, join, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const [profile, mode, verifiedPortableRecord] = process.argv.slice(2);
if (process.platform !== 'win32' || !['debug', 'release'].includes(profile) || !['built', 'reuse'].includes(mode)) throw new Error('Unsupported portable packaging arguments.');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const sourceExe = join(root, `apps/desktop/src-tauri/target/${profile}/omarchy-setup-desktop.exe`);
const application = readFileSync(sourceExe);
const peOffset = application.length >= 64 ? application.readUInt32LE(0x3c) : -1;
if (peOffset < 0 || peOffset + 94 > application.length || application.readUInt16LE(0) !== 0x5a4d ||
    application.readUInt32LE(peOffset) !== 0x4550 || application.readUInt16LE(peOffset + 92) !== 2) {
  throw new Error('The portable application must use the Windows GUI subsystem so closing a console cannot terminate it.');
}
const applicationSha = hash(application);
if (mode === 'reuse') {
  const prior = JSON.parse(readFileSync(verifiedPortableRecord ? resolve(verifiedPortableRecord) : join(root, 'artifacts/windows-package/package-record.json'), 'utf8'));
  const priorMatches = verifiedPortableRecord
    ? prior.format === 'portable-executable' && prior.launcher?.extractionAndHashesVerified === true && prior.sourceApplicationSha256 === applicationSha && prior.files.some(file => file.path === 'Omarchy Setup.exe' && file.sha256 === applicationSha && file.sizeBytes === application.length)
    : prior.files.some(file => resolve(file.path) === sourceExe && file.sha256 === applicationSha && file.sizeBytes === application.length);
  if (prior.profile !== profile || !priorMatches) {
    throw new Error('The existing executable does not match its verified build record. Build afresh.');
  }
}
const manifestBytes = readFileSync(join(root, 'apps/desktop/.native-providers/provider-lock.json'));
if (!application.includes(manifestBytes)) throw new Error('The staged provider manifest is not embedded in this executable. Rebuild before packaging.');
const manifest = JSON.parse(manifestBytes);
if (manifest.schema !== 1 || manifest.platform !== 'win32' || manifest.architecture !== 'x64' || !manifest.files.length || manifest.files.length > 50000) throw new Error('Unsupported provider manifest.');
if (!manifest.files.some(file => file.path === 'image-builder-x86/runtime.tar')) throw new Error('The portable preview must include the construction runtime.');
const outputParent = join(root, 'artifacts/windows-portable');
mkdirSync(outputParent, { recursive: true });
const packageId = randomUUID().slice(0, 8);
const outputDirectory = join(outputParent, packageId);
mkdirSync(outputDirectory); // Always a new directory; never merge user files into a package.
// NSIS's Windows compiler still bounds source paths. Keep its private staging
// shorter than the normal output directory for deeply nested SDK dependencies.
const stagingParent = join(root, 'artifacts/p');
mkdirSync(stagingParent, { recursive: true });
const portable = join(stagingParent, packageId);
if (manifest.files.some(file => join(portable, 'providers', file.path).length >= 260)) throw new Error('The checkout path is too long for NSIS packaging; use a shorter checkout path.');
mkdirSync(portable);
const files = [];
function put(relative, bytes) {
  const path = join(portable, relative);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, bytes, { flag: 'wx' });
  files.push({ path: relative, sizeBytes: bytes.length, sha256: hash(bytes) });
}
put('Omarchy Setup.exe', application);
const seen = new Set();
const bundle = join(root, 'apps/desktop/.native-providers/bundle');
for (const file of manifest.files) {
  if (typeof file.path !== 'string' || isAbsolute(file.path) || /[:\\\0]/.test(file.path) || file.path.split('/').some(part => !part || part === '.' || part === '..' || /[. ]$/.test(part))) throw new Error('Invalid provider path.');
  const key = file.path.toLowerCase();
  if (seen.has(key)) throw new Error('Duplicate provider path.');
  seen.add(key);
  let cursor = bundle;
  for (const part of file.path.split('/')) {
    cursor = join(cursor, part);
    if (lstatSync(cursor).isSymbolicLink()) throw new Error('Provider links are not permitted.');
  }
  const stat = lstatSync(cursor);
  if (!stat.isFile() || stat.size !== file.length || stat.size > 256 * 1024 * 1024) throw new Error(`Invalid provider file: ${file.path}`);
  const bytes = readFileSync(cursor);
  if (hash(bytes) !== file.sha256) throw new Error(`Provider hash mismatch: ${file.path}`);
  put(`providers/${file.path}`, bytes);
}
put('LICENSE.txt', readFileSync(join(root, 'LICENSE')));
put('THIRD_PARTY_NOTICES.md', readFileSync(join(root, 'THIRD_PARTY_NOTICES.md')));
put('LICENSE-Omarchy.txt', readFileSync(join(root, 'apps/desktop/src/assets/omarchy/LICENSE-Omarchy.txt')));
put('OFL-JetBrainsMono.txt', readFileSync(join(root, 'apps/desktop/src/assets/omarchy/OFL-JetBrainsMono.txt')));
put('README.txt', Buffer.from(`Omarchy Setup — portable community preview

The distribution is one executable. Open Omarchy-Setup-0.1.0-x64-portable.exe.
It extracts the payload once into a cache under LocalAppData/OmarchySetup/p.
Later launches verify and reuse the exact bundled contents. Missing or changed
files cause a fresh extraction. Different payloads use separate cache versions.
There is no application installation wizard, Start menu registration or
uninstaller. Normal exit retains the cache and removes the small temporary
verifier. The cache can be removed while the app is closed; the next launch
recreates it. Interrupted or damaged generations are set aside within the cache.
Launch through the portable executable so the cache is checked before use.

Requires Windows x64 and Microsoft Edge WebView2 Runtime.
Direct Omarchy installation also requires Docker Desktop with its Linux engine,
85 GiB of working storage after ISO staging and 10 GiB of free memory.
The application still stores downloads, cache and operation/recovery records
in their normal locations and requests administrator access for privileged work.

This is an unsigned, unofficial preview. Complete installation and boot/recovery
qualification remain pending. The portable packaging does not change those limits.
`, 'utf8'));
put('verify-bundle.mjs', readFileSync(join(root, 'scripts/verify-portable-bundle.mjs')));
put('bundle-files.json', Buffer.from(JSON.stringify({ schemaVersion: 1, files }, null, 2) + '\n'));
// Compare copied bytes as well as their authenticated sources.
for (const file of files) if (hash(readFileSync(join(portable, file.path))) !== file.sha256) throw new Error(`Portable copy mismatch: ${file.path}`);
const recordPath = join(outputDirectory, 'portable-record.json');
// A fresh verifier checks this manifest against the digest compiled into NSIS.
// Include bundle-files.json too; the cached manifest cannot authenticate itself.
const cacheManifest = Buffer.from(JSON.stringify({ schemaVersion: 1, files }, null, 2) + '\n');
const cacheHash = hash(cacheManifest);
const cacheId = cacheHash.slice(0, 20); // Short paths; the full SHA-256 is always verified.
const cacheManifestPath = join(outputDirectory, 'cache-manifest.json');
writeFileSync(cacheManifestPath, cacheManifest, { flag: 'wx' });
writeFileSync(recordPath, JSON.stringify({ schemaVersion: 1, model: 'gpt-6-astra', createdAt: new Date().toISOString(), format: 'portable-executable', profile, signed: false, installationQualified: false, windowsGuiSubsystemVerified: true, sourceApplicationSha256: applicationSha, providerManifestSha256: hash(manifestBytes), providerFilesVerified: manifest.files.length, cache: { id: cacheId, manifestSha256: cacheHash, payloadBytes: files.reduce((sum, file) => sum + file.sizeBytes, 0) }, files }, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify({ outputDirectory, payloadDirectory: portable, recordPath, cacheManifestPath, cacheHash, cacheId, executablePath: join(outputDirectory, 'Omarchy-Setup-0.1.0-x64-portable.exe'), nodeRelative: manifest.media.executable.replaceAll('/', '\\') }));
