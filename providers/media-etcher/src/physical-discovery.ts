import { createHash } from 'node:crypto';
import { access, readFile, realpath, stat } from 'node:fs/promises';
import * as path from 'node:path';
import type { Drive } from 'drivelist';
import { PhysicalWriteError, type DriveIdentity, type PhysicalDrive, type PhysicalProbe } from './physical-contracts.js';
import { validatePathSyntax } from './safety.js';
import { runTool } from './physical-tools.js';
import { linuxBackingDisks, linuxHasHolders, linuxSwapPaths, linuxTargetMounts } from './physical-linux.js';
import { macUsbMedia, macApfsStores, macBackingDisks, type MacUsbMedia } from './physical-macos.js';
export { runTool } from './physical-tools.js';

export function fail(code: string, message: string): never { throw new PhysicalWriteError(code, message); }
export function supportedPlatform(): void {
  if (!['win32', 'linux', 'darwin'].includes(process.platform)) fail('UNSUPPORTED_PLATFORM', 'USB writing supports Windows, Linux and macOS only.');
  if (![22, 24].includes(Number(process.versions.node.split('.')[0]))) fail('UNSUPPORTED_RUNTIME', 'The packaged provider requires Node 22 or 24.');
}

export function powershellPath(): string {
  const root = process.env.SystemRoot;
  if (!root || !/^[a-z]:\\Windows$/i.test(root)) fail('PLATFORM_PREREQUISITE', 'A standard trusted Windows SystemRoot is required.');
  return path.join(root, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe');
}

export interface WindowsDisk {
  Number: number; UniqueId: string; SerialNumber: string; Path: string; Size: number;
  LogicalSectorSize: number; PhysicalSectorSize: number; BusType: string;
  IsBoot: boolean; IsSystem: boolean; IsReadOnly: boolean; IsOffline: boolean;
}
const WINDOWS_INVENTORY = "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); @(Get-Disk | Select-Object Number,UniqueId,SerialNumber,Path,Size,LogicalSectorSize,PhysicalSectorSize,@{n='BusType';e={[string]$_.BusType}},IsBoot,IsSystem,IsReadOnly,IsOffline) | ConvertTo-Json -Compress -Depth 3";
export function windowsHardwareId(disk: WindowsDisk): string {
  return JSON.stringify({ uniqueId: disk.UniqueId?.trim(), serialNumber: disk.SerialNumber?.trim(), path: disk.Path });
}
export async function windowsDisks(): Promise<WindowsDisk[]> {
  const json = JSON.parse(await runTool(powershellPath(), ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', WINDOWS_INVENTORY]));
  return json === null ? [] : Array.isArray(json) ? json : [json];
}

async function linuxHardwareId(drive: Drive): Promise<string> {
  const name = path.basename(drive.device);
  const sys = await realpath(`/sys/class/block/${name}`);
  try { await access(`${sys}/partition`); fail('NOT_WHOLE_DEVICE', 'Partitions cannot be written.'); }
  catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
  if (!sys.startsWith('/sys/devices/')) fail('IDENTITY_UNAVAILABLE', 'Device sysfs ancestry is unavailable.');
  if (await linuxHasHolders(sys)) fail('DEVICE_IN_USE', 'Device or its partitions have active storage holders.');
  let serial = '';
  let current = sys;
  while (current !== '/sys/devices') {
    try { serial = (await readFile(`${current}/serial`, 'utf8')).trim(); } catch { /* try parent */ }
    if (serial) break;
    current = path.dirname(current);
    if (!current.startsWith('/sys/devices')) break;
  }
  if (!serial) {
    try { serial = (await readFile(`${sys}/device/serial`, 'utf8')).trim(); } catch { /* fail closed below */ }
  }
  if (!serial) fail('IDENTITY_UNAVAILABLE', 'USB hardware serial is unavailable.');
  const dev = (await readFile(`${sys}/dev`, 'utf8')).trim();
  return JSON.stringify({ serial, sys, dev });
}

function wholeDevice(drive: Drive): boolean {
  if (process.platform === 'win32') return /^\\\\\.\\PhysicalDrive\d+$/i.test(drive.device) && drive.raw.toLowerCase() === drive.device.toLowerCase();
  if (process.platform === 'darwin') return /^\/dev\/disk\d+$/.test(drive.device) && drive.raw === drive.device.replace('/dev/disk', '/dev/rdisk');
  // USB disk transports currently supported by the pinned SDK/drivelist.
  return /^\/dev\/sd[a-z]+$/.test(drive.device) && drive.raw === drive.device;
}

export function identityFields(value: DriveIdentity): Omit<DriveIdentity, 'fingerprint' | 'description'> {
  // Linux display descriptions include volume labels/mountpoints. They change
  // on unmount and when the ISO replaces partitions; they are not hardware ID.
  return { device: value.device, raw: value.raw, devicePath: value.devicePath, hardwareId: value.hardwareId,
    size: value.size, blockSize: value.blockSize, logicalBlockSize: value.logicalBlockSize,
    busType: value.busType };
}
export function fingerprint(value: DriveIdentity): string {
  return createHash('sha256').update(JSON.stringify(identityFields(value))).digest('hex');
}
export function sameIdentity(left: DriveIdentity, right: DriveIdentity): boolean {
  return left.fingerprint === right.fingerprint && JSON.stringify(identityFields(left)) === JSON.stringify(identityFields(right));
}
export function validSector(value: number): boolean { return Number.isInteger(value) && value >= 512 && value <= 4096 && (value & (value - 1)) === 0; }

/** Compare filesystem device IDs as well as paths to cover volume aliases and bind mounts. */
async function exclusionReasons(drives: Drive[], sourcePath?: string): Promise<Map<Drive, string[]>> {
  const reasons = new Map<Drive, string[]>(drives.map(d => [d, []]));
  const protectedPaths = process.platform === 'win32'
    ? [process.env.SystemRoot!, process.execPath]
    : process.platform === 'darwin'
      ? ['/', '/System/Volumes/Data', '/Users', '/private/var', process.execPath]
      : ['/', '/boot', '/boot/efi', '/usr', '/var', '/home', process.execPath];
  if (process.platform === 'linux') {
    const paths = [...protectedPaths, ...await linuxSwapPaths(), ...(sourcePath ? [sourcePath] : [])];
    const filesystems = new Map<number, string[]>();
    for (const filename of paths) {
      let metadata;
      try { metadata = await stat(filename); }
      catch (error) {
        if (filename !== sourcePath && (error as NodeJS.ErrnoException).code === 'ENOENT') continue;
        fail('EXCLUSION_UNAVAILABLE', 'Cannot identify a protected Linux filesystem.');
      }
      // Swap devices use rdev, not the device ID of devtmpfs. Do not cache them.
      let backing = metadata.isBlockDevice() ? undefined : filesystems.get(metadata.dev);
      if (!backing) {
        backing = await linuxBackingDisks(filename);
        if (!metadata.isBlockDevice()) filesystems.set(metadata.dev, backing);
      }
      if (backing.some(device => !drives.some(d => d.device === device))) {
        fail('EXCLUSION_UNAVAILABLE', 'A protected Linux filesystem has an unenumerated backing disk.');
      }
      for (const drive of drives) {
        if (backing.includes(drive.device)) reasons.get(drive)!.push(filename === sourcePath ? 'SOURCE_DEVICE' : 'SYSTEM_DEVICE');
      }
    }
    return reasons;
  }
  if (process.platform === 'darwin') {
    const stores = await macApfsStores();
    // Synthesized APFS disks own the mountpoints; their physical USB stores
    // must not become selectable merely because they have no direct mounts.
    for (const drive of drives) {
      if ([...stores.values()].some(disks => disks.includes(drive.device))) reasons.get(drive)!.push('DEVICE_IN_USE');
    }
    const filesystems = new Map<number, string[]>();
    for (const filename of [...protectedPaths, ...(sourcePath ? [sourcePath] : [])]) {
      let metadata;
      try { metadata = await stat(filename); }
      catch (error) {
        if (filename !== sourcePath && (error as NodeJS.ErrnoException).code === 'ENOENT') continue;
        fail('EXCLUSION_UNAVAILABLE', 'Cannot identify a protected macOS filesystem.');
      }
      let backing = filesystems.get(metadata.dev);
      if (!backing) { backing = await macBackingDisks(filename, stores); filesystems.set(metadata.dev, backing); }
      if (backing.some(device => !drives.some(d => d.device === device))) fail('EXCLUSION_UNAVAILABLE', 'A protected macOS filesystem has an unenumerated backing disk.');
      for (const drive of drives) {
        if (backing.includes(drive.device)) reasons.get(drive)!.push(filename === sourcePath ? 'SOURCE_DEVICE' : 'SYSTEM_DEVICE');
      }
    }
    return reasons;
  }
  const volumes = await Promise.all(drives.map(async drive => ({ drive, mounts: await Promise.all(drive.mountpoints.map(async m => {
    try { return { path: await realpath(m.path), dev: (await stat(m.path)).dev }; }
    catch { reasons.get(drive)!.push('MOUNT_IDENTITY_UNAVAILABLE'); return null; }
  })) })));
  for (const filename of [...protectedPaths, ...(sourcePath ? [sourcePath] : [])]) {
    let dev: number;
    try { dev = (await stat(filename)).dev; }
    catch (error) {
      if (filename !== sourcePath && (error as NodeJS.ErrnoException).code === 'ENOENT') continue;
      fail('EXCLUSION_UNAVAILABLE', 'Cannot identify a protected filesystem.');
    }
    let matched = false;
    for (const entry of volumes) {
      if (entry.mounts.some(m => m && m.dev === dev)) {
        matched = true;
        reasons.get(entry.drive)!.push(filename === sourcePath ? 'SOURCE_DEVICE' : 'SYSTEM_DEVICE');
      }
    }
    // System flags plus exact source mapping are mandatory; ambiguous source layouts fail closed.
    if (filename === sourcePath && !matched) fail('SOURCE_DEVICE_UNRESOLVED', 'Cannot map the source filesystem to an enumerated disk.');
  }
  return reasons;
}

export interface InventoryEntry { drive: Drive; public: PhysicalDrive; windows?: WindowsDisk }
export async function inventory(sourcePath?: string): Promise<InventoryEntry[]> {
  supportedPlatform();
  if (sourcePath !== undefined) validatePathSyntax(sourcePath);
  const { list } = await import('drivelist');
  const drives = await list();
  const exclusions = await exclusionReasons(drives, sourcePath);
  let windows: WindowsDisk[] = [];
  let mac = new Map<string, MacUsbMedia>();
  let enrichmentError: string | undefined;
  try {
    if (process.platform === 'win32') windows = await windowsDisks();
    if (process.platform === 'darwin') mac = await macUsbMedia();
  } catch (error) { enrichmentError = (error as Error).message; }
  return Promise.all(drives.map(async drive => {
    const reasons = exclusions.get(drive)!;
    if (!wholeDevice(drive)) reasons.push('NOT_WHOLE_DEVICE');
    if (drive.error) reasons.push('DISCOVERY_ERROR');
    if (drive.isUSB !== true || drive.busType.toUpperCase() !== 'USB') reasons.push('NOT_USB');
    if (drive.isSystem !== false) reasons.push('SYSTEM_DEVICE');
    if (drive.isVirtual !== false) reasons.push('VIRTUAL_OR_UNKNOWN_DEVICE');
    if (drive.isReadOnly !== false) reasons.push('READ_ONLY');
    if (!Number.isSafeInteger(drive.size) || !drive.size || drive.size <= 0) reasons.push('UNKNOWN_CAPACITY');
    if (!validSector(drive.blockSize) || !validSector(drive.logicalBlockSize) || drive.blockSize % drive.logicalBlockSize !== 0) reasons.push('UNSUPPORTED_SECTOR_SIZE');
    let hardwareId = '';
    let native: WindowsDisk | undefined;
    if (wholeDevice(drive)) {
      try {
        if (enrichmentError) fail('IDENTITY_UNAVAILABLE', enrichmentError);
        if (process.platform === 'win32') {
          const diskNumber = Number(drive.device.match(/\d+$/)![0]);
          native = windows.find(d => d.Number === diskNumber);
          if (!native || !native.UniqueId?.trim() || !native.SerialNumber?.trim() || !native.Path) fail('IDENTITY_UNAVAILABLE', 'Disk hardware identity is unavailable.');
          if (native.IsBoot !== false || native.IsSystem !== false) reasons.push('SYSTEM_DEVICE');
          if (native.IsReadOnly !== false || native.IsOffline !== false) reasons.push('READ_ONLY_OR_OFFLINE');
          if (native.BusType !== 'USB') reasons.push('NOT_USB');
          if (native.Size !== drive.size || native.LogicalSectorSize !== drive.logicalBlockSize || native.PhysicalSectorSize !== drive.blockSize) reasons.push('IDENTITY_DISAGREEMENT');
          hardwareId = windowsHardwareId(native);
        } else if (process.platform === 'linux') {
          hardwareId = await linuxHardwareId(drive);
          drive.mountpoints = (await linuxTargetMounts(drive.device)).map(m => ({ path: m.mountpoint, label: null }));
        } else {
          const media = mac.get(drive.device);
          hardwareId = media?.hardwareId ?? '';
          if (media) {
            if ((media.size !== undefined && media.size !== drive.size) ||
                (media.logicalBlockSize !== undefined && media.logicalBlockSize !== drive.logicalBlockSize)) reasons.push('IDENTITY_DISAGREEMENT');
            if (media.physicalBlockSize !== undefined) {
              if (!validSector(media.physicalBlockSize) || media.physicalBlockSize % drive.logicalBlockSize !== 0) reasons.push('UNSUPPORTED_SECTOR_SIZE');
              else drive.blockSize = media.physicalBlockSize;
            }
          }
        }
      } catch (error) { reasons.push((error as PhysicalWriteError).code ?? 'IDENTITY_UNAVAILABLE'); }
    }
    if (!hardwareId) reasons.push('IDENTITY_UNAVAILABLE');
    const identity: DriveIdentity = { fingerprint: '', device: drive.device, raw: drive.raw, devicePath: drive.devicePath,
      hardwareId, size: drive.size ?? 0, blockSize: drive.blockSize, logicalBlockSize: drive.logicalBlockSize,
      description: drive.description, busType: drive.busType };
    identity.fingerprint = fingerprint(identity);
    return { drive, windows: native, public: { identity, mountpoints: drive.mountpoints.map(m => m.path),
      eligible: reasons.length === 0, reasons: [...new Set(reasons)] } };
  }));
}

export async function reidentify(target: DriveIdentity, sourcePath: string, volumesHeld = false): Promise<InventoryEntry> {
  const matches = (await inventory(sourcePath)).filter(d => sameIdentity(d.public.identity, target));
  if (matches.length !== 1) fail('TARGET_CHANGED', 'The selected USB identity changed or disappeared. Select the drive again.');
  // Windows dismounted/locked volumes may retain drive letters that can no longer
  // be stat'ed. This exception applies only after acquiring our own volume locks;
  // all other exclusions, source mapping and the exact hardware identity remain.
  const reasons = matches[0].public.reasons.filter(reason => !(volumesHeld && reason === 'MOUNT_IDENTITY_UNAVAILABLE'));
  if (reasons.length) fail('UNSAFE_TARGET', `Target is ineligible: ${reasons.join(', ')}.`);
  return matches[0];
}

export async function probePhysical(): Promise<PhysicalProbe> {
  let reason: string | null = null;
  try {
    supportedPlatform();
    const sdk = require('etcher-sdk/package.json');
    if (sdk.version !== '10.2.14') fail('SDK_VERSION', 'Pinned Etcher SDK version mismatch.');
    require('etcher-sdk/build/multi-write');
    require('etcher-sdk/build/source-destination/block-device');
    const io = require('@ronomon/direct-io');
    const mounts = require('mountutils');
    if (typeof io.getBlockDevice !== 'function' || typeof mounts.unmountDisk !== 'function' || typeof mounts.eject !== 'function') fail('PLATFORM_PREREQUISITE', 'Required native device functions are missing.');
    if (process.platform === 'win32') {
      await access(path.join(__dirname, '..', 'scripts', 'windows-volume-lock.ps1'));
      await windowsDisks();
    }
    if (process.platform === 'darwin') await macUsbMedia();
    if (process.platform === 'linux') {
      await access('/sys/class/block');
      await runTool('/usr/bin/lsblk', ['--version']);
      await runTool('/usr/bin/findmnt', ['--version']);
      await access('/usr/bin/umount');
    }
    require('drivelist');
  } catch (error) { reason = (error as Error).message; }
  return { available: reason === null, platform: process.platform, arch: process.arch, engine: 'etcher-sdk', engineVersion: '10.2.14', reason,
    capabilities: { discovery: reason === null, write: reason === null, requiresElevation: true,
      mandatoryVerification: true, fullReadback: true, hardwareQualified: false } };
}
