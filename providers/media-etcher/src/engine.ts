import { createHash } from 'node:crypto';
import type { FileHandle } from 'node:fs/promises';
import { File } from 'etcher-sdk/build/source-destination/file';
import { pipeSourceToDestinations } from 'etcher-sdk/build/multi-write';
import { FileWriteError, type FileWriteEvent, type FileWriteReceipt, type FileWriteRequest } from './contracts.js';
import { createDestination, openSource } from './safety.js';

/** SDK streams use these already checked handles; SDK never reopens caller paths. */
class OpenHandleFile extends File {
  constructor(filename: string, handle: FileHandle, write: boolean) {
    super({ path: filename, write });
    this.fileHandle = handle;
  }
  protected override async _open(): Promise<void> { /* owned by runFileWrite */ }
  protected override async _close(): Promise<void> { /* owned by runFileWrite */ }
  protected override async _getMetadata() {
    return { size: (await this.fileHandle.stat()).size, name: 'raw-file.img' };
  }
}

async function digest(handle: FileHandle, size: number, progress?: (bytes: number) => void): Promise<string> {
  const hash = createHash('sha256');
  const buffer = Buffer.alloc(1024 * 1024);
  let position = 0;
  while (position < size) {
    const { bytesRead } = await handle.read(buffer, 0, Math.min(buffer.length, size - position), position);
    if (!bytesRead) throw new FileWriteError('SHORT_READ', 'File ended before the expected byte length.');
    hash.update(buffer.subarray(0, bytesRead));
    position += bytesRead;
    progress?.(position);
  }
  if ((await handle.stat()).size !== size) {
    throw new FileWriteError('SIZE_CHANGED', 'File size differs from the expected image length.');
  }
  return hash.digest('hex');
}

// Worker-internal entry point. Not exported by the package's public adapter.
export async function runFileWrite(request: FileWriteRequest, emit: (event: FileWriteEvent) => void): Promise<FileWriteReceipt> {
  emit({ type: 'validating' });
  const source = await openSource(request);
  let destination: FileHandle | undefined;
  try {
    const expected = request.expectedSha256.toLowerCase();
    if (await digest(source.handle, source.stat.size) !== expected) {
      throw new FileWriteError('SOURCE_DIGEST_MISMATCH', 'Source bytes do not match the required SHA-256 digest.');
    }
    destination = await createDestination(request.destinationPath);
    const sdkSource = new OpenHandleFile(request.sourcePath, source.handle, false);
    const sdkDestination = new OpenHandleFile(request.destinationPath, destination, true);
    emit({ type: 'writing', bytes: 0, totalBytes: source.stat.size });
    const result = await pipeSourceToDestinations({
      source: sdkSource,
      destinations: [sdkDestination],
      verify: true,
      numBuffers: 2,
      onFail: () => { /* failures are inspected before any success receipt */ },
      onProgress: progress => {
        if (progress.type === 'flashing' || progress.type === 'verifying') {
          emit({ type: progress.type === 'flashing' ? 'writing' : 'verifying',
            bytes: progress.position, totalBytes: source.stat.size });
        }
      },
    });
    if (result.failures.size) {
      throw new FileWriteError('SDK_WRITE_OR_VERIFY_FAILED', [...result.failures.values()].map(e => e.message).join('; '));
    }
    await destination.sync();
    emit({ type: 'readback', bytes: 0, totalBytes: source.stat.size });
    const sha256 = await digest(destination, source.stat.size, bytes =>
      emit({ type: 'readback', bytes, totalBytes: source.stat.size }));
    if (sha256 !== expected) throw new FileWriteError('READBACK_MISMATCH', 'Destination SHA-256 differs from the verified source.');
    // Receipt refers to the bytes supplied to the engine; detect concurrent source changes as well.
    if (await digest(source.handle, source.stat.size) !== expected) {
      throw new FileWriteError('SOURCE_CHANGED', 'Source changed during the operation.');
    }
    return { engine: 'etcher-sdk', engineVersion: '10.2.14', destinationKind: 'regular-file',
      bytesWritten: source.stat.size, sha256, sdkVerification: true, fullReadbackVerification: true };
  } finally {
    await Promise.all([source.handle.close(), destination?.close()]);
  }
}
