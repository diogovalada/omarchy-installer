/** Protocol 1 is deliberately independent of the existing file-only adapter. */
export interface DriveIdentity {
  fingerprint: string;
  device: string;
  raw: string;
  devicePath: string | null;
  hardwareId: string;
  size: number;
  blockSize: number;
  logicalBlockSize: number;
  description: string;
  busType: string;
}

export interface PhysicalDrive {
  identity: DriveIdentity;
  mountpoints: string[];
  eligible: boolean;
  reasons: string[];
}

export interface PhysicalWriteRequest {
  protocol: 1;
  action: 'write';
  /** Already authenticated raw local image; no URL or decompression support. */
  sourcePath: string;
  length: number;
  sha256: string;
  /** Exact identity returned by list. A pathname alone is never accepted. */
  target: DriveIdentity;
}

export type PhysicalCommand =
  | { protocol: 1; action: 'probe' }
  | { protocol: 1; action: 'list'; sourcePath?: string }
  | PhysicalWriteRequest;

export interface PhysicalProbe {
  available: boolean;
  platform: string;
  arch: string;
  engine: 'etcher-sdk';
  engineVersion: '10.2.14';
  reason: string | null;
  capabilities: {
    discovery: boolean;
    write: boolean;
    requiresElevation: true;
    mandatoryVerification: true;
    fullReadback: true;
    hardwareQualified: false;
  };
}

export interface PhysicalWriteReceipt {
  engine: 'etcher-sdk';
  engineVersion: '10.2.14';
  destinationKind: 'usb-whole-device';
  target: DriveIdentity;
  /** Logical authenticated image bytes, excluding any final-sector zero padding. */
  bytesWritten: number;
  /** Physical write boundary: image length rounded up to one physical sector. */
  writeSpanBytes: number;
  /** At most blockSize - 1 bytes, always zero and never included in sha256. */
  paddingBytes: number;
  sha256: string;
  sdkVerification: true;
  flushed: true;
  fullReadbackVerification: true;
  /** Readback covers every image byte, not unused capacity or power-loss durability. */
  readbackBytes: number;
  /** Full physical span is read; only original image bytes feed the SHA-256. */
  readbackSpanBytes: number;
  paddingVerification: true;
  eject: { status: 'ejected' | 'unmounted' | 'failed'; message: string };
  hardwareQualified: false;
}

export type PhysicalStage = 'validating' | 'hashing' | 'unmounting' | 'writing' |
  'verifying' | 'flushing' | 'readback' | 'ejecting';
export type PhysicalEvent =
  | { protocol: 1; type: 'event'; stage: PhysicalStage; bytes?: number; totalBytes?: number }
  | { protocol: 1; type: 'result'; action: 'probe'; result: PhysicalProbe }
  | { protocol: 1; type: 'result'; action: 'list'; result: { drives: PhysicalDrive[] } }
  | { protocol: 1; type: 'result'; action: 'write'; result: PhysicalWriteReceipt }
  | { protocol: 1; type: 'error'; code: string; message: string; possiblyModified: boolean };

export class PhysicalWriteError extends Error {
  constructor(public readonly code: string, message: string) {
    super(message);
    this.name = 'PhysicalWriteError';
  }
}
