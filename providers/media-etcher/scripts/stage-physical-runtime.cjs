// Stage only. Does not execute tests, enumerate devices, elevate or write media.
// The caller builds TypeScript before invoking this script with a NEW output dir.
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const root = path.resolve(__dirname, '..');
const args = process.argv.slice(2);
if (args.length !== 2 || args[0] !== '--output' || !path.isAbsolute(args[1])) {
  throw new Error('Usage: node scripts/stage-physical-runtime.cjs --output <absolute-new-directory>');
}
const output = path.resolve(args[1]);
if (fs.existsSync(output)) throw new Error('Output must be a new directory; staging never replaces or deletes existing files.');
if (![22, 24].includes(Number(process.versions.node.split('.')[0]))) throw new Error('Stage with Node 22 or 24.');
if (!fs.existsSync(path.join(root, 'dist', 'physical-cli.js'))) throw new Error('Run npm run build first.');
if (process.platform === 'win32' && require('@ronomon/direct-io').omarchyGeometryVersion !== 1) throw new Error('Build the Windows native geometry extension before staging.');
const lock = JSON.parse(fs.readFileSync(path.join(root, 'package-lock.json'), 'utf8'));
const sdk = JSON.parse(fs.readFileSync(path.join(root, 'node_modules', 'etcher-sdk', 'package.json'), 'utf8'));
if (sdk.version !== '10.2.14') throw new Error('Installed SDK version differs from pinned version.');
const nodeLicense = [process.env.OMARCHY_NODE_LICENSE, ...['LICENSE', 'LICENSE.txt', 'LICENSE.md'].map(name => path.join(path.dirname(process.execPath), name)),
  path.join(path.dirname(process.execPath), '..', 'LICENSE'),
  ...(process.platform === 'linux' ? ['/usr/share/doc/nodejs/copyright'] : [])]
  .filter(Boolean).find(filename => fs.existsSync(filename) && fs.statSync(filename).isFile());
if (!nodeLicense) throw new Error('Node distribution license is missing. Set OMARCHY_NODE_LICENSE to the license shipped with this exact Node runtime.');
fs.mkdirSync(output, { recursive: true });
const executable = process.platform === 'win32' ? 'node.exe' : 'node';
fs.copyFileSync(process.execPath, path.join(output, executable));
if (process.platform !== 'win32') fs.chmodSync(path.join(output, executable), 0o755);
fs.copyFileSync(nodeLicense, path.join(output, 'NODE-LICENSE.txt'));
// Give compiled CommonJS files their own package scope; a parent application's
// package.json may declare type=module when testing or deploying this runtime.
fs.copyFileSync(path.join(root, 'package.json'), path.join(output, 'package.json'));
fs.cpSync(path.join(root, 'dist'), path.join(output, 'dist'), { recursive: true, dereference: true });
// Keep the locked production tree and its runtime assets. Native binaries and
// package metadata (including binding.gyp, used by module-root discovery) stay.
// Compiler outputs, headers, source maps and TS declarations are build inputs.
const developmentPackages = new Set(Object.entries(lock.packages)
  .filter(([name, pkg]) => name && pkg.dev === true).map(([name]) => name));
const buildOnly = /\.(?:pdb|obj|lib|iobj|ipdb|ilk|exp|tlog|h|hpp|map|d\.ts)$/i;
const omitted = { developmentPackages: [...developmentPackages].sort(), buildFiles: 0, buildBytes: 0 };
fs.cpSync(path.join(root, 'node_modules'), path.join(output, 'node_modules'), {
  recursive: true, dereference: true,
  filter(filename) {
    const local = path.relative(root, filename).split(path.sep).join('/');
    if (developmentPackages.has(local) || local.split('/').includes('.bin') || local.endsWith('/.package-lock.json') || path.basename(filename).startsWith('.omarchy-')) return false;
    if (buildOnly.test(filename) && fs.statSync(filename).isFile()) {
      omitted.buildFiles++; omitted.buildBytes += fs.statSync(filename).size;
      return false;
    }
    return true;
  },
});
fs.mkdirSync(path.join(output, 'scripts'));
fs.copyFileSync(path.join(root, 'scripts', 'windows-volume-lock.ps1'), path.join(output, 'scripts', 'windows-volume-lock.ps1'));
fs.copyFileSync(path.join(root, 'package-lock.json'), path.join(output, 'package-lock.json'));
fs.copyFileSync(path.join(root, 'PHYSICAL-PROVIDER.md'), path.join(output, 'PHYSICAL-PROVIDER.md'));
// Isolate the read-only discovery dependency closure from the writing SDK.
// Re-audit this closure when changing the pinned drivelist version.
const inspection = path.join(output, 'inspection');
fs.mkdirSync(path.join(inspection, 'dist'), { recursive: true });
fs.writeFileSync(path.join(inspection, 'package.json'), JSON.stringify({ private: true, type: 'commonjs' }));
for (const name of ['inspection-cli', 'physical-discovery', 'physical-tools', 'physical-linux', 'physical-macos', 'physical-contracts', 'safety', 'contracts']) {
  fs.copyFileSync(path.join(root, 'dist', name + '.js'), path.join(inspection, 'dist', name + '.js'));
}
if (require(path.join(root, 'node_modules/drivelist/package.json')).version !== '12.0.2') throw new Error('Re-audit inspector dependencies for the new drivelist version.');
for (const name of ['drivelist', 'bindings', 'file-uri-to-path', '@balena/apple-plist', 'sax']) {
  fs.cpSync(path.join(root, 'node_modules', name), path.join(inspection, 'node_modules', name), {
    recursive: true, dereference: true, filter: filename => !buildOnly.test(filename),
  });
}
const files = [];
function walk(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const filename = path.join(directory, entry.name);
    if (entry.isDirectory()) walk(filename);
    else if (entry.isFile()) {
      const bytes = fs.readFileSync(filename);
      files.push({ path: path.relative(output, filename).split(path.sep).join('/'), length: bytes.length,
        sha256: createHash('sha256').update(bytes).digest('hex') });
    } else throw new Error('Staging only accepts ordinary files and directories.');
  }
}
walk(output);
const manifest = { protocol: 1, engine: 'etcher-sdk', sdkVersion: sdk.version, platform: process.platform,
  arch: process.arch, nodeVersion: process.version, nodeModuleAbi: process.versions.modules,
  model: 'gpt-6-astra', executable, entrypoint: 'dist/physical-cli.js', inspection_entrypoint: 'inspection/dist/inspection-cli.js', hardwareQualified: false,
  lockfileSha256: createHash('sha256').update(fs.readFileSync(path.join(root, 'package-lock.json'))).digest('hex'),
  staging: { policy: 'production-without-build-artifacts-v1', omitted },
  packages: Object.entries(lock.packages).filter(([name, pkg]) => name && !pkg.dev).map(([name, pkg]) => ({
    path: name, version: pkg.version, license: pkg.license ?? null, integrity: pkg.integrity ?? null })), files };
fs.writeFileSync(path.join(output, 'runtime-manifest.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' });
process.stdout.write(JSON.stringify({ output, manifest: 'runtime-manifest.json', files: files.length,
  node: process.version, platform: process.platform, arch: process.arch, hardwareQualified: false }) + '\n');
