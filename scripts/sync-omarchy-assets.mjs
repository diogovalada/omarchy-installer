#!/usr/bin/env node
// Explicit, reviewable updates of the small asset bundle; never runs during app startup.
import { createHash } from 'node:crypto';
import { readFile, writeFile, rename } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const assetDir = resolve(root, 'apps/desktop/src/assets/omarchy');
const lockPath = resolve(assetDir, 'sources.json');
const specs = [
  ['omarchy', 'logo.svg', 'logo.svg'],
  ['omarchy', 'icon.png', 'icon.png'],
  ['omarchy', 'themes/tokyo-night/preview.png', 'desktop-preview.png'],
  ['omarchy', 'themes/tokyo-night/backgrounds/1-quattro.webp', 'quattro.webp'],
  ['omarchy', 'themes/tokyo-night/backgrounds/0-winding-road.webp', 'winding-road.webp'],
  ['omarchy', 'LICENSE', 'LICENSE-Omarchy.txt'],
  ['fonts', 'fonts/webfonts/JetBrainsMono-Regular.woff2', 'JetBrainsMono-Regular.woff2'],
  ['fonts', 'fonts/webfonts/JetBrainsMono-Bold.woff2', 'JetBrainsMono-Bold.woff2'],
  ['fonts', 'OFL.txt', 'OFL-JetBrainsMono.txt'],
];
const repos = { omarchy: 'omacom/omarchy', fonts: 'JetBrains/JetBrainsMono' };
const digest = bytes => createHash('sha256').update(bytes).digest('hex');

function parseArgs(args) {
  if (args.length === 1 && args[0] === '--check') return { check: true };
  const result = {};
  for (let i = 0; i < args.length; i += 2) {
    const key = { '--ref': 'omarchy', '--font-ref': 'fonts' }[args[i]];
    if (!key || !args[i + 1] || args[i + 1].startsWith('--') || result[key]) {
      throw new Error('Usage: pnpm assets:sync --ref <Omarchy commit/tag/branch> [--font-ref <font commit/tag>] OR pnpm assets:check');
    }
    result[key] = args[i + 1];
  }
  if (!result.omarchy) throw new Error('An explicit --ref is required; updates never silently follow a branch.');
  return result;
}

async function fetchBytes(url, maxBytes) {
  const response = await fetch(url, {
    redirect: 'error',
    headers: { 'User-Agent': 'omarchy-setup-assets', Accept: '*/*' },
    signal: AbortSignal.timeout(60000),
  });
  if (!response.ok || !response.body) throw new Error(`HTTP ${response.status}: ${url}`);
  const declared = response.headers.get('content-length');
  if (declared && Number(declared) > maxBytes) throw new Error(`Asset exceeds ${maxBytes} bytes: ${url}`);
  const chunks = [];
  let length = 0;
  for await (const chunk of response.body) {
    length += chunk.length;
    if (length > maxBytes) throw new Error(`Asset exceeds ${maxBytes} bytes: ${url}`);
    chunks.push(chunk);
  }
  return Buffer.concat(chunks, length);
}

async function resolveRevision(repo, ref) {
  if (/^[a-f0-9]{40}$/i.test(ref)) return ref.toLowerCase();
  const bytes = await fetchBytes(`https://api.github.com/repos/${repo}/commits/${encodeURIComponent(ref)}`, 2 * 1024 * 1024);
  const { sha } = JSON.parse(bytes.toString('utf8'));
  if (!/^[a-f0-9]{40}$/.test(sha ?? '')) throw new Error(`No exact revision resolved for ${repo}`);
  return sha;
}

function validateFormat(file, bytes) {
  if (!bytes.length) throw new Error(`Empty asset: ${file}`);
  const prefix = bytes.subarray(0, 16);
  const valid = file.endsWith('.png') ? prefix.subarray(0, 8).equals(Buffer.from([137,80,78,71,13,10,26,10]))
    : file.endsWith('.webp') ? prefix.toString('ascii', 0, 4) === 'RIFF' && prefix.toString('ascii', 8, 12) === 'WEBP'
    : file.endsWith('.woff2') ? prefix.toString('ascii', 0, 4) === 'wOF2'
    : file.endsWith('.svg') ? bytes.toString('utf8').includes('<svg') && !/<script\b|<foreignObject\b|\bon\w+\s*=/i.test(bytes.toString('utf8'))
    : file.startsWith('LICENSE') ? bytes.toString('utf8').includes('Permission is hereby granted')
    : bytes.toString('utf8').includes('SIL OPEN FONT LICENSE');
  if (!valid) throw new Error(`Unexpected content for ${file}; inspect upstream changes manually.`);
}

async function checkBundle(lock) {
  if (lock.schemaVersion !== 1 || lock.files.length !== specs.length) throw new Error('Unexpected asset lock structure');
  for (const [source, upstreamPath, file] of specs) {
    const pin = lock.sources[source];
    if (pin.repository !== repos[source] || !/^[a-f0-9]{40}$/.test(pin.revision)) throw new Error(`Invalid source pin: ${source}`);
    const entry = lock.files.find(value => value.file === file);
    const expectedUrl = `https://raw.githubusercontent.com/${repos[source]}/${pin.revision}/${upstreamPath}`;
    if (!entry || entry.url !== expectedUrl) throw new Error(`Unexpected source: ${file}`);
    const bytes = await readFile(resolve(assetDir, file));
    validateFormat(file, bytes);
    if (bytes.length !== entry.bytes || digest(bytes) !== entry.sha256) throw new Error(`Asset differs from its lock: ${file}`);
  }
  console.log(`Verified ${specs.length} bundled assets against exact upstream pins (offline).`);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const previous = JSON.parse((await readFile(lockPath, 'utf8')).replace(/^\uFEFF/, ''));
  if (options.check) return checkBundle(previous);
  const revisions = {
    omarchy: await resolveRevision(repos.omarchy, options.omarchy),
    fonts: await resolveRevision(repos.fonts, options.fonts ?? previous.sources.fonts.revision),
  };
  const files = [];
  // Fetch and validate the whole set before changing any existing asset.
  for (const [source, upstreamPath, file] of specs) {
    const url = `https://raw.githubusercontent.com/${repos[source]}/${revisions[source]}/${upstreamPath}`;
    const bytes = await fetchBytes(url, 8 * 1024 * 1024);
    validateFormat(file, bytes);
    files.push({ file, url, bytes: bytes.length, sha256: digest(bytes), content: bytes });
  }
  for (const file of files) {
    const target = resolve(assetDir, file.file);
    await writeFile(target + '.tmp', file.content);
    await rename(target + '.tmp', target);
  }
  const lock = {
    schemaVersion: 1,
    sources: Object.fromEntries(Object.entries(repos).map(([key, repository]) => [key, { repository, revision: revisions[key] }])),
    files: files.map(({ content, ...record }) => record),
  };
  await writeFile(lockPath + '.tmp', JSON.stringify(lock, null, 2) + '\n');
  await rename(lockPath + '.tmp', lockPath);
  await checkBundle(lock);
  const cli = resolve(root, 'apps/desktop/node_modules/@tauri-apps/cli/tauri.js');
  const result = spawnSync(process.execPath, [cli, 'icon', 'src/assets/omarchy/icon.png', '--output', 'src-tauri/icons'], {
    cwd: resolve(root, 'apps/desktop'), stdio: 'inherit', windowsHide: true,
  });
  if (result.error || result.status !== 0) throw new Error('Assets updated, but icon generation failed. Install workspace dependencies, then run the icon command documented in the asset README.');
  console.log('Assets and desktop icons updated. Review the diff, then run desktop checks and visual QA before shipping.');
}

main().catch(error => { console.error(error.message); process.exitCode = 1; });
