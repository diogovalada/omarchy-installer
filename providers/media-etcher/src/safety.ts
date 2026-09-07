import { lstat, open } from 'node:fs/promises';
import { constants } from 'node:fs';
import * as path from 'node:path';
import { FileWriteError, type FileWriteRequest } from './contracts.js';

export function validatePathSyntax(input: string): void {
  if (typeof input !== 'string' || !input || input.includes('\0')) {
    throw new FileWriteError('UNSAFE_PATH', 'A nonempty absolute local file path is required.');
  }
  const portable = input.replaceAll('\\', '/');
  // Reject Win32/NT device namespaces, UNC/network paths, POSIX device/pseudo files,
  // URLs, drive-relative paths, alternate data streams, and Win32 reserved names.
  if (!path.isAbsolute(input) || (process.platform === 'win32' && !/^[a-z]:\//i.test(portable)) ||
      portable.startsWith('//') ||
      /^\/(?:dev|proc|sys)(?:\/|$)/i.test(portable) ||
      /^\/(?:\?\?|Device|GLOBALROOT)(?:\/|$)/i.test(portable) ||
      /^[a-z]+:\/\//i.test(portable) ||
      portable.replace(/^[a-z]:/i, '').includes(':') ||
      portable.split('/').some(part => part === '..' ||
        /[. ]$/.test(part) || /^(?:con|prn|aux|nul|com[1-9¹²³]|lpt[1-9¹²³])(?:\.|$)/i.test(part))) {
    throw new FileWriteError('UNSAFE_PATH', 'Only ordinary absolute local file paths are allowed.');
  }
}

export function validateRequest(request: FileWriteRequest): void {
  if (!request || typeof request !== 'object') {
    throw new FileWriteError('INVALID_REQUEST', 'A file write request is required.');
  }
  validatePathSyntax(request.sourcePath);
  validatePathSyntax(request.destinationPath);
  if (typeof request.expectedSha256 !== 'string' || !/^[a-f0-9]{64}$/i.test(request.expectedSha256)) {
    throw new FileWriteError('INVALID_DIGEST', 'A 64-character SHA-256 digest is required.');
  }
  const key = (value: string) => process.platform === 'win32' ? path.resolve(value).toLowerCase() : path.resolve(value);
  if (key(request.sourcePath) === key(request.destinationPath)) {
    throw new FileWriteError('SOURCE_DESTINATION_ALIAS', 'The source and destination must differ.');
  }
}

async function rejectLinkedComponents(filename: string): Promise<void> {
  const resolved = path.resolve(filename);
  const root = path.parse(resolved).root;
  let component = root;
  for (const part of resolved.slice(root.length).split(path.sep).filter(Boolean)) {
    component = path.join(component, part);
    const stat = await lstat(component);
    if (stat.isSymbolicLink()) {
      throw new FileWriteError('SYMLINK_REJECTED', 'Symbolic links and directory junctions are not allowed.');
    }
  }
}

export async function openSource(request: FileWriteRequest) {
  validateRequest(request);
  await rejectLinkedComponents(request.sourcePath);
  const handle = await open(request.sourcePath, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  try {
    const stat = await handle.stat();
    if (!stat.isFile() || stat.size === 0 || !Number.isSafeInteger(stat.size)) {
      throw new FileWriteError('INVALID_SOURCE', 'Source must be a nonempty regular file with a safe byte length.');
    }
    return { handle, stat };
  } catch (error) {
    await handle.close();
    throw error;
  }
}

export async function createDestination(filename: string) {
  validatePathSyntax(filename);
  await rejectLinkedComponents(path.dirname(filename));
  try {
    const existing = await lstat(filename);
    throw new FileWriteError(existing.isSymbolicLink() ? 'SYMLINK_REJECTED' : 'DESTINATION_EXISTS',
      'The destination must not exist; existing files, links, and devices are never overwritten.');
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }
  // O_EXCL also rejects an existing symlink swapped into the final component.
  const handle = await open(filename, constants.O_CREAT | constants.O_EXCL | constants.O_RDWR |
    (constants.O_NOFOLLOW ?? 0), 0o600);
  try {
    if (!(await handle.stat()).isFile()) {
      throw new FileWriteError('INVALID_DESTINATION', 'Destination must be a regular file.');
    }
    return handle;
  } catch (error) {
    await handle.close();
    throw error;
  }
}
