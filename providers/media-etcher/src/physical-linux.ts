import { readFile, realpath, readdir, stat } from 'node:fs/promises';
import { posix as path } from 'node:path';
import { PhysicalWriteError } from './physical-contracts.js';
import { runTool } from './physical-tools.js';

function fail(message: string): never { throw new PhysicalWriteError('EXCLUSION_UNAVAILABLE', message); }
const decode = (value: string) => value.replace(/\\(040|011|012|134)/g, (_match, code) => String.fromCharCode(parseInt(code, 8)));
export interface LinuxMount { id: number; parent: number; device: string; mountpoint: string; type: string; source: string }

export function parseLinuxMounts(text: string): LinuxMount[] {
  if (text.length > 32 * 1024 * 1024) fail('Linux mount inventory exceeded its bound.');
  return text.trim().split('\n').filter(Boolean).map(line => {
    const fields = line.split(' ');
    const separator = fields.indexOf('-');
    if (separator < 6 || fields.length < separator + 4 || !/^\d+$/.test(fields[0]) || !/^\d+$/.test(fields[1]) || !/^\d+:\d+$/.test(fields[2])) {
      fail('Linux mount inventory is incomplete.');
    }
    const mountpoint = decode(fields[4]);
    if (!mountpoint.startsWith('/')) fail('Linux mountpoint is not absolute.');
    return { id: Number(fields[0]), parent: Number(fields[1]), device: fields[2], mountpoint, type: fields[separator + 1], source: decode(fields[separator + 2]) };
  });
}

function assertVisibleMount(selected: LinuxMount, mounts: LinuxMount[]): void {
  const byId = new Map(mounts.map(mount => [mount.id, mount]));
  if (byId.size !== mounts.length) fail('Linux mount identities are ambiguous.');
  const ancestors = new Set<number>();
  let current: LinuxMount | undefined = selected;
  while (current) {
    if (ancestors.has(current.id)) fail('Linux mount ancestry is cyclic.');
    ancestors.add(current.id);
    if (current.parent === current.id && current.mountpoint === '/') break;
    current = byId.get(current.parent);
  }
  // A mount at this pathname, or covering an ancestor directory, can hide an
  // older mount. umount(path) would operate on that other filesystem instead.
  if (mounts.some(mount => !ancestors.has(mount.id) &&
      (selected.mountpoint === mount.mountpoint || selected.mountpoint.startsWith(mount.mountpoint === '/' ? '/' : mount.mountpoint + '/')))) {
    fail('A Linux mount is hidden by overlapping mounts; resolve them before writing.');
  }
}

async function exists(filename: string): Promise<boolean> {
  try { await stat(filename); return true; }
  catch (error) { if (['ENOENT', 'ENOTDIR'].includes((error as NodeJS.ErrnoException).code ?? '')) return false; throw error; }
}

/** Follow partitions and all device-mapper/RAID members to whole disks. */
export async function linuxBlockDisks(sysPath: string, seen = new Set<string>()): Promise<string[]> {
  const sys = await realpath(sysPath);
  if (!sys.startsWith('/sys/devices/') || seen.has(sys) || seen.size >= 32) fail('Linux backing-device ancestry is ambiguous.');
  const next = new Set(seen).add(sys);
  if (await exists(`${sys}/partition`)) return linuxBlockDisks(path.dirname(sys), next);
  const slaves = await readdir(`${sys}/slaves`);
  if (slaves.length) {
    return [...new Set((await Promise.all(slaves.map(name => linuxBlockDisks(`${sys}/slaves/${name}`, next)))).flat())];
  }
  // Loop-backed runtime/images still belong to the disk holding their file.
  if (await exists(`${sys}/loop/backing_file`)) {
    const backing = (await readFile(`${sys}/loop/backing_file`, 'utf8')).trim();
    if (!backing.startsWith('/')) fail('Linux loop backing file is unavailable.');
    return linuxBackingDisks(backing, next);
  }
  if (/^\/sys\/devices\/virtual\/block\/zram\d+$/.test(sys)) return [];
  if (sys.startsWith('/sys/devices/virtual/')) fail('Linux virtual storage has no resolvable backing disk.');
  const name = path.basename(sys);
  if (!/^[a-zA-Z0-9_!-]+$/.test(name)) fail('Invalid Linux block-device name.');
  return [`/dev/${name}`];
}

async function mountDisks(mount: LinuxMount, seen: Set<string>): Promise<string[]> {
  if (['tmpfs', 'ramfs'].includes(mount.type)) return [];
  if (mount.type === 'fuseblk') {
    // ntfs-3g uses a synthetic FUSE device number; its mount source identifies
    // the actual block device. Never accept a regular file or arbitrary source.
    if (!mount.source.startsWith('/dev/')) fail('FUSE block filesystem source is unavailable.');
    const source = await realpath(mount.source);
    if (!(await stat(source)).isBlockDevice()) fail('FUSE block filesystem source is not a block device.');
    return linuxBlockDisks(`/sys/class/block/${path.basename(source)}`, seen);
  }
  if (['fuse.AppImage', 'fuse.squashfuse'].includes(mount.type)) {
    fail('The application is running from a FUSE image. Launch the AppImage with --appimage-extract-and-run, or use an installed package, before creating a USB.');
  }
  if (mount.type === 'btrfs') {
    // Btrfs subvolumes have synthetic st_dev values. Its sysfs membership also
    // includes every member of a multi-device filesystem, not just SOURCE.
    const data = JSON.parse(await runTool('/usr/bin/findmnt', ['--json', '--target', mount.mountpoint, '--output', 'UUID,FSTYPE']));
    const filesystems = data.filesystems;
    if (!Array.isArray(filesystems) || filesystems.length !== 1 || filesystems[0].fstype !== 'btrfs' ||
        !/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i.test(filesystems[0].uuid ?? '')) fail('Btrfs filesystem identity is unavailable.');
    const devices = `/sys/fs/btrfs/${filesystems[0].uuid}/devices`;
    const members = await readdir(devices);
    if (!members.length) fail('Btrfs backing devices are unavailable.');
    return [...new Set((await Promise.all(members.map(name => linuxBlockDisks(`${devices}/${name}`, seen)))).flat())];
  }
  return linuxBlockDisks(`/sys/dev/block/${mount.device}`, seen);
}

export async function linuxBackingDisks(filename: string, seen = new Set<string>()): Promise<string[]> {
  const canonical = await realpath(filename);
  const metadata = await stat(canonical);
  if (metadata.isBlockDevice()) return linuxBlockDisks(`/sys/class/block/${path.basename(canonical)}`, seen);
  const mounts = parseLinuxMounts(await readFile('/proc/self/mountinfo', 'utf8'));
  const candidates = mounts.filter(m => canonical === m.mountpoint || canonical.startsWith(m.mountpoint === '/' ? '/' : m.mountpoint + '/'));
  candidates.sort((a, b) => b.mountpoint.length - a.mountpoint.length || b.id - a.id);
  if (!candidates.length) fail('Cannot find the Linux filesystem containing the source or application.');
  assertVisibleMount(candidates[0], mounts);
  return mountDisks(candidates[0], seen);
}

export async function linuxSwapPaths(): Promise<string[]> {
  const lines = (await readFile('/proc/swaps', 'utf8')).trim().split('\n');
  return lines.slice(1).filter(Boolean).map(line => {
    const filename = decode(line.split(/\s+/)[0]);
    if (!filename.startsWith('/')) fail('Linux swap backing storage is unavailable.');
    return filename;
  });
}

export async function linuxHasHolders(sys: string): Promise<boolean> {
  if ((await readdir(`${sys}/holders`)).length) return true;
  for (const child of await readdir(sys)) {
    if (await exists(`${sys}/${child}/partition`) && (await readdir(`${sys}/${child}/holders`)).length) return true;
  }
  return false;
}

export async function linuxHardwareId(device: string): Promise<string> {
  const sys = await realpath(`/sys/class/block/${path.basename(device)}`);
  if (!sys.startsWith('/sys/devices/')) fail('Device sysfs ancestry is unavailable.');
  if (await exists(`${sys}/partition`)) throw new PhysicalWriteError('NOT_WHOLE_DEVICE', 'Partitions cannot be written.');
  if (await linuxHasHolders(sys)) throw new PhysicalWriteError('DEVICE_IN_USE', 'Device or its partitions have active storage holders.');
  let current = sys;
  let serial = '';
  while (current.startsWith('/sys/devices/')) {
    if (await exists(`${current}/idVendor`) && await exists(`${current}/idProduct`)) {
      try { serial = (await readFile(`${current}/serial`, 'utf8')).trim(); }
      catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
      // This is the disk's own USB device. Never inherit an upstream hub serial.
      break;
    }
    current = path.dirname(current);
  }
  if (!current.startsWith('/sys/devices/')) throw new PhysicalWriteError('IDENTITY_UNAVAILABLE', 'USB device ancestry is unavailable.');
  if (!serial) {
    try { serial = (await readFile(`${sys}/device/serial`, 'utf8')).trim(); }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
  }
  if (!serial) throw new PhysicalWriteError('IDENTITY_UNAVAILABLE', 'USB hardware serial is unavailable.');
  const dev = (await readFile(`${sys}/dev`, 'utf8')).trim();
  return JSON.stringify({ serial, sys, dev });
}

/** Exact device membership, including aliases and bind mounts; never a pathname prefix. */
export async function linuxTargetMounts(device: string): Promise<LinuxMount[]> {
  if (!/^\/dev\/sd[a-z]+$/.test(device)) fail('Expected a whole Linux USB disk.');
  const sys = await realpath(`/sys/class/block/${path.basename(device)}`);
  const numbers = new Set([(await readFile(`${sys}/dev`, 'utf8')).trim()]);
  for (const name of await readdir(sys)) {
    if (await exists(`${sys}/${name}/partition`)) numbers.add((await readFile(`${sys}/${name}/dev`, 'utf8')).trim());
  }
  const result: LinuxMount[] = [];
  const mounts = parseLinuxMounts(await readFile('/proc/self/mountinfo', 'utf8'));
  for (const mount of mounts) {
    if (['btrfs', 'fuseblk'].includes(mount.type)) {
      const disks = await mountDisks(mount, new Set());
      if (disks.includes(device)) {
        if (disks.length !== 1) fail('Cannot unmount a filesystem shared with another disk.');
        result.push(mount);
      }
    } else if (numbers.has(mount.device)) result.push(mount);
  }
  for (const mount of result) assertVisibleMount(mount, mounts);
  return result.sort((a, b) => b.mountpoint.length - a.mountpoint.length || b.id - a.id);
}

export async function unmountLinuxTarget(device: string): Promise<void> {
  for (const mount of await linuxTargetMounts(device)) {
    const fresh = (await linuxTargetMounts(device)).find(candidate => candidate.id === mount.id);
    if (!fresh || fresh.mountpoint !== mount.mountpoint || fresh.device !== mount.device || fresh.parent !== mount.parent) {
      throw new PhysicalWriteError('UNMOUNT_FAILED', 'The USB mount layout changed before unmounting.');
    }
    // No lazy or forced fallback. Busy filesystems abort before any raw write.
    await runTool('/usr/bin/umount', ['--', mount.mountpoint]);
  }
  if ((await linuxTargetMounts(device)).length) throw new PhysicalWriteError('UNMOUNT_FAILED', 'The selected USB still has mounted filesystems.');
}
