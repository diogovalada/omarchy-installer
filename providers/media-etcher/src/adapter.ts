import { spawn } from 'node:child_process';
import { join } from 'node:path';
import { FileWriteError, type FileWriteEvent, type FileWriteReceipt, type FileWriteRequest } from './contracts.js';
import { validateRequest } from './safety.js';
export { FileWriteError } from './contracts.js';
export type { FileWriteEvent, FileWriteReceipt, FileWriteRequest } from './contracts.js';

export interface FileWriteOptions {
  signal?: AbortSignal;
  onEvent?: (event: FileWriteEvent) => void;
}

/** File-only feasibility API. Success/cancellation is reported only after worker exit. */
export async function writeVerifiedTestFile(request: FileWriteRequest, options: FileWriteOptions = {}): Promise<FileWriteReceipt> {
  // Observer failures must not become uncaught exceptions in a stream callback.
  const emit = (event: FileWriteEvent) => { try { options.onEvent?.(event); } catch { /* observer only */ } };
  let payload: FileWriteRequest;
  try {
    if (!request || typeof request !== 'object') {
      throw new FileWriteError('INVALID_REQUEST', 'A file write request is required.');
    }
    // Snapshot exactly the public fields before validation. Structural typing
    // permits caller metadata, getters and toJSON methods; none belong on IPC.
    payload = { sourcePath: request.sourcePath, destinationPath: request.destinationPath,
      expectedSha256: request.expectedSha256 };
    validateRequest(payload);
  } catch (error) {
    const failure = error as FileWriteError;
    emit({ type: 'error', code: failure.code, message: failure.message });
    throw error;
  }
  if (options.signal?.aborted) {
    emit({ type: 'cancelled', message: 'Cancelled before starting; no worker was created.' });
    throw new FileWriteError('CANCELLED', 'Cancelled before starting.');
  }
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [join(__dirname, 'worker.js')], {
      windowsHide: true, stdio: ['ignore', 'ignore', 'pipe', 'ipc'],
    });
    let terminal: Extract<FileWriteEvent, { type: 'completed' | 'error' }> | undefined;
    let cancelled = false;
    let stderr = '';
    child.stderr?.on('data', chunk => { stderr = (stderr + String(chunk)).slice(-4096); });
    const cancel = () => { cancelled = true; child.kill(); };
    options.signal?.addEventListener('abort', cancel, { once: true });
    child.on('message', (event: FileWriteEvent) => {
      if (cancelled) return;
      if (event.type === 'completed' || event.type === 'error') terminal = event;
      else emit(event);
    });
    child.once('error', error => {
      terminal = { type: 'error', code: 'WORKER_ERROR', message: error.message };
    });
    child.once('close', code => {
      options.signal?.removeEventListener('abort', cancel);
      if (cancelled) {
        emit({ type: 'cancelled', message: 'Worker stopped. A partial destination file may remain.' });
        reject(new FileWriteError('CANCELLED', 'Worker stopped; a partial destination file may remain.'));
      } else if (terminal?.type === 'completed' && code === 0) {
        emit(terminal);
        resolve(terminal.receipt);
      } else {
        const failure = terminal?.type === 'error' ? terminal : { type: 'error' as const,
          code: 'WORKER_EXIT', message: `Etcher worker exited without a verified result (${code}). ${stderr}`.trim() };
        emit(failure);
        reject(new FileWriteError(failure.code, failure.message));
      }
    });
    const failSend = (error: Error) => {
      if (!cancelled) {
        terminal = { type: 'error', code: 'WORKER_IPC', message: error.message };
        child.kill();
      }
    };
    if (options.signal?.aborted) cancel();
    else {
      try {
        child.send(payload, error => { if (error) failSend(error); });
      } catch (error) {
        // child.send can throw synchronously. Still wait for close before the
        // existing terminal handler emits an error and rejects the operation.
        failSend(error instanceof Error ? error : new Error(String(error)));
      }
    }
  });
}
