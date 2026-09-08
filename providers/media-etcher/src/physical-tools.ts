import { execFile } from 'node:child_process';
import * as path from 'node:path';
import { PhysicalWriteError } from './physical-contracts.js';

/** Fixed system tools only. Paths and device names are arguments, never shell code. */
export function runTool(executable: string, args: string[], input?: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = execFile(executable, args, { windowsHide: true, encoding: 'utf8', timeout: 30000,
      maxBuffer: 32 * 1024 * 1024, env: { ...process.env, LC_ALL: 'C', LANG: 'C' } },
    (error, stdout, stderr) => error ? reject(new PhysicalWriteError('PLATFORM_PREREQUISITE',
      `${path.basename(executable)} failed: ${stderr.trim() || error.message}`)) : resolve(stdout));
    child.stdin?.end(input);
  });
}
