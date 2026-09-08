const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const tools = require('../dist/physical-tools.js');
const linux = require('../dist/physical-linux.js');

// A synthetic sysfs and mount table. Unknown reads/commands fail instead of
// inspecting or unmounting any device attached to the test host.
function filesystem(t, files = {}, dirs = {}, links = {}, blocks = []) {
  const missing = name => Object.assign(new Error(`Fixture has no ${name}`), { code: 'ENOENT' });
  t.mock.method(fs, 'readFile', async name => {
    if (!(name in files)) throw missing(name);
    return files[name];
  });
  t.mock.method(fs, 'readdir', async name => {
    if (!(name in dirs)) throw missing(name);
    return dirs[name];
  });
  t.mock.method(fs, 'realpath', async name => {
    if (name in links) return links[name];
    if (name in files || name in dirs) return name;
    throw missing(name);
  });
  t.mock.method(fs, 'stat', async name => {
    if (!(name in files) && !(name in dirs) && !blocks.includes(name)) throw missing(name);
    return { isBlockDevice: () => blocks.includes(name) };
  });
  t.mock.method(tools, 'runTool', async (tool, args) => { throw new Error(`Unexpected OS command ${tool} ${args}`); });
  return { files, dirs, links };
}

const disk = '/sys/devices/pci0000:00/nvme/nvme0/nvme0n1';
const part = `${disk}/nvme0n1p2`;
const mapper = '/sys/devices/virtual/block/dm-0';

test('Linux source and root on LUKS/LVM resolve through every storage layer', async t => {
  filesystem(t, { '/home/example/image.iso': '', '/proc/self/mountinfo': '31 1 253:0 / / rw - ext4 /dev/mapper/root rw\n',
    [`${part}/partition`]: '2' }, { [`${mapper}/slaves`]: ['nvme0n1p2'], [`${disk}/slaves`]: [] },
  { '/sys/dev/block/253:0': mapper, [`${mapper}/slaves/nvme0n1p2`]: part, [disk]: disk });
  assert.deepEqual(await linux.linuxBackingDisks('/home/example/image.iso'), ['/dev/nvme0n1']);
});

test('Btrfs subvolumes protect every filesystem member despite synthetic device IDs', async t => {
  const second = '/sys/devices/pci0000:01/block/sdc';
  const uuid = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee';
  const devices = `/sys/fs/btrfs/${uuid}/devices`;
  filesystem(t, { '/home/example/image.iso': '', '/proc/self/mountinfo': '31 1 0:43 /@home /home rw - btrfs /dev/dm-0 rw\n',
    [`${part}/partition`]: '2' }, { [devices]: ['dm-0', 'sdc'], [`${mapper}/slaves`]: ['nvme0n1p2'], [`${disk}/slaves`]: [], [`${second}/slaves`]: [] },
  { [`${devices}/dm-0`]: mapper, [`${devices}/sdc`]: second, [`${mapper}/slaves/nvme0n1p2`]: part, [disk]: disk });
  t.mock.method(tools, 'runTool', async (exe, args) => {
    assert.equal(exe, '/usr/bin/findmnt');
    assert.deepEqual(args, ['--json', '--target', '/home', '--output', 'UUID,FSTYPE']);
    return JSON.stringify({ filesystems: [{ uuid, fstype: 'btrfs' }] });
  });
  assert.deepEqual(await linux.linuxBackingDisks('/home/example/image.iso'), ['/dev/nvme0n1', '/dev/sdc']);
});

test('unresolved layered devices and ancestry cycles are refused', async t => {
  filesystem(t, {}, { [`${mapper}/slaves`]: [] }, { '/sys/dev/block/253:0': mapper });
  await assert.rejects(linux.linuxBlockDisks('/sys/dev/block/253:0'), /no resolvable backing disk/);
  await assert.rejects(linux.linuxBlockDisks('/sys/dev/block/253:0', new Set([mapper])), /ambiguous/);
});

test('an active holder on any USB partition makes the whole disk busy', async t => {
  const sys = '/sys/devices/pci0000:00/usb1/block/sdb';
  filesystem(t, { [`${sys}/sdb1/partition`]: '1' }, { [sys]: ['sdb1', 'size'], [`${sys}/holders`]: [], [`${sys}/sdb1/holders`]: ['dm-1'] });
  assert.equal(await linux.linuxHasHolders(sys), true);
});

test('Linux unmount uses exact device numbers, aliases and nested bind mounts', async t => {
  const sys = '/sys/devices/pci0000:00/usb1/block/sdb';
  const unrelated = '34 1 8:432 / /media/other rw - vfat /dev/sdba rw';
  const env = filesystem(t, { [`${sys}/dev`]: '8:16', [`${sys}/sdb1/partition`]: '1', [`${sys}/sdb1/dev`]: '8:17',
    '/proc/self/mountinfo': ['31 1 8:17 / /media/My\\040USB rw - vfat /dev/disk/by-label/MYUSB rw',
      '32 31 8:17 /sub /media/My\\040USB/nested rw - vfat /dev/sdb1 rw', unrelated].join('\n') },
  { [sys]: ['sdb1', 'dev'] }, { '/sys/class/block/sdb': sys });
  const calls = [];
  t.mock.method(tools, 'runTool', async (exe, args) => {
    assert.equal(exe, '/usr/bin/umount'); assert.equal(args[0], '--');
    calls.push(args[1]);
    env.files['/proc/self/mountinfo'] = env.files['/proc/self/mountinfo'].split('\n')
      .filter(line => linux.parseLinuxMounts(line)[0].mountpoint !== args[1]).join('\n');
    return '';
  });
  await linux.unmountLinuxTarget('/dev/sdb');
  assert.deepEqual(calls, ['/media/My USB/nested', '/media/My USB']);
  assert.equal(env.files['/proc/self/mountinfo'], unrelated);
});

test('a busy Linux mount aborts without a lazy or forced retry', async t => {
  const sys = '/sys/devices/pci0000:00/usb1/block/sdb';
  filesystem(t, { [`${sys}/dev`]: '8:16', '/proc/self/mountinfo': '31 1 8:16 / /media/USB rw - vfat /dev/sdb rw' },
    { [sys]: ['dev'] }, { '/sys/class/block/sdb': sys });
  const calls = [];
  t.mock.method(tools, 'runTool', async (exe, args) => { calls.push([exe, ...args]); throw new Error('Device busy'); });
  await assert.rejects(linux.unmountLinuxTarget('/dev/sdb'), /Device busy/);
  assert.deepEqual(calls, [['/usr/bin/umount', '--', '/media/USB']]);
});

test('zram swap is memory-backed and swap paths preserve escaped spaces', async t => {
  const zram = '/sys/devices/virtual/block/zram0';
  filesystem(t, { '/proc/swaps': 'Filename Type Size Used Priority\n/dev/zram0 partition 1024 0 100\n/home/swap\\040file file 1024 0 -2\n' },
    { [`${zram}/slaves`]: [] }, { '/sys/class/block/zram0': zram });
  assert.deepEqual(await linux.linuxBlockDisks('/sys/class/block/zram0'), []);
  assert.deepEqual(await linux.linuxSwapPaths(), ['/dev/zram0', '/home/swap file']);
});

test('Linux USB identity never borrows an upstream hub serial', async t => {
  const hub = '/sys/devices/pci0000:00/usb1/1-1';
  const usb = `${hub}/1-1.2`;
  const sys = `${usb}/1-1.2:1.0/host1/target1/block/sdb`;
  const env = filesystem(t, { [`${hub}/serial`]: 'hub-only', [`${hub}/idVendor`]: '0001', [`${hub}/idProduct`]: '0002',
    [`${usb}/idVendor`]: '0003', [`${usb}/idProduct`]: '0004', [`${sys}/dev`]: '8:16' },
  { [sys]: ['dev', 'holders'], [`${sys}/holders`]: [] }, { '/sys/class/block/sdb': sys });
  await assert.rejects(linux.linuxHardwareId('/dev/sdb'), error => error.code === 'IDENTITY_UNAVAILABLE');
  env.files[`${usb}/serial`] = 'own-usb-serial';
  assert.deepEqual(JSON.parse(await linux.linuxHardwareId('/dev/sdb')), { serial: 'own-usb-serial', sys, dev: '8:16' });
  delete env.files[`${usb}/serial`];
  env.files[`${sys}/device/serial`] = 'own-scsi-serial';
  assert.equal(JSON.parse(await linux.linuxHardwareId('/dev/sdb')).serial, 'own-scsi-serial');
});

for (const coveringMount of [
  '32 31 8:33 / /media/usb/child rw - ext4 /dev/sdc1 rw',
  '32 1 8:33 / /media/usb rw - ext4 /dev/sdc1 rw',
]) {
  test(`Linux refuses a hidden target mount before any unmount: ${coveringMount}`, async t => {
    const sys = '/sys/devices/pci0000:00/usb1/block/sdb';
    filesystem(t, { [`${sys}/dev`]: '8:16', '/media/usb/child/source.iso': '',
      '/proc/self/mountinfo': `1 1 8:1 / / rw - ext4 /dev/sda1 rw\n31 1 8:16 / /media/usb/child rw - vfat /dev/sdb rw\n${coveringMount}` },
    { [sys]: ['dev'] }, { '/sys/class/block/sdb': sys });
    await assert.rejects(linux.unmountLinuxTarget('/dev/sdb'), /overlapping mounts/);
    // With an overmounted ancestor, longest-path matching must not guess the
    // now-hidden filesystem as the backing disk for a visible source file.
    if (coveringMount.includes(' /media/usb rw')) {
      await assert.rejects(linux.linuxBackingDisks('/media/usb/child/source.iso'), /overlapping mounts/);
    }
  });
}

test('Linux rechecks mount identity immediately before each unmount', async t => {
  const sys = '/sys/devices/pci0000:00/usb1/block/sdb';
  filesystem(t, { [`${sys}/dev`]: '8:16' }, { [sys]: ['dev'] }, { '/sys/class/block/sdb': sys });
  let reads = 0;
  t.mock.method(fs, 'readFile', async filename => {
    if (filename === `${sys}/dev`) return '8:16';
    assert.equal(filename, '/proc/self/mountinfo');
    return `${++reads === 1 ? 31 : 32} 1 8:16 / /media/usb rw - vfat /dev/sdb rw`;
  });
  await assert.rejects(linux.unmountLinuxTarget('/dev/sdb'), /layout changed/);
});

test('Linux resolves and normally unmounts NTFS-3G through its real block source', async t => {
  const sys = '/sys/devices/pci0000:00/usb1/block/sdb';
  const partition = `${sys}/sdb1`;
  const env = filesystem(t, { '/media/NTFS/source.iso': '', [`${sys}/dev`]: '8:16',
    [`${partition}/partition`]: '1', [`${partition}/dev`]: '8:17',
    '/proc/self/mountinfo': '31 1 0:45 / /media/NTFS rw - fuseblk /dev/disk/by-label/NTFS rw' },
  { [sys]: ['sdb1', 'dev'], [`${sys}/slaves`]: [] },
  { '/sys/class/block/sdb': sys, '/sys/class/block/sdb1': partition, [sys]: sys, '/dev/disk/by-label/NTFS': '/dev/sdb1' }, ['/dev/sdb1']);
  assert.deepEqual(await linux.linuxBackingDisks('/media/NTFS/source.iso'), ['/dev/sdb']);
  const calls = [];
  t.mock.method(tools, 'runTool', async (exe, args) => {
    calls.push([exe, args]); env.files['/proc/self/mountinfo'] = ''; return '';
  });
  await linux.unmountLinuxTarget('/dev/sdb');
  assert.deepEqual(calls, [['/usr/bin/umount', ['--', '/media/NTFS']]]);
});

test('Linux gives an actionable refusal for a FUSE-mounted AppImage runtime', async t => {
  filesystem(t, { '/tmp/.mount_omarchy/usr/bin/node': '',
    '/proc/self/mountinfo': '31 1 0:46 / /tmp/.mount_omarchy ro - fuse.AppImage omarchy.AppImage ro' });
  await assert.rejects(linux.linuxBackingDisks('/tmp/.mount_omarchy/usr/bin/node'), /--appimage-extract-and-run/);
});
