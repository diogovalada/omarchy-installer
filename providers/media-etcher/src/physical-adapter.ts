import { spawn } from 'node:child_process';
import { join } from 'node:path';
import { PhysicalWriteError, type PhysicalCommand, type PhysicalEvent, type PhysicalDrive,
  type PhysicalProbe, type PhysicalWriteReceipt, type PhysicalWriteRequest } from './physical-contracts.js';
export * from './physical-contracts.js';

export interface PhysicalOptions { signal?: AbortSignal; onEvent?: (event: PhysicalEvent) => void }

/** Ordinary-process API; privileged callers must independently establish artifact
 * authenticity, trustworthy runtime files and an immutable source directory.
 * Terminal events are delivered only after the provider child has exited.
 */
async function invoke(command: PhysicalCommand, options: PhysicalOptions = {}): Promise<PhysicalEvent> {
  if (options.signal?.aborted) throw new PhysicalWriteError('CANCELLED', 'Cancelled before provider launch.');
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [join(__dirname, 'physical-cli.js')], {
      windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env, NODE_PATH: '', NODE_OPTIONS: '' },
    });
    let pending = '';
    let stderr = '';
    let terminal: PhysicalEvent | undefined;
    let cancelled = false;
    let forceStop: NodeJS.Timeout | undefined;
    const emit = (event: PhysicalEvent) => { try { options.onEvent?.(event); } catch { /* observer */ } };
    const stop = () => {
      cancelled = true;
      child.stdin.write('{"protocol":1,"action":"abort"}\n');
      forceStop = setTimeout(() => child.kill('SIGKILL'), 3000);
    };
    options.signal?.addEventListener('abort', stop, { once: true });
    child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-4096); });
    child.stdin.on('error', () => { /* final result/exit is authoritative */ });
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (chunk: string) => {
      pending += chunk;
      if (pending.length > 1024 * 1024) { terminal = { protocol: 1, type: 'error', code: 'PROVIDER_PROTOCOL', message: 'Provider response exceeded its bound.', possiblyModified: command.action === 'write' }; child.kill(); return; }
      let newline: number;
      while ((newline = pending.indexOf('\n')) >= 0) {
        const line = pending.slice(0, newline); pending = pending.slice(newline + 1);
        try {
          const event: PhysicalEvent = JSON.parse(line);
          if (event.protocol !== 1 || !['event', 'error', 'result'].includes(event.type)) throw new Error('Invalid event');
          if (event.type === 'event') { if (!cancelled) emit(event); }
          else terminal = event;
        } catch { terminal = { protocol: 1, type: 'error', code: 'PROVIDER_PROTOCOL', message: 'Malformed provider event.', possiblyModified: command.action === 'write' }; child.kill(); }
      }
    });
    child.once('error', error => { terminal = { protocol: 1, type: 'error', code: 'PROVIDER_LAUNCH', message: error.message, possiblyModified: false }; });
    child.once('close', code => {
      options.signal?.removeEventListener('abort', stop);
      if (forceStop) clearTimeout(forceStop);
      if (cancelled) terminal = { protocol: 1, type: 'error', code: 'CANCELLED', message: 'Provider stopped. The USB may contain an incomplete image.', possiblyModified: command.action === 'write' };
      if (!cancelled && code === 0 && terminal?.type === 'result' && terminal.action === command.action && !pending.trim()) {
        emit(terminal); resolve(terminal);
      } else {
        const event: Extract<PhysicalEvent, { type: 'error' }> = terminal?.type === 'error' ? terminal : {
          protocol: 1, type: 'error', code: 'PROVIDER_EXIT', message: `Provider exited without a verified result (${code}). ${stderr}`.trim(), possiblyModified: command.action === 'write' };
        emit(event); reject(new PhysicalWriteError(event.code, event.message));
      }
    });
    try { child.stdin.write(JSON.stringify(command) + '\n'); }
    catch (error) { terminal = { protocol: 1, type: 'error', code: 'INVALID_REQUEST', message: (error as Error).message, possiblyModified: false }; child.kill(); }
    if (options.signal?.aborted) stop();
  });
}

export async function probeUsbProvider(): Promise<PhysicalProbe> {
  const event = await invoke({ protocol: 1, action: 'probe' });
  if (event.type !== 'result' || event.action !== 'probe') throw new PhysicalWriteError('PROVIDER_PROTOCOL', 'Expected probe result.');
  return event.result;
}
export async function listUsbDrives(sourcePath?: string): Promise<PhysicalDrive[]> {
  const event = await invoke({ protocol: 1, action: 'list', ...(sourcePath ? { sourcePath } : {}) });
  if (event.type !== 'result' || event.action !== 'list') throw new PhysicalWriteError('PROVIDER_PROTOCOL', 'Expected drive list.');
  return event.result.drives;
}
export async function writeVerifiedUsb(request: PhysicalWriteRequest, options?: PhysicalOptions): Promise<PhysicalWriteReceipt> {
  const event = await invoke(request, options);
  if (event.type !== 'result' || event.action !== 'write') throw new PhysicalWriteError('PROVIDER_PROTOCOL', 'Expected write receipt.');
  return event.result;
}
