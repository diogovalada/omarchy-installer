import { fail, validSector, type WindowsDisk } from './physical-discovery.js';

export interface HandleGeometry {
  size: number;
  logicalSectorSize: number;
  physicalSectorSize: number;
  alignmentQueryError?: number;
}

/** Resolve only an explicitly unsupported Windows alignment query. The OS disk
 * must be freshly reidentified while retaining the queried exclusive handle.
 * Capacity and logical sectors always come from that actual handle.
 */
export function resolveHandleGeometry(geometry: HandleGeometry, windows?: WindowsDisk): HandleGeometry {
  if (!Number.isSafeInteger(geometry.size) || geometry.size <= 0 ||
      !validSector(geometry.logicalSectorSize) || geometry.size % geometry.logicalSectorSize !== 0) {
    fail('HANDLE_GEOMETRY_INVALID', 'Opened device reports invalid capacity or logical sectors.');
  }
  let physicalSectorSize = geometry.physicalSectorSize;
  if (geometry.alignmentQueryError) {
    if (![1, 50].includes(geometry.alignmentQueryError) || physicalSectorSize !== 0 || !windows ||
        windows.BusType !== 'USB' || windows.IsBoot || windows.IsSystem || windows.IsOffline || windows.IsReadOnly ||
        windows.Size !== geometry.size || windows.LogicalSectorSize !== geometry.logicalSectorSize ||
        !validSector(windows.PhysicalSectorSize)) {
      fail('HANDLE_GEOMETRY_UNAVAILABLE', 'USB alignment query is unsupported and fresh Windows geometry could not be confirmed.');
    }
    physicalSectorSize = windows.PhysicalSectorSize;
  }
  if (!validSector(physicalSectorSize) || physicalSectorSize % geometry.logicalSectorSize !== 0) {
    fail('HANDLE_GEOMETRY_INVALID', 'Opened device reports invalid physical sectors.');
  }
  return { ...geometry, physicalSectorSize };
}
