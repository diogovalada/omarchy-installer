// Reproducible, hash-guarded extension of the pinned native dependency.
// npm ci restores upstream; every Windows build reapplies and compiles it.
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { execFileSync } = require('node:child_process');
if (process.platform !== 'win32') process.exit(0);
const root = path.resolve(__dirname, '..');
const moduleRoot = path.join(root, 'node_modules/@ronomon/direct-io');
const sourcePath = path.join(moduleRoot, 'binding.c');
const backup = path.join(moduleRoot, '.omarchy-upstream-binding.c');
const statePath = path.join(moduleRoot, '.omarchy-build.json');
const hash = value => createHash('sha256').update(value).digest('hex');
const upstreamHash = '83c85428f985e168aa0eca4ec52249cc3e6ea1e638210eeb15a5497e2a2f4d6a';
if (JSON.parse(fs.readFileSync(path.join(moduleRoot, 'package.json'))).version !== '3.0.1') throw new Error('Re-audit the geometry extension for the new direct-io version.');
const current = fs.readFileSync(sourcePath, 'utf8');
const original = hash(current) === upstreamHash ? current : fs.readFileSync(backup, 'utf8');
if (hash(original) !== upstreamHash) throw new Error('Unexpected direct-io upstream source; refusing to patch.');
const header = fs.readFileSync(path.join(__dirname, 'native/windows-geometry.h'), 'utf8');
let patched = original.replace('  const char* error;\n', '  const char* error;\n  char geometry_error[160];\n  int64_t alignment_query_error;\n');
patched = patched.replace('void task_assert(', '#if defined(_WIN32)\n' + header + '\n#endif\n\nvoid task_assert(');
patched = patched.replace('    set_int(env, argv[1], "size", task->device_size);', '    set_int(env, argv[1], "size", task->device_size);\n    set_int(env, argv[1], "alignmentQueryError", task->alignment_query_error);');
const start = patched.indexOf('#elif defined(_WIN32)', patched.indexOf('void task_execute_get_block_device_size('));
const end = patched.indexOf('#elif defined(__FreeBSD__)', start);
if (start < 0 || end < 0) throw new Error('Expected upstream Windows geometry implementation.');
patched = patched.slice(0, start) + '#elif defined(_WIN32)\n  omarchy_windows_geometry(task);\n' + patched.slice(end);
patched = patched.replace('  return exports;\n', '  set_int(env, exports, "omarchyGeometryVersion", 1);\n  return exports;\n');
const binary = path.join(moduleRoot, 'binding.node');
const expected = hash(patched + process.version + process.arch);
let previous;
try { previous = JSON.parse(fs.readFileSync(statePath)); } catch {}
if (current === patched && previous?.source === expected && fs.existsSync(binary) && previous.binary === hash(fs.readFileSync(binary))) process.exit(0);
if (hash(current) !== upstreamHash && current !== patched && previous?.patched !== hash(current)) throw new Error('Unexpected modified direct-io source; refusing to overwrite it.');
fs.writeFileSync(backup, original);
fs.writeFileSync(sourcePath, patched);
// Run npm through PowerShell with fixed arguments; Windows .cmd shims cannot
// be execFile'd directly. No user-controlled strings are shell source.
execFileSync(path.join(process.env.SystemRoot, 'System32/WindowsPowerShell/v1.0/powershell.exe'),
  ['-NoProfile', '-NonInteractive', '-Command', 'npm rebuild @ronomon/direct-io; exit $LASTEXITCODE'],
  { cwd: root, stdio: 'inherit', windowsHide: true });
const io = require(binary);
if (io.omarchyGeometryVersion !== 1) throw new Error('Native geometry extension was not built.');
fs.writeFileSync(statePath, JSON.stringify({ source: expected, patched: hash(patched), binary: hash(fs.readFileSync(binary)) }));
