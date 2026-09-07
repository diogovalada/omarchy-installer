import { createHash } from 'node:crypto';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { constants, type Stats } from 'node:fs';
import { lstat, open, realpath, type FileHandle } from 'node:fs/promises';
import * as path from 'node:path';
import { File } from 'etcher-sdk/build/source-destination/file';
import { BlockDevice } from 'etcher-sdk/build/source-destination/block-device';
import { pipeSourceToDestinations } from 'etcher-sdk/build/multi-write';
import type { Drive } from 'drivelist';
import { PhysicalWriteError, type DriveIdentity, type PhysicalEvent, type PhysicalWriteRequest, type PhysicalWriteReceipt } from './physical-contracts.js';
import { fail, fingerprint, inventory, powershellPath, reidentify, sameIdentity, validSector, type InventoryEntry } from './physical-discovery.js';
import { validatePathSyntax } from './safety.js';
import { physicalOpenPath } from './physical-path.js';

interface DirectIo {
  O_DIRECT: number; O_EXLOCK: number; O_SYNC: number;
  getAlignedBuffer(length: number, alignment: number): Buffer;
  getBlockDevice(fd: number, callback: (error: Error | null, value: { size: number; logicalSectorSize: number; physicalSectorSize: number; serialNumber?: string }) => void): void;
  setF_NOCACHE(fd: number, value: number, callback: (error: Error | null) => void): void;
}
const io: DirectIo = require('@ronomon/direct-io');
const mountutils: { unmountDisk(device: string, cb: (e: Error | null) => void): void; eject(device: string, cb: (e: Error | null) => void): void } = require('mountutils');
const call = (fn: (cb: (e: Error | null) => void) => void) => new Promise<void>((resolve, reject) => fn(error => error ? reject(error) : resolve()));

function alignedWriteSpan(length: number, physicalSectorSize: number): number {
  const padding = (physicalSectorSize - length % physicalSectorSize) % physicalSectorSize;
  const span = length + padding;
  if (!Number.isSafeInteger(span) || span < length) fail('CAPACITY', 'Rounded physical write span exceeds the safe byte range.');
  return span;
}

export function validatePhysicalWrite(value: PhysicalWriteRequest): void {
  if (!value || value.protocol !== 1 || value.action !== 'write') fail('INVALID_REQUEST', 'Expected a protocol 1 write command.');
  validatePathSyntax(value.sourcePath);
  if (!Number.isSafeInteger(value.length) || value.length <= 0) fail('INVALID_LENGTH', 'Expected a positive safe integer image length.');
  if (typeof value.sha256 !== 'string' || !/^[a-f0-9]{64}$/i.test(value.sha256)) fail('INVALID_DIGEST', 'Expected an authenticated SHA-256 digest.');
  const t = value.target;
  if (!t || typeof t !== 'object' || typeof t.fingerprint !== 'string' || !/^[a-f0-9]{64}$/.test(t.fingerprint) ||
      ['device', 'raw', 'hardwareId', 'description', 'busType'].some(k => typeof t[k as keyof DriveIdentity] !== 'string') ||
      !(t.devicePath === null || typeof t.devicePath === 'string') || !t.hardwareId ||
      !Number.isSafeInteger(t.size) || t.size <= 0 || !validSector(t.blockSize) || !validSector(t.logicalBlockSize) ||
      t.fingerprint !== fingerprint(t)) fail('INVALID_TARGET', 'An exact complete discovered target identity is required.');
  if (t.blockSize % t.logicalBlockSize !== 0 || t.size % t.logicalBlockSize !== 0) {
    fail('SECTOR_ALIGNMENT', 'Target physical/logical sector sizes or capacity are inconsistent.');
  }
  if (alignedWriteSpan(value.length, t.blockSize) > t.size) fail('CAPACITY', 'The image and its bounded final-sector zero padding exceed target capacity.');
}

async function openStableSource(request: PhysicalWriteRequest): Promise<{ handle: FileHandle; stat: Stats }> {
  const resolved = path.resolve(request.sourcePath);
  let component = path.parse(resolved).root;
  for (const part of resolved.slice(component.length).split(path.sep).filter(Boolean)) {
    component = path.join(component, part);
    if ((await lstat(component)).isSymbolicLink()) fail('SYMLINK_REJECTED', 'Source symlinks and directory junctions are not accepted.');
  }
  if (await realpath(resolved) !== resolved && process.platform !== 'win32') fail('UNSAFE_SOURCE', 'Source path must be canonical.');
  const handle = await open(resolved, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  try {
    const before = await handle.stat();
    if (!before.isFile() || before.size !== request.length) fail('SOURCE_LENGTH', 'Source must be a regular file of the authenticated image length.');
    return { handle, stat: before };
  } catch (error) { await handle.close(); throw error; }
}

function stable(before: Stats, after: Stats): boolean {
  return before.dev === after.dev && before.ino === after.ino && before.size === after.size &&
    before.mtimeMs === after.mtimeMs && before.ctimeMs === after.ctimeMs;
}
async function hashSource(handle: FileHandle, before: Stats, emit: (bytes: number) => void): Promise<string> {
  const hash = createHash('sha256');
  const buffer = Buffer.alloc(1024 * 1024);
  for (let position = 0; position < before.size;) {
    const { bytesRead } = await handle.read(buffer, 0, Math.min(buffer.length, before.size - position), position);
    if (!bytesRead) fail('SHORT_SOURCE', 'The opened source ended early.');
    hash.update(buffer.subarray(0, bytesRead)); position += bytesRead; emit(position);
  }
  if (!stable(before, await handle.stat())) fail('SOURCE_CHANGED', 'Opened source metadata changed during hashing.');
  return hash.digest('hex');
}

class StableSdkSource extends File {
  constructor(filename: string, handle: FileHandle, private readonly size: number) {
    super({ path: filename }); this.fileHandle = handle;
  }
  protected override async _open(): Promise<void> { }
  protected override async _close(): Promise<void> { }
  protected override async _getMetadata() { return { size: this.size, name: 'authenticated-raw.img' }; }
}

/** Own the exclusive descriptor through SDK verify, flush and SHA-256 readback.
 * SDK BlockDevice.open's pre-lock Windows diskpart clean is intentionally bypassed.
 * The SDK still supplies its aligned block writer and mandatory verifier.
 */
class HeldSdkDevice extends BlockDevice {
  private held = false;
  private mayWrite = false;
  readonly writeSpanBytes: number;
  constructor(drive: Drive, private readonly length: number, private readonly modified: () => void,
    private readonly logicalSectorSize: number) {
    super({ drive, write: true, direct: true, keepOriginal: true, unmountOnSuccess: false });
    this.writeSpanBytes = alignedWriteSpan(length, drive.blockSize);
  }
  protected override async _open(): Promise<void> { if (!this.held) fail('DEVICE_NOT_LOCKED', 'Device handle must be acquired explicitly.'); }
  protected override async _close(): Promise<void> { /* Retained until all verification finishes. */ }
  async acquire(): Promise<void> {
    if ((process.platform !== 'darwin' && !io.O_DIRECT) || !io.O_SYNC || (process.platform !== 'linux' && !io.O_EXLOCK)) fail('LOCKING_UNAVAILABLE', 'Native direct I/O/exclusive flags are missing.');
    const flags = constants.O_RDWR | (process.platform === 'darwin' ? 0 : io.O_DIRECT) | io.O_SYNC |
      (process.platform === 'linux' ? constants.O_EXCL : io.O_EXLOCK);
    this.fileHandle = await open(physicalOpenPath(this.raw), flags);
    this.held = true;
    try {
      if (process.platform === 'darwin') await call(cb => io.setF_NOCACHE(this.fileHandle.fd, 1, cb));
      const geometry = await new Promise<{ size: number; logicalSectorSize: number; physicalSectorSize: number }>((resolve, reject) => {
        io.getBlockDevice(this.fileHandle.fd, (e, v) => e ? reject(e) : resolve(v));
      });
      if (geometry.size !== this.size || geometry.physicalSectorSize !== this.alignment || geometry.logicalSectorSize !== this.logicalSectorSize || !validSector(geometry.logicalSectorSize) ||
          geometry.physicalSectorSize % geometry.logicalSectorSize !== 0 || this.writeSpanBytes % geometry.physicalSectorSize !== 0 || this.writeSpanBytes > geometry.size) {
        fail('HANDLE_GEOMETRY_CHANGED', 'Opened device geometry differs from the selected target.');
      }
      if (process.platform !== 'win32') {
        const descriptor = await this.fileHandle.stat();
        const named = await lstat(this.raw);
        if (!(process.platform === 'linux' ? descriptor.isBlockDevice() : descriptor.isCharacterDevice()) ||
            named.isSymbolicLink() || descriptor.rdev !== named.rdev) fail('HANDLE_IDENTITY_CHANGED', 'The opened device no longer matches the discovered device node.');
      }
    } catch (error) { await this.release(); throw error; }
  }
  enableWrites(): void { this.mayWrite = true; }
  override async write(buffer: Buffer, bufferOffset: number, length: number, offset: number) {
    if (!this.mayWrite || !Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(length) || length <= 0 ||
        !Number.isSafeInteger(bufferOffset) || bufferOffset < 0 || bufferOffset + length > buffer.length ||
        offset + length > this.length || offset % this.alignment !== 0) fail('WRITE_BOUNDS', 'Write exceeds the authenticated image boundary or sector alignment.');
    const physicalLength = alignedWriteSpan(length, this.alignment);
    if (offset + physicalLength > this.writeSpanBytes || (physicalLength !== length && offset + length !== this.length)) {
      fail('WRITE_BOUNDS', 'Only the final image chunk may use bounded final-sector zero padding.');
    }
    let physicalBuffer = buffer;
    let physicalBufferOffset = bufferOffset;
    if (physicalLength !== length) {
      // SDK BlockReadStream keeps the original logical final-chunk length.
      // BlockDevice.alignedWrite would preserve existing tail bytes with a
      // read-modify-write. Instead supply an explicitly zeroed aligned buffer,
      // so super.write takes its aligned path and never writes unknown tail data.
      physicalBuffer = io.getAlignedBuffer(physicalLength, this.alignment);
      physicalBuffer.fill(0);
      buffer.copy(physicalBuffer, 0, bufferOffset, bufferOffset + length);
      physicalBufferOffset = 0;
    }
    this.modified();
    const result = await super.write(physicalBuffer, physicalBufferOffset, physicalLength, offset);
    if (result.bytesWritten !== physicalLength) fail('SHORT_WRITE', 'The device did not accept the complete aligned write.');
    // SDK BlockWriteStream counts original buffer.length and verifies the
    // original source length. Do not change its logical-byte accounting.
    return { buffer, bytesWritten: length };
  }
  async flush(): Promise<void> { await this.fileHandle.sync(); }
  async release(): Promise<void> {
    this.mayWrite = false;
    if (this.held) { this.held = false; await this.fileHandle.close(); }
  }
}

interface VolumeLock { release(): Promise<void> }
async function lockWindowsVolumes(entry: InventoryEntry, criticalFailure: (error: Error) => void): Promise<VolumeLock> {
  const disk = entry.windows;
  if (!disk) fail('IDENTITY_UNAVAILABLE', 'Windows disk identity is unavailable.');
  const expected = Buffer.from(JSON.stringify({ uniqueId: disk.UniqueId.trim(), serialNumber: disk.SerialNumber.trim(), path: disk.Path, size: disk.Size })).toString('base64');
  const child: ChildProcessWithoutNullStreams = spawn(powershellPath(), ['-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
    '-File', path.join(__dirname, '..', 'scripts', 'windows-volume-lock.ps1'), '-DiskNumber', String(disk.Number), '-ExpectedIdentity', expected],
  { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  let releasing = false;
  let ready = false;
  let diagnostic = '';
  let output = '';
  const closed = new Promise<void>(resolve => child.once('close', () => resolve()));
  child.stderr.on('data', chunk => { diagnostic = (diagnostic + chunk).slice(-4096); });
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => { child.kill(); reject(new PhysicalWriteError('VOLUME_LOCK_TIMEOUT', 'Windows volume locking timed out.')); }, 30000);
    child.once('error', error => { clearTimeout(timeout); reject(error); });
    child.once('close', code => {
      clearTimeout(timeout);
      const error = new PhysicalWriteError('VOLUME_LOCK_LOST', `Windows volume lock process exited (${code}). ${diagnostic.trim()}`);
      if (!ready) reject(error);
      else if (!releasing) criticalFailure(error);
    });
    child.stdout.on('data', chunk => {
      output += chunk;
      if (output.length > 4096) { child.kill(); return; }
      if (output.trim() === '{"ready":true}') { ready = true; clearTimeout(timeout); resolve(); }
    });
  });
  return { async release() {
    releasing = true; child.stdin.end();
    const timeout = setTimeout(() => child.kill(), 5000);
    try { await closed; } finally { clearTimeout(timeout); }
  } };
}

async function ejectTarget(target: DriveIdentity, sourcePath: string): Promise<PhysicalWriteReceipt['eject']> {
  try {
    // Never send an eject to a reused device pathname after releasing the lock.
    const matches = (await inventory(sourcePath)).filter(d => sameIdentity(d.public.identity, target));
    if (matches.length !== 1) return { status: 'failed', message: 'Written device could not be reidentified for ejection; use the operating system safely-remove action.' };
    if (matches[0].public.reasons.some(reason => ['SYSTEM_DEVICE', 'SOURCE_DEVICE', 'NOT_USB', 'NOT_WHOLE_DEVICE'].includes(reason))) {
      return { status: 'failed', message: 'The selected device safety state changed before ejection; use the operating system safely-remove action.' };
    }
    if (process.platform === 'linux') {
      // Pinned mountutils Linux eject is only an alias for unmount_disk.
      await call(cb => mountutils.unmountDisk(target.device, cb));
      return { status: 'unmounted', message: 'Filesystems unmounted. This SDK has no Linux power-off/eject implementation; use the operating system safely-remove action.' };
    }
    await call(cb => mountutils.eject(target.device, cb));
    if ((await inventory()).some(d => sameIdentity(d.public.identity, target))) {
      return { status: 'failed', message: 'Ejection returned without confirming device removal; use the operating system safely-remove action.' };
    }
    return { status: 'ejected', message: 'The operating system removed the selected device from drive discovery.' };
  } catch (error) { return { status: 'failed', message: `Image verified, but ejection failed: ${(error as Error).message}` }; }
}

/** Internal process entry point. Cancellation must terminate this process. */
export async function runPhysicalWrite(request: PhysicalWriteRequest, emit: (event: PhysicalEvent) => void,
  markModified: () => void, criticalFailure: (error: Error) => void): Promise<PhysicalWriteReceipt> {
  validatePhysicalWrite(request);
  const writeSpanBytes = alignedWriteSpan(request.length, request.target.blockSize);
  const paddingBytes = writeSpanBytes - request.length;
  const stage = (name: 'validating' | 'hashing' | 'unmounting' | 'writing' | 'verifying' | 'flushing' | 'readback' | 'ejecting', bytes?: number) =>
    emit({ protocol: 1, type: 'event', stage: name, ...(bytes === undefined ? {} : { bytes, totalBytes: request.length }) });
  stage('validating');
  const source = await openStableSource(request);
  let device: HeldSdkDevice | undefined;
  let volumeLock: VolumeLock | undefined;
  try {
    // Always reidentify before spending time hashing and once more before unmount/open.
    await reidentify(request.target, request.sourcePath);
    stage('hashing', 0);
    const expected = request.sha256.toLowerCase();
    if (await hashSource(source.handle, source.stat, bytes => stage('hashing', bytes)) !== expected) fail('SOURCE_DIGEST', 'Source does not match the authenticated digest.');
    const selected = await reidentify(request.target, request.sourcePath);
    stage('unmounting');
    if (process.platform === 'win32') volumeLock = await lockWindowsVolumes(selected, criticalFailure);
    else await call(cb => mountutils.unmountDisk(selected.drive.device, cb));
    // The root source must live on another disk and remain mapped after unmount.
    const unmounted = await reidentify(request.target, request.sourcePath, !!volumeLock);
    if (process.platform !== 'win32' && unmounted.drive.mountpoints.length) fail('UNMOUNT_FAILED', 'Target still has mounted filesystems.');
    device = new HeldSdkDevice(unmounted.drive, request.length, markModified, request.target.logicalBlockSize);
    await device.acquire();
    // Check pathname identity again while holding the exclusive descriptor. Fail closed
    // if this host cannot enumerate an exclusively opened device.
    await reidentify(request.target, request.sourcePath, !!volumeLock);
    if (!stable(source.stat, await source.handle.stat())) fail('SOURCE_CHANGED', 'Opened source changed before writing.');
    device.enableWrites();
    stage('writing', 0);
    const result = await pipeSourceToDestinations({ source: new StableSdkSource(request.sourcePath, source.handle, request.length),
      destinations: [device], verify: true, numBuffers: 2, onFail: () => {}, onProgress: progress => {
        if (progress.type === 'flashing' || progress.type === 'verifying') stage(progress.type === 'flashing' ? 'writing' : 'verifying', progress.position);
      } });
    // Pinned multi-write reports the logical source-stream byte position. The
    // final physical-sector padding is outside both its count and source hash.
    if (result.failures.size || result.bytesWritten !== request.length) fail('SDK_WRITE_VERIFY_FAILED',
      [...result.failures.values()].map(e => e.message).join('; ') || 'SDK wrote an unexpected byte count.');
    stage('flushing');
    await device.flush();
    stage('readback', 0);
    const hash = createHash('sha256');
    const buffer = io.getAlignedBuffer(1024 * 1024, request.target.blockSize);
    for (let position = 0; position < writeSpanBytes;) {
      const count = Math.min(buffer.length, writeSpanBytes - position);
      const { bytesRead } = await device.read(buffer, 0, count, position);
      if (bytesRead !== count) fail('SHORT_READBACK', 'Device readback ended before the bounded physical write boundary.');
      const imageBytes = Math.min(bytesRead, Math.max(0, request.length - position));
      hash.update(buffer.subarray(0, imageBytes));
      if (buffer.subarray(imageBytes, bytesRead).some(byte => byte !== 0)) fail('PADDING_MISMATCH', 'Final-sector padding did not read back as zero.');
      position += bytesRead; stage('readback', Math.min(position, request.length));
    }
    const sha256 = hash.digest('hex');
    if (sha256 !== expected) fail('READBACK_MISMATCH', 'Full image SHA-256 readback differs from the authenticated source.');
    stage('hashing', 0);
    if (await hashSource(source.handle, source.stat, bytes => stage('hashing', bytes)) !== expected) fail('SOURCE_CHANGED', 'Source changed during writing.');
    await device.flush();
    await device.release(); device = undefined;
    await volumeLock?.release(); volumeLock = undefined;
    stage('ejecting');
    const eject = await ejectTarget(request.target, request.sourcePath);
    return { engine: 'etcher-sdk', engineVersion: '10.2.14', destinationKind: 'usb-whole-device', target: request.target,
      bytesWritten: request.length, writeSpanBytes, paddingBytes, sha256, sdkVerification: true, flushed: true,
      fullReadbackVerification: true, readbackBytes: request.length, readbackSpanBytes: writeSpanBytes,
      paddingVerification: true, eject, hardwareQualified: false };
  } finally {
    // A close/flush error must prevent a success receipt. Do not swallow cleanup failures.
    try { await device?.release(); }
    finally { try { await volumeLock?.release(); } finally { await source.handle.close(); } }
  }
}
