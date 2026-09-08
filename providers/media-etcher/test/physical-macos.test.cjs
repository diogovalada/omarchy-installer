const { test } = require('node:test');
const assert = require('node:assert/strict');
const { parseMacUsbRegistry, parseMacApfsStores, parseMacBackingDisks } = require('../dist/physical-macos.js');
const { fingerprint, sameIdentity } = require('../dist/physical-discovery.js');

function xml(value) {
  if (Buffer.isBuffer(value)) return `<data>${value.toString('base64')}</data>`;
  if (Array.isArray(value)) return `<array>${value.map(xml).join('')}</array>`;
  if (typeof value === 'object') return `<dict>${Object.entries(value).map(([key, child]) => `<key>${key}</key>${xml(child)}`).join('')}</dict>`;
  if (typeof value === 'number') return `<integer>${value}</integer>`;
  if (typeof value === 'boolean') return value ? '<true/>' : '<false/>';
  return `<string>${value}</string>`;
}
const plist = value => `<?xml version="1.0" encoding="UTF-8"?><plist version="1.0">${xml(value)}</plist>`;
const media = overrides => ({ 'BSD Name': 'disk2', Whole: true, Size: 16000000000, 'Preferred Block Size': 512, IORegistryEntryID: 300, ...overrides });
const usb = children => ({ IOObjectClass: 'IOUSBHostDevice', 'USB Serial Number': 'fixture-usb-123', locationID: 1234,
  'Device Characteristics': { 'Physical Block Size': 4096, 'Logical Block Size': 512 }, IORegistryEntryChildren: children });

test('macOS USB discovery reads binary plist properties and real 512e geometry', () => {
  const result = parseMacUsbRegistry(plist([usb([media(), { 'USB Descriptor': Buffer.from([0, 255, 42]) }])]));
  assert.equal(result.size, 1);
  assert.deepEqual(result.get('/dev/disk2'), { hardwareId: JSON.stringify({ serial: 'fixture-usb-123', location: '1234', registryId: 300 }),
    physicalBlockSize: 4096, logicalBlockSize: 512, size: 16000000000 });
});
test('macOS excludes partitions and USB devices without their own serial', () => {
  const result = parseMacUsbRegistry(plist([usb([
    media({ 'BSD Name': 'disk2s1', Whole: false }),
    { IOObjectClass: 'IOUSBHostDevice', locationID: 222, IORegistryEntryChildren: [media()] },
  ])]));
  assert.equal(result.size, 0);
});
test('macOS rejects duplicate media identities and malformed registry input', () => {
  assert.throws(() => parseMacUsbRegistry(plist([usb([media(), media()])])), /ambiguous/);
  assert.throws(() => parseMacUsbRegistry('not a plist'));
  assert.throws(() => parseMacUsbRegistry(plist([usb([media({ IORegistryEntryID: 9007199254740992 })])])), /ambiguous/);
});
test('USB identity survives changing display labels but still binds hardware and geometry', () => {
  const target = { device: '/dev/sdb', raw: '/dev/sdb', devicePath: null, hardwareId: 'serial-and-attachment',
    size: 16000000000, blockSize: 4096, logicalBlockSize: 512, description: 'USB (/media/example)', busType: 'USB', fingerprint: '' };
  target.fingerprint = fingerprint(target);
  const unmounted = { ...target, description: 'USB' };
  assert.equal(fingerprint(unmounted), target.fingerprint);
  assert.equal(sameIdentity(target, unmounted), true);
  for (const changed of [{ hardwareId: 'another-serial' }, { size: 32000000000 }, { blockSize: 512 }, { raw: '/dev/sdc' }]) {
    assert.equal(sameIdentity(target, { ...target, ...changed }), false);
  }
});

test('macOS maps APFS snapshots and Fusion containers to every physical disk', () => {
  const stores = parseMacApfsStores(plist({ Containers: [{ ContainerReference: 'disk3', PhysicalStores: [
    { DeviceIdentifier: 'disk0s2' }, { DeviceIdentifier: 'disk1s2' },
  ] }] }));
  assert.deepEqual(parseMacBackingDisks(plist({ DeviceIdentifier: 'disk3s1s1', ParentWholeDisk: 'disk3',
    Whole: false, VirtualOrPhysical: 'Virtual', FilesystemType: 'apfs' }), stores), ['/dev/disk0', '/dev/disk1']);
  assert.deepEqual(parseMacBackingDisks(plist({ ParentWholeDisk: 'disk2', Whole: false,
    VirtualOrPhysical: 'Physical', FilesystemType: 'msdos' }), stores), ['/dev/disk2']);
  assert.throws(() => parseMacBackingDisks(plist({ ParentWholeDisk: 'disk4', VirtualOrPhysical: 'Virtual' }), stores), /unresolved|resolved/);
  assert.throws(() => parseMacApfsStores(plist({ Containers: [{ ContainerReference: 'disk3', PhysicalStores: [] }] })), /ambiguous/);
});
