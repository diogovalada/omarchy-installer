const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

test('production native Windows query handles unsupported, failed, and conflicting responses', { skip: process.platform !== 'win32' }, () => {
  const vswhere = path.join(process.env['ProgramFiles(x86)'], 'Microsoft Visual Studio/Installer/vswhere.exe');
  const vs = execFileSync(vswhere, ['-latest', '-products', '*', '-requires',
    'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath'], { encoding: 'utf8', windowsHide: true }).trim();
  assert.ok(vs, 'Windows native tests require the MSVC build tools used by the provider.');
  const output = fs.mkdtempSync(path.join(os.tmpdir(), 'omarchy-geometry-test-'));
  try {
    const command = path.join(output, 'test.cmd');
    fs.writeFileSync(command, '@echo off\r\ncall "%OMARCHY_VCVARS%" >nul\r\nif errorlevel 1 exit /b 1\r\ncl /nologo /W4 /WX /TC "%OMARCHY_TEST_SOURCE%" /Fe:"%OMARCHY_TEST_EXE%" /Fo:"%OMARCHY_TEST_OBJ%"\r\nif errorlevel 1 exit /b 1\r\n"%OMARCHY_TEST_EXE%"\r\n');
    const result = execFileSync(process.env.ComSpec, ['/d', '/s', '/c', '"' + command + '"'], {
      encoding: 'utf8', windowsHide: true, windowsVerbatimArguments: true, env: { ...process.env,
        OMARCHY_VCVARS: path.join(vs, 'VC/Auxiliary/Build/vcvars64.bat'),
        OMARCHY_TEST_SOURCE: path.join(__dirname, 'windows-geometry-native.c'),
        OMARCHY_TEST_EXE: path.join(output, 'test.exe'), OMARCHY_TEST_OBJ: path.join(output, 'test.obj') },
    });
    assert.match(result, /14 native Windows geometry scenarios passed/);
  } finally {
    // Only the fresh test directory's known generated files are removed.
    for (const name of ['test.cmd', 'test.exe', 'test.obj']) fs.rmSync(path.join(output, name), { force: true });
    fs.rmdirSync(output);
  }
});
