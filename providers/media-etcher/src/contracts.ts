export interface FileWriteRequest {
  /** Absolute, local, existing regular file. Raw bytes only; no decompression. */
  sourcePath: string;
  /** Absolute path to a NEW regular file in an existing local directory. */
  destinationPath: string;
  /** Required digest from the caller's independently verified artifact record. */
  expectedSha256: string;
}

export type FileWriteEvent =
  | { type: 'validating' }
  | { type: 'writing' | 'verifying'; bytes: number; totalBytes: number }
  | { type: 'readback'; bytes: number; totalBytes: number }
  | { type: 'completed'; receipt: FileWriteReceipt }
  | { type: 'cancelled'; message: string }
  | { type: 'error'; code: string; message: string };

export interface FileWriteReceipt {
  engine: 'etcher-sdk';
  engineVersion: '10.2.14';
  destinationKind: 'regular-file';
  bytesWritten: number;
  sha256: string;
  sdkVerification: true;
  fullReadbackVerification: true;
}

export class FileWriteError extends Error {
  constructor(public readonly code: string, message: string) {
    super(message);
    this.name = 'FileWriteError';
  }
}
