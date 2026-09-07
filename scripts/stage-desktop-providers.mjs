import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync, lstatSync, mkdirSync, copyFileSync, writeFileSync } from 'node:fs';
import { resolve, relative, join, dirname, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const staging = join(root, 'apps/desktop/.native-providers');
const bundle = join(staging, 'bundle');
mkdirSync(bundle, { recursive: true });
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name)).flatMap(entry => {
    if (entry.name.startsWith('.') || ['node_modules', '__pycache__'].includes(entry.name)) return [];
    const path = join(directory, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`Provider source contains a link: ${path}`);
    return entry.isDirectory() ? files(path) : [path];
  });
}
const media = join(root, 'providers/media-etcher');
const compiler = join(media, 'node_modules/typescript/bin/tsc');
if (!existsSync(compiler)) throw new Error('Install providers/media-etcher dependencies with npm ci first.');
execFileSync(process.execPath, [compiler, '-p', join(media, 'tsconfig.json')], { cwd: media, stdio: 'inherit' });
const sourceFingerprint = digest(Buffer.concat([
  Buffer.from(`${process.version}/${process.platform}/${process.arch}`),
  readFileSync(join(media, 'package-lock.json')),
  readFileSync(join(media, 'PHYSICAL-PROVIDER.md')),
  ...files(join(media, 'src')).map(path => readFileSync(path)),
  ...files(join(media, 'scripts')).map(path => readFileSync(path)),
]));
const mediaName = `media-${sourceFingerprint.slice(0, 20)}`;
const mediaOutput = join(bundle, mediaName);
if (!existsSync(join(mediaOutput, 'runtime-manifest.json'))) {
  if (existsSync(mediaOutput)) throw new Error(`Incomplete staging directory; inspect it before retrying: ${mediaOutput}`);
  execFileSync(process.execPath, [join(media, 'scripts/stage-physical-runtime.cjs'), '--output', mediaOutput], { cwd: media, stdio: 'inherit' });
}
const runtime = JSON.parse(readFileSync(join(mediaOutput, 'runtime-manifest.json'), 'utf8'));
const records = runtime.files.map(file => ({ path: `${mediaName}/${file.path.replaceAll('\\', '/')}`, length: file.length, sha256: file.sha256 }));
const manifest = { schema: 1, platform: process.platform, architecture: process.arch,
  media: { executable: `${mediaName}/${runtime.executable}`, entrypoint: `${mediaName}/${runtime.entrypoint}` },
  media_inspection: { executable: `${mediaName}/${runtime.executable}`, entrypoint: `${mediaName}/${runtime.inspection_entrypoint}` },
  direct_x86: null, apple: null, files: records };

for (const providerName of ['direct-x86', 'image-builder-x86', 'usb-preserve']) {
  const source = join(root, 'providers', providerName);
  if (!existsSync(source)) continue;
  for (const path of files(source)) {
    if (providerName === 'usb-preserve' && !relative(source, path).startsWith('boot' + sep)) continue;
    if (!/\.(ps1|py|json|lock|gpg|asc|sh|cs|EFI|xz|txt|dsc|template|cfg)$/.test(path) && !['Dockerfile', 'NOTICE', 'runtime.tar'].includes(path.split(sep).at(-1))) continue;
    const local = relative(source, path).split(sep).join('/');
    const destination = join(bundle, providerName, local);
    mkdirSync(dirname(destination), { recursive: true });
    copyFileSync(path, destination);
    const data = readFileSync(destination);
    manifest.files.push({ path: `${providerName}/${local}`, length: data.length, sha256: digest(data) });
  }
}
if (existsSync(join(bundle, 'direct-x86/Invoke-DirectX86.ps1'))) manifest.direct_x86 = { entrypoint: 'direct-x86/Invoke-DirectX86.ps1' };
// Swift companion signing/packaging has its own descriptor and cannot be
// replaced by copying an unsigned executable from the source submodule.
manifest.files.sort((a, b) => a.path.localeCompare(b.path));
for (const file of manifest.files) {
  const path = resolve(bundle, file.path);
  if (!path.startsWith(bundle + sep) || lstatSync(path).isSymbolicLink() || !statSync(path).isFile()) throw new Error('Invalid staged provider file.');
  const data = readFileSync(path);
  if (data.length !== file.length || digest(data) !== file.sha256) throw new Error(`Staged file changed: ${file.path}`);
}
writeFileSync(join(staging, 'provider-lock.json'), JSON.stringify(manifest, null, 2) + '\n');
console.log(`Staged ${manifest.files.length} authenticated provider files for ${process.platform}/${process.arch}.`);
