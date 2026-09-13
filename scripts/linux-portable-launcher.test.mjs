import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { test } from 'node:test';
import { installPortableLauncher } from './linux-portable-launcher.mjs';

test('portable launcher extracts once and preserves arguments, including spaces', {skip: process.platform === 'win32'}, t => {
  const root = mkdtempSync(join(tmpdir(), 'omarchy-apprun-'));
  t.after(() => {
    assert.equal(dirname(resolve(root)), resolve(tmpdir()));
    assert.match(basename(root), /^omarchy-apprun-[A-Za-z0-9]+$/);
    rmSync(root, {recursive:true, force:true});
  });
  function script(name, content) {
    const path = join(root, name);
    writeFileSync(path, '#!/bin/sh\nset -eu\n' + content);
    chmodSync(path, 0o755);
    return path;
  }
  script('AppRun', 'printf "%s\\n" "$@" > "$APPDIR/arguments"\n');
  const image = script('mock image', 'test "$1" = --appimage-extract-and-run\nshift\nprintf x >> "$APPDIR/extractions"\nexec "$APPDIR/AppRun" "$@"\n');
  installPortableLauncher(root);
  const env = {...process.env, APPDIR:root, APPIMAGE:image, APPIMAGE_EXTRACT_AND_RUN:'', OMARCHY_PORTABLE_EXTRACTED:''};
  const run = overrides => spawnSync(join(root, 'AppRun'), ['path with spaces', '--flag'], {env:{...env,...overrides}, encoding:'utf8'});
  const first = run({});
  assert.equal(first.status, 0, first.stderr);
  assert.equal(readFileSync(join(root, 'extractions'), 'utf8'), 'x');
  assert.equal(readFileSync(join(root, 'arguments'), 'utf8'), 'path with spaces\n--flag\n');
  assert.equal(run({APPIMAGE_EXTRACT_AND_RUN:'1'}).status, 0);
  assert.equal(readFileSync(join(root, 'extractions'), 'utf8'), 'x');
});
