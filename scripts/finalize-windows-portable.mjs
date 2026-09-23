// Verify the fixed build output, migrate any last legacy build, then discard old packages.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const outputParent = join(root, 'artifacts/windows-portable');
const payloadParent = join(root, 'artifacts/p');
const packageId = process.argv[2];
assert.match(packageId ?? '', /^(release|testing|[a-f0-9]{8})$/, 'Expected release, testing, or a legacy package ID.');
assert.ok(process.argv.length === 3 || (process.argv.length === 4 && process.argv[3] === '--dry-run'), 'Unsupported finalization arguments.');
const dryRun = process.argv[3] === '--dry-run';

const names = {
  release: 'Omarchy-Installer-0.1.0-x64-portable.exe',
  testing: 'Omarchy-Installer-0.1.0-x64-testing.exe',
};
const sha256 = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const isOldOutput = name => /^[a-f0-9]{8}$/.test(name) || /^preview-[a-f0-9]{12}-[a-f0-9]{8}$/.test(name);

function directory(parent, name) {
  assert.equal(basename(name), name);
  const path = join(parent, name);
  const stat = lstatSync(path);
  assert.ok(stat.isDirectory() && !stat.isSymbolicLink(), `Unsafe package directory: ${path}`);
  return path;
}

function candidate(name) {
  const path = directory(outputParent, name);
  const recordPath = join(path, 'portable-record.json');
  if (!existsSync(recordPath) || lstatSync(recordPath).isSymbolicLink()) return null;
  let record;
  try { record = JSON.parse(readFileSync(recordPath, 'utf8')); } catch { return null; }
  if (record.format !== 'portable-executable' || record.launcher?.extractionAndHashesVerified !== true) return null;
  const kind = record.testingBuild === true ? 'testing' : 'release';
  const exe = join(path, names[kind]);
  if (!existsSync(exe) || !lstatSync(exe).isFile() || lstatSync(exe).isSymbolicLink()) return null;
  return { name, path, record, recordPath, kind, exe, created: Date.parse(record.createdAt) || lstatSync(path).mtimeMs };
}

const staged = candidate(packageId);
assert.ok(staged, 'The staged package must be complete and verified before promotion.');
assert.equal(staged.record.launcher.path, staged.exe, 'The staged launcher path does not match the package.');

const entries = readdirSync(outputParent, { withFileTypes: true })
  .filter(entry => entry.isDirectory() && (isOldOutput(entry.name) || entry.name === 'release' || entry.name === 'testing'));
const candidates = entries.map(entry => candidate(entry.name)).filter(Boolean);
const chosen = {
  [staged.kind]: staged,
  [staged.kind === 'release' ? 'testing' : 'release']: candidates
    .filter(item => item.kind !== staged.kind)
    .sort((a, b) => b.created - a.created)[0],
};

function verify(item) {
  assert.ok(item.record.launcher.sha256 && sha256(item.exe) === item.record.launcher.sha256,
    `Launcher checksum mismatch: ${item.exe}`);
  assert.equal(lstatSync(item.exe).size, item.record.launcher.sizeBytes);
}

// Verify both keepers before changing either destination.
for (const item of Object.values(chosen).filter(Boolean)) verify(item);
if (dryRun) {
  console.log(JSON.stringify({
    keep: Object.fromEntries(Object.entries(chosen).map(([kind, item]) => [kind, item?.name ?? null])),
    oldOutputs: entries.filter(entry => isOldOutput(entry.name) && entry.name !== chosen.release?.name && entry.name !== chosen.testing?.name).length,
    oldPayloads: existsSync(payloadParent) ? readdirSync(payloadParent).filter(name => /^(release|testing|[a-f0-9]{8})$/.test(name)).length : 0,
  }));
  process.exit(0);
}

function promote(item) {
  if (!item || item.name === item.kind) return;
  const target = join(outputParent, item.kind);
  if (existsSync(target)) {
    directory(outputParent, item.kind);
    rmSync(target, { recursive: true });
  }
  renameSync(item.path, target);
  item.record.launcher.path = join(target, names[item.kind]);
  writeFileSync(join(target, 'portable-record.json'), JSON.stringify(item.record, null, 2) + '\n');
}

promote(chosen[staged.kind === 'release' ? 'testing' : 'release']);
promote(staged);

// These two parents contain generated packages only. Leave unknown names alone.
for (const entry of readdirSync(outputParent, { withFileTypes: true })) {
  if (isOldOutput(entry.name) && entry.isDirectory()) {
    try { rmSync(directory(outputParent, entry.name), { recursive: true }); }
    catch (error) { console.warn(`Could not remove old package ${entry.name}: ${error.message}`); }
  }
}
if (existsSync(payloadParent)) {
  for (const entry of readdirSync(payloadParent, { withFileTypes: true })) {
    if (/^(release|testing|[a-f0-9]{8})$/.test(entry.name) && entry.isDirectory()) {
      try { rmSync(directory(payloadParent, entry.name), { recursive: true }); }
      catch (error) { console.warn(`Could not remove old payload ${entry.name}: ${error.message}`); }
    }
  }
}
console.log(join(outputParent, staged.kind, names[staged.kind]));
