import { createHash } from 'node:crypto';
import { constants, type BigIntStats } from 'node:fs';
import { lstat, open, realpath, type FileHandle } from 'node:fs/promises';
import * as path from 'node:path';
import { PhysicalWriteError, type ParentHeldIso, type PhysicalWriteRequest } from './physical-contracts.js';
import { validatePathSyntax } from './safety.js';

export interface OpenedSource { handle: FileHandle; stat: BigIntStats }
function fail(code: string, message: string): never { throw new PhysicalWriteError(code, message); }

export async function openStableSource(request: PhysicalWriteRequest): Promise<OpenedSource> {
  validatePathSyntax(request.sourcePath);
  const resolved = path.resolve(request.sourcePath);
  let component = path.parse(resolved).root;
  for (const part of resolved.slice(component.length).split(path.sep).filter(Boolean)) {
    component = path.join(component, part);
    if ((await lstat(component)).isSymbolicLink()) fail('SYMLINK_REJECTED', 'Source symlinks and directory junctions are not accepted.');
  }
  if (await realpath(resolved) !== resolved && process.platform !== 'win32') fail('UNSAFE_SOURCE', 'Source path must be canonical.');
  const handle = await open(resolved, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  try {
    const stat = await handle.stat({ bigint: true });
    if (!stat.isFile() || stat.size !== BigInt(request.length)) fail('SOURCE_LENGTH', 'Source must be a regular file of the authenticated image length.');
    return { handle, stat };
  } catch (error) { await handle.close(); throw error; }
}

export async function assertSourceUnchanged(source: OpenedSource, binding?: ParentHeldIso): Promise<void> {
  const before = source.stat;
  const after = await source.handle.stat({ bigint: true });
  if (before.dev !== after.dev || before.ino !== after.ino || before.size !== after.size ||
      before.mtimeNs !== after.mtimeNs || before.ctimeNs !== after.ctimeNs) {
    fail('SOURCE_CHANGED', 'The opened source image changed.');
  }
  if (binding !== undefined) checkParentBinding(binding, source.stat);
}

function checkParentBinding(value: ParentHeldIso, stat: BigIntStats): void {
  if (process.platform !== 'win32' || !value || typeof value !== 'object' || Array.isArray(value) ||
      Object.keys(value).length !== 4 || Object.keys(value).some(key => !['kind', 'parentPid', 'volumeSerial', 'fileIndex'].includes(key)) ||
      value.kind !== 'windows-held-iso-v1' || !Number.isSafeInteger(value.parentPid) || value.parentPid <= 1 ||
      value.parentPid !== process.ppid || typeof value.volumeSerial !== 'string' ||
      !/^(0|[1-9][0-9]{0,9})$/.test(value.volumeSerial) || typeof value.fileIndex !== 'string' ||
      !/^[1-9][0-9]{0,19}$/.test(value.fileIndex)) {
    fail('INVALID_SOURCE_BINDING', 'The source verification handoff must come from the current Windows parent.');
  }
  // GetFileInformationByHandle supplies a 32-bit volume serial. Recent libuv
  // versions expose the full NT volume serial in st_dev; compare its low word.
  // Use bigint for the file index: a JS number can alias distinct NTFS files.
  if (BigInt(value.volumeSerial) !== (stat.dev & 0xffffffffn) || BigInt(value.fileIndex) !== stat.ino) {
    fail('SOURCE_IDENTITY_CHANGED', 'The opened image is not the file authenticated and held by the parent.');
  }
  try { process.kill(value.parentPid, 0); }
  catch { fail('SOURCE_PARENT_LOST', 'The parent retaining the verified image is unavailable.'); }
}

/** A live parent handoff is not a cache or a signature supplied by the webview.
 * The privileged parent authenticated this exact file while denying writes and
 * deletion, retains that guard until child exit, and keeps the lifetime pipe
 * open. The CLI aborts on pipe loss or parent exit. Standalone callers without
 * this Windows binding still hash the opened source before and after writing.
 */
export async function verifyPhysicalSource(source: OpenedSource, request: PhysicalWriteRequest,
  emitHash: (bytes: number) => void): Promise<void> {
  await assertSourceUnchanged(source, request.sourceVerification);
  if (request.sourceVerification !== undefined) {
    return;
  }
  const hash = createHash('sha256');
  const buffer = Buffer.alloc(1024 * 1024);
  emitHash(0);
  for (let position = 0; position < request.length;) {
    const { bytesRead } = await source.handle.read(buffer, 0, Math.min(buffer.length, request.length - position), position);
    if (!bytesRead) fail('SHORT_SOURCE', 'The opened source ended early.');
    hash.update(buffer.subarray(0, bytesRead)); position += bytesRead; emitHash(position);
  }
  await assertSourceUnchanged(source);
  if (hash.digest('hex') !== request.sha256.toLowerCase()) fail('SOURCE_DIGEST', 'Source does not match the authenticated digest.');
}
