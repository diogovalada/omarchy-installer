import { PhysicalWriteError } from './physical-contracts.js';
import { runTool } from './physical-tools.js';

export interface MacUsbMedia { hardwareId: string; physicalBlockSize?: number; logicalBlockSize?: number; size?: number }
const Plist = require('@balena/apple-plist') as { parse(value: Buffer): { data: unknown } };

export function parseMacApfsStores(plist: string): Map<string, string[]> {
  const data = Plist.parse(Buffer.from(plist, 'utf8')).data as { Containers?: unknown[] };
  if (!data || !Array.isArray(data.Containers)) throw new PhysicalWriteError('EXCLUSION_UNAVAILABLE', 'APFS container inventory is unavailable.');
  const result = new Map<string, string[]>();
  for (const value of data.Containers) {
    const container = value as { ContainerReference?: string; PhysicalStores?: { DeviceIdentifier?: string }[] };
    if (!container || !/^disk\d+$/.test(container.ContainerReference ?? '') || !Array.isArray(container.PhysicalStores) || !container.PhysicalStores.length || result.has(container.ContainerReference!)) {
      throw new PhysicalWriteError('EXCLUSION_UNAVAILABLE', 'APFS container identity is ambiguous.');
    }
    const disks = container.PhysicalStores.map(store => {
      const match = store.DeviceIdentifier?.match(/^(disk\d+)(?:s\d+)?$/);
      if (!match) throw new PhysicalWriteError('EXCLUSION_UNAVAILABLE', 'APFS physical store is unresolved.');
      return `/dev/${match[1]}`;
    });
    result.set(container.ContainerReference!, [...new Set(disks)]);
  }
  return result;
}

export function parseMacBackingDisks(plist: string, stores: Map<string, string[]>, expectedDevice?: string): string[] {
  const info = Plist.parse(Buffer.from(plist, 'utf8')).data as Record<string, unknown>;
  if (expectedDevice && info?.DeviceIdentifier !== expectedDevice) throw new PhysicalWriteError('EXCLUSION_UNAVAILABLE', 'macOS source volume identity changed.');
  const whole = info?.Whole === true ? info.DeviceIdentifier : info?.ParentWholeDisk;
  if (typeof whole !== 'string' || !/^disk\d+$/.test(whole)) throw new PhysicalWriteError('EXCLUSION_UNAVAILABLE', 'macOS filesystem backing disk is unavailable.');
  const backing = stores.get(whole);
  if (backing) return backing;
  if (info.VirtualOrPhysical !== 'Physical' || info.FilesystemType === 'apfs') {
    throw new PhysicalWriteError('EXCLUSION_UNAVAILABLE', 'macOS virtual filesystem has no resolved physical stores.');
  }
  return [`/dev/${whole}`];
}

export async function macApfsStores(): Promise<Map<string, string[]>> {
  return parseMacApfsStores(await runTool('/usr/sbin/diskutil', ['apfs', 'list', '-plist']));
}

export async function macBackingDisks(filename: string, stores: Map<string, string[]>): Promise<string[]> {
  // diskutil accepts device identifiers or volume mountpoints, not arbitrary
  // image/executable paths. df resolves a file through the actual mounted volume,
  // including APFS firmlinks and system snapshots, without guessing path prefixes.
  const rows = (await runTool('/bin/df', ['-P', filename])).trim().split('\n').slice(1);
  const match = rows.length === 1 ? rows[0].match(/^\/dev\/(disk\d+(?:s\d+)*)\s+\d+\s+\d+\s+-?\d+\s+\d+%\s+\//) : null;
  if (!match) throw new PhysicalWriteError('EXCLUSION_UNAVAILABLE', 'macOS source does not resolve to one local disk volume.');
  return parseMacBackingDisks(await runTool('/usr/sbin/diskutil', ['info', '-plist', `/dev/${match[1]}`]), stores, match[1]);
}

/** ioreg contains CFData values which plutil cannot represent as JSON. */
export function parseMacUsbRegistry(plist: string): Map<string, MacUsbMedia> {
  // ioreg succeeds with no output when no service matches the requested class.
  if (!plist.trim()) return new Map();
  const tree: unknown = Plist.parse(Buffer.from(plist, 'utf8')).data;
  const result = new Map<string, MacUsbMedia>();
  let count = 0;
  function visit(value: unknown, serial = '', location = '', physical?: number, logical?: number, depth = 0): void {
    if (++count > 16384 || depth > 64) throw new PhysicalWriteError('IDENTITY_UNAVAILABLE', 'USB registry exceeds its structural bounds.');
    if (Array.isArray(value)) { value.forEach(item => visit(item, serial, location, physical, logical, depth + 1)); return; }
    if (!value || typeof value !== 'object' || Buffer.isBuffer(value)) return;
    const node = value as Record<string, unknown>;
    if (node.IOObjectClass === 'IOUSBHostDevice' ||
        (Array.isArray(node.IOObjectInheritance) && node.IOObjectInheritance.includes('IOUSBHostDevice')) ||
        Object.hasOwn(node, 'USB Serial Number')) {
      serial = typeof node['USB Serial Number'] === 'string' ? node['USB Serial Number'].trim() : '';
      const id = node.locationID ?? node.IORegistryEntryID;
      location = typeof id === 'number' && Number.isSafeInteger(id) && id > 0 ? String(id) : '';
      physical = undefined; logical = undefined;
    }
    const characteristics = node['Device Characteristics'] as Record<string, unknown> | undefined;
    const p = node['Physical Block Size'] ?? characteristics?.['Physical Block Size'];
    const l = node['Logical Block Size'] ?? characteristics?.['Logical Block Size'];
    if (typeof p === 'number') physical = p;
    if (typeof l === 'number') logical = l;
    const bsd = node['BSD Name'];
    if (typeof bsd === 'string' && /^disk\d+$/.test(bsd) && node.Whole === true && serial && location) {
      const key = `/dev/${bsd}`;
      const registryId = node.IORegistryEntryID;
      if (!Number.isSafeInteger(registryId) || Number(registryId) <= 0 || result.has(key)) {
        throw new PhysicalWriteError('IDENTITY_UNAVAILABLE', 'USB registry contains an ambiguous media identity.');
      }
      result.set(key, { hardwareId: JSON.stringify({ serial, location, registryId }),
        physicalBlockSize: physical, logicalBlockSize: logical ?? (typeof node['Preferred Block Size'] === 'number' ? node['Preferred Block Size'] : undefined),
        size: typeof node.Size === 'number' ? node.Size : undefined });
    }
    if (node.IORegistryEntryChildren) visit(node.IORegistryEntryChildren, serial, location, physical, logical, depth + 1);
  }
  visit(tree);
  return result;
}

export async function macUsbMedia(): Promise<Map<string, MacUsbMedia>> {
  return parseMacUsbRegistry(await runTool('/usr/sbin/ioreg', ['-a', '-r', '-c', 'IOUSBHostDevice', '-l', '-i']));
}
