import { chmodSync, renameSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

// FUSE mounts normally cannot be read by the separately elevated USB helper.
// Relaunch once through the AppImage runtime's extraction mode before opening UI.
export function installPortableLauncher(appdir) {
  renameSync(join(appdir, 'AppRun'), join(appdir, 'AppRun.original'));
  writeFileSync(join(appdir, 'AppRun'), `#!/bin/sh
set -eu
if [ -n "\${APPIMAGE:-}" ] && [ "\${APPIMAGE_EXTRACT_AND_RUN:-}" != 1 ] && [ "\${OMARCHY_PORTABLE_EXTRACTED:-}" != 1 ]; then
  export APPIMAGE_EXTRACT_AND_RUN=1 OMARCHY_PORTABLE_EXTRACTED=1
  exec "$APPIMAGE" --appimage-extract-and-run "$@"
fi
exec "\${APPDIR:?AppImage application directory is missing}/AppRun.original" "$@"
`, { flag: 'wx', mode: 0o755 });
  chmodSync(join(appdir, 'AppRun'), 0o755);
}
