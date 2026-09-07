// Fixed read-only launcher smoke test. Uses only the files extracted beside it.
import { readFileSync, lstatSync } from 'node:fs';
import { dirname, join, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
const root = dirname(fileURLToPath(import.meta.url));
const bundle = JSON.parse(readFileSync(join(root, 'bundle-files.json'), 'utf8'));
if (bundle.schemaVersion !== 1 || !Array.isArray(bundle.files) || !bundle.files.length || bundle.files.length > 50000) throw new Error('Invalid bundle file list.');
const seen = new Set();
for (const file of bundle.files) {
  if (typeof file.path !== 'string' || isAbsolute(file.path) || /[:\\\0]/.test(file.path) || file.path.split('/').some(part => !part || part === '.' || part === '..' || /[. ]$/.test(part))) throw new Error('Invalid bundle path.');
  const key = file.path.toLowerCase();
  if (seen.has(key)) throw new Error('Duplicate bundle path.');
  seen.add(key);
  let path = root;
  for (const part of file.path.split('/')) {
    path = join(path, part);
    if (lstatSync(path).isSymbolicLink()) throw new Error('Bundle links are not permitted.');
  }
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.size !== file.sizeBytes || stat.size > 256 * 1024 * 1024) throw new Error('Bundle file size mismatch.');
  if (createHash('sha256').update(readFileSync(path)).digest('hex') !== file.sha256) throw new Error('Bundle file hash mismatch.');
}
if (!seen.has('omarchy setup.exe') || !seen.has('providers/image-builder-x86/runtime.tar')) throw new Error('Required payload is absent.');
console.log(`Verified ${bundle.files.length} portable payload files.`);
