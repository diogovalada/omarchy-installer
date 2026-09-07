import { format } from 'node:util';
import type { PhysicalCommand, PhysicalEvent } from './physical-contracts.js';

// Reserve stdout for protocol messages before loading any SDK/native dependencies.
delete process.env.MOUNTUTILS_DEBUG;
delete process.env.DEBUG;
for (const method of ['log', 'info', 'warn', 'error', 'debug'] as const) {
  console[method] = (...args: unknown[]) => { process.stderr.write(format(...args) + '\n'); };
}

let started = false;
let writeActive = false;
let terminal = false;
let possiblyModified = false;
let input = '';
let parentMonitor: NodeJS.Timeout | undefined;
let lastStage = '';
let lastProgress = 0;
const parentPid = process.ppid;

function emit(event: PhysicalEvent): void {
  if (terminal) return;
  if (event.type === 'event') {
    const now = Date.now();
    if (event.stage === lastStage && event.bytes !== undefined && event.bytes !== event.totalBytes && now - lastProgress < 100) return;
    lastStage = event.stage; lastProgress = now;
  } else terminal = true;
  process.stdout.write(JSON.stringify(event) + '\n');
}

function fail(code: string, message: string, exitCode = 1): never {
  emit({ protocol: 1, type: 'error', code, message, possiblyModified });
  // This is the SDK cancellation boundary: process termination releases the raw
  // descriptor. The Windows volume-lock child observes stdin EOF and releases its
  // handles. The native caller must wait for this process exit before acknowledging.
  process.exit(exitCode);
}
function abort(message: string): never { return fail('CANCELLED', message, 2); }
process.on('SIGTERM', () => abort('Provider terminated. The USB may contain an incomplete image.'));
process.on('SIGINT', () => abort('Provider interrupted. The USB may contain an incomplete image.'));
process.stdout.on('error', () => process.exit(2));
process.stdin.on('error', () => { if (!terminal) abort('Parent input pipe failed.'); });
process.stdin.on('end', () => {
  if (!started) fail('INVALID_PROTOCOL', 'A newline-terminated JSON command is required.');
  if (writeActive && !terminal) abort('Parent closed the lifetime pipe. The USB may contain an incomplete image.');
});

function keysOnly(value: Record<string, unknown>, allowed: string[]): boolean {
  return Object.keys(value).every(key => allowed.includes(key));
}
async function execute(value: unknown): Promise<void> {
  try {
    if (!value || typeof value !== 'object' || Array.isArray(value)) fail('INVALID_PROTOCOL', 'Expected a JSON command object.');
    const command = value as PhysicalCommand;
    if (command.protocol !== 1 || !['probe', 'list', 'write'].includes(command.action)) fail('INVALID_PROTOCOL', 'Only protocol 1 probe, list and write actions are accepted.');
    const fields = command.action === 'write' ? ['protocol', 'action', 'sourcePath', 'length', 'sha256', 'target', 'sourceVerification'] :
      command.action === 'list' ? ['protocol', 'action', 'sourcePath'] : ['protocol', 'action'];
    if (!keysOnly(command as unknown as Record<string, unknown>, fields)) fail('INVALID_PROTOCOL', 'Unknown command fields are not accepted.');
    if (command.action === 'write') {
      writeActive = true;
      if (!command.target || !keysOnly(command.target as unknown as Record<string, unknown>,
        ['fingerprint', 'device', 'raw', 'devicePath', 'hardwareId', 'size', 'blockSize', 'logicalBlockSize', 'description', 'busType'])) fail('INVALID_TARGET', 'Only discovered identity fields are accepted.');
      parentMonitor = setInterval(() => {
        if (process.ppid !== parentPid || process.ppid <= 1) abort('Parent exited while writing.');
        try { process.kill(parentPid, 0); }
        catch (error) { if ((error as NodeJS.ErrnoException).code === 'ESRCH') abort('Parent exited while writing.'); }
      }, 1000);
      const { runPhysicalWrite } = await import('./physical-engine.js');
      const receipt = await runPhysicalWrite(command, emit, () => { possiblyModified = true; },
        error => fail('LOCKING_LOST', error.message));
      emit({ protocol: 1, type: 'result', action: 'write', result: receipt });
    } else {
      const { inventory, probePhysical } = await import('./physical-discovery.js');
      if (command.action === 'probe') emit({ protocol: 1, type: 'result', action: 'probe', result: await probePhysical() });
      else emit({ protocol: 1, type: 'result', action: 'list', result: { drives: (await inventory(command.sourcePath)).map(d => d.public) } });
    }
    if (parentMonitor) clearInterval(parentMonitor);
    process.stdin.pause();
    process.stdout.write('', () => process.exit(0));
  } catch (error) {
    const e = error as Error & { code?: string };
    fail(e.code ?? 'PROVIDER_ERROR', e.message);
  }
}

process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk: string) => {
  input += chunk;
  if (Buffer.byteLength(input) > 64 * 1024) fail('INVALID_PROTOCOL', 'Command input exceeds 64 KiB.');
  let newline: number;
  while ((newline = input.indexOf('\n')) >= 0) {
    const line = input.slice(0, newline); input = input.slice(newline + 1);
    let value: unknown;
    try { value = JSON.parse(line); } catch { fail('INVALID_PROTOCOL', 'Input must contain one JSON object per line.'); }
    if (started) {
      const abortCommand = value as Record<string, unknown> | null;
      if (abortCommand && abortCommand.protocol === 1 && abortCommand.action === 'abort' && keysOnly(abortCommand, ['protocol', 'action'])) {
        abort('Cancellation requested. The USB may contain an incomplete image.');
      }
      fail('INVALID_PROTOCOL', 'Only an abort command is accepted after the initial command.');
    }
    started = true;
    // Set before the first await so same-chunk EOF aborts a write, even during import.
    writeActive = (value as { action?: string })?.action === 'write';
    void execute(value);
  }
});
