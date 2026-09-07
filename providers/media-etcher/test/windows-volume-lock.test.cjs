const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

// The real helper is executed with mock Storage commands and a managed-only
// volume API. No P/Invoke is loaded and no disk or volume handle is opened.
const wrapper = String.raw`
param([string]$Helper, [string]$Scenario)
$ErrorActionPreference = 'Stop'
$fixture = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($Scenario)) | ConvertFrom-Json
Microsoft.PowerShell.Utility\Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using Microsoft.Win32.SafeHandles;
public static class OmarchyVolumeLock {
  public static List<string> Calls = new List<string>();
  public static List<SafeFileHandle> Handles = new List<SafeFileHandle>();
  public static int FailControl;
  public static int Controls;
  public static SafeFileHandle CreateFileW(string name, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template) {
    Calls.Add("open:" + name);
    var handle = new SafeFileHandle(new IntPtr(123), false);
    Handles.Add(handle); return handle;
  }
  public static void Control(SafeFileHandle handle, uint code) {
    Calls.Add("control:" + code); Controls++;
    if (Controls == FailControl) throw new Exception("Injected volume busy");
  }
}
'@
[OmarchyVolumeLock]::FailControl = $fixture.failControl
function Add-Type { param($TypeDefinition) } # The native P/Invoke body is never compiled.
function Get-Disk {
  [CmdletBinding()]param($Number)
  if ($Number -ne 2147483647) { throw 'Unexpected disk query' }
  [pscustomobject]@{ UniqueId='fixture'; SerialNumber='fixture'; Path='fixture'; Size=1048576;
    IsBoot=[bool]$fixture.isBoot; IsSystem=$false; IsReadOnly=$false; IsOffline=$false; BusType='USB';
    NumberOfPartitions=$fixture.partitionCount }
}
function Get-Partition {
  [CmdletBinding()]param($DiskNumber)
  throw 'No MSFT_Partition objects found with property DiskNumber (old query regression)'
}
function Get-CimInstance {
  [CmdletBinding()]param($Namespace, $ClassName, $Filter)
  if ($Namespace -cne 'root/Microsoft/Windows/Storage' -or $ClassName -cne 'MSFT_Partition' -or $Filter -cne 'DiskNumber = 2147483647') { throw 'Unexpected partition query' }
  if ($fixture.queryError) { throw 'Injected storage query failure' }
  $fixture.partitions
}
$expected = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('{"uniqueId":"fixture","serialNumber":"fixture","path":"fixture","size":1048576}'))
$global:LASTEXITCODE = 0
& $Helper -DiskNumber 2147483647 -ExpectedIdentity $expected
$result = $LASTEXITCODE
@{calls=@([OmarchyVolumeLock]::Calls); closed=@([OmarchyVolumeLock]::Handles | ForEach-Object {$_.IsClosed})} | ConvertTo-Json -Compress
exit $result
`;

function run(scenario) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'omarchy-volume-test-'));
  const filename = path.join(directory, 'fixture.ps1');
  try {
    fs.writeFileSync(filename, wrapper);
    const result = spawnSync(path.join(process.env.SystemRoot, 'System32/WindowsPowerShell/v1.0/powershell.exe'),
      ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', filename,
        '-Helper', path.resolve(__dirname, '../scripts/windows-volume-lock.ps1'),
        '-Scenario', Buffer.from(JSON.stringify({ failControl: 0, partitionCount: 0, partitions: [], ...scenario })).toString('base64')],
      { encoding: 'utf8', input: '', windowsHide: true, timeout: 15000 });
    assert.equal(result.error, undefined);
    const lines = result.stdout.trim().split(/\r?\n/);
    return { status: result.status, ready: lines.includes('{"ready":true}'), state: JSON.parse(lines.at(-1)), error: result.stderr };
  } finally { fs.rmSync(filename); fs.rmdirSync(directory); }
}
const windows = { skip: process.platform !== 'win32' };
test('blank USB with zero partitions is ready without opening any volumes', windows, () => {
  const result = run({});
  assert.equal(result.status, 0, result.error); assert.equal(result.ready, true);
  assert.deepEqual(result.state.calls, []);
});
test('partition query errors and missing partitions fail before volume access', windows, () => {
  for (const scenario of [{ queryError: true }, { partitionCount: 1 }, { isBoot: true }]) {
    const result = run(scenario);
    assert.equal(result.status, 1); assert.equal(result.ready, false); assert.deepEqual(result.state.calls, []);
  }
});
test('failure locking a later volume releases every previously acquired handle', windows, () => {
  const partitions = [1, 2].map(n => ({ DiskNumber: 2147483647, AccessPaths: [`\\\\?\\Volume{00000000-0000-0000-0000-00000000000${n}}\\`] }));
  const result = run({ partitionCount: 2, partitions, failControl: 3 });
  assert.equal(result.status, 1); assert.equal(result.ready, false);
  assert.equal(result.state.calls.filter(call => call.startsWith('open:')).length, 2);
  assert.deepEqual(result.state.closed, [true, true]);
});
