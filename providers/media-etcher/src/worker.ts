import type { FileWriteRequest, FileWriteEvent } from './contracts.js';

function send(event: FileWriteEvent) { process.send?.(event); }

process.once('message', async (request: FileWriteRequest) => {
  try {
    // Loading the real SDK is part of each worker run, not replaced by a fake writer.
    const { runFileWrite } = await import('./engine.js');
    const receipt = await runFileWrite(request, send);
    send({ type: 'completed', receipt });
    process.disconnect?.();
  } catch (error) {
    const failure = error as Error & { code?: string };
    send({ type: 'error', code: failure.code ?? 'ENGINE_ERROR', message: failure.message });
    process.exitCode = 1;
    process.disconnect?.();
  }
});
