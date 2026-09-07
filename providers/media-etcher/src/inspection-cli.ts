// Deliberately read-only: this package excludes the SDK and physical writer.
import { inventory, supportedPlatform } from './physical-discovery.js';
import { writeSync } from 'node:fs';
import { format } from 'node:util';
for (const method of ['log', 'info', 'warn', 'error', 'debug'] as const) {
  console[method] = (...args: unknown[]) => { process.stderr.write(format(...args) + '\n'); };
}
let input = '';
let started = false;
function fail(message: string): never {
  writeSync(1, JSON.stringify({ protocol: 1, type: 'error', code: 'INSPECTION_FAILED', message, possiblyModified: false }) + '\n');
  process.exit(1);
}
process.stdout.on('error', () => process.exit(1));
process.stdin.setEncoding('utf8');
process.stdin.on('end', () => { if (!started) fail('A newline-terminated command is required.'); });
process.stdin.on('data', (chunk: string) => {
  input += chunk;
  if (Buffer.byteLength(input) > 64 * 1024) fail('Command exceeds 64 KiB.');
  if (!input.includes('\n')) return;
  if (started || input.slice(input.indexOf('\n') + 1).trim()) fail('Only one command is accepted.');
  started = true;
  process.stdin.pause();
  void (async () => {
    try {
      const command: unknown = JSON.parse(input);
      if (!command || typeof command !== 'object' || Array.isArray(command)) throw new Error('Expected a command object.');
      const value = command as Record<string, unknown>;
      if (value.protocol !== 1 || value.action !== 'list' || typeof value.sourcePath !== 'string' ||
          Object.keys(value).some(key => !['protocol', 'action', 'sourcePath'].includes(key))) throw new Error('Only read-only list requests are accepted.');
      supportedPlatform();
      const drives = (await inventory(value.sourcePath)).map(drive => drive.public);
      process.stdout.write(JSON.stringify({ protocol: 1, type: 'result', action: 'list', result: { drives } }) + '\n', () => process.exit(0));
    } catch (error) { fail((error as Error).message); }
  })();
});
