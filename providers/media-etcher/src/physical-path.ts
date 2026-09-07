import { PhysicalWriteError } from './physical-contracts.js';

/** Node 22 resolves a bare \\.\PhysicalDriveN as a UNC share root and
 * appends a backslash before opening it. Use the same global DOS-device alias
 * through the NT root, where the disk name is a leaf rather than a share root.
 * Discovery, consent and reidentification keep the original device identity.
 */
export function physicalOpenPath(raw: string, platform: NodeJS.Platform = process.platform): string {
  if (platform !== 'win32') return raw;
  const match = /^\\\\\.\\(PhysicalDrive(?:0|[1-9]\d*))$/i.exec(raw);
  if (!match || !Number.isSafeInteger(Number(match[1].slice('PhysicalDrive'.length)))) {
    throw new PhysicalWriteError('INVALID_DEVICE_PATH', 'Expected an exact Windows physical disk device path.');
  }
  return `\\\\?\\GLOBALROOT\\GLOBAL??\\${match[1]}`;
}
