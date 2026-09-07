param(
  [Parameter(Mandatory=$true)][ValidateRange(0,2147483647)][int]$DiskNumber,
  [Parameter(Mandatory=$true)][string]$ExpectedIdentity
)
# Fixed privileged helper. Not a general PowerShell endpoint. stdin is a lifetime
# pipe: EOF releases every retained volume handle, including after Node exits.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$heldVolumes = [System.Collections.Generic.List[Microsoft.Win32.SafeHandles.SafeFileHandle]]::new()
try {
  $expected = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($ExpectedIdentity)) | ConvertFrom-Json
  $disk = Get-Disk -Number $DiskNumber -ErrorAction Stop
  if ($disk.IsBoot -or $disk.IsSystem -or $disk.IsReadOnly -or $disk.IsOffline -or [string]$disk.BusType -cne 'USB' -or
      $disk.UniqueId.Trim() -cne $expected.uniqueId -or $disk.SerialNumber.Trim() -cne $expected.serialNumber -or
      $disk.Path -cne $expected.path -or [long]$disk.Size -ne [long]$expected.size) {
    throw 'Target identity or safety state changed before volume locking.'
  }
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class OmarchyVolumeLock {
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern SafeFileHandle CreateFileW(string name, uint access, uint share,
      IntPtr security, uint disposition, uint flags, IntPtr template);
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool DeviceIoControl(SafeFileHandle handle, uint code,
      IntPtr input, uint inputLength, IntPtr output, uint outputLength,
      out uint returned, IntPtr overlapped);
  public static void Control(SafeFileHandle handle, uint code) {
    uint returned;
    if (!DeviceIoControl(handle, code, IntPtr.Zero, 0, IntPtr.Zero, 0, out returned, IntPtr.Zero))
      throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
  }
}
'@
  # AccessPaths includes volume GUID names, including volumes without drive letters.
  # Get-Partition -DiskNumber raises ObjectNotFound for a valid blank disk.
  # A filtered CIM enumeration returns an empty set without suppressing real
  # query failures. Reconcile it with Get-Disk's partition count before locking.
  $partitions = @(Get-CimInstance -Namespace root/Microsoft/Windows/Storage -ClassName MSFT_Partition -Filter "DiskNumber = $DiskNumber" -ErrorAction Stop)
  if ($null -eq $disk.NumberOfPartitions -or $partitions.Count -ne $disk.NumberOfPartitions) {
    throw 'Target partition inventory is incomplete or changed before volume locking.'
  }
  $volumePattern = '^\\\\\?\\Volume\{[0-9a-fA-F-]{36}\}\\$'
  foreach ($partition in $partitions) {
    $accessPaths = @($partition.AccessPaths | Where-Object { $_ })
    if ($accessPaths.Count -gt 0 -and @($accessPaths | Where-Object { $_ -match $volumePattern }).Count -ne 1) {
      throw 'A target partition cannot be mapped to exactly one lockable volume GUID.'
    }
  }
  $volumes = @($partitions | ForEach-Object { $_.AccessPaths } |
    Where-Object { $_ -match $volumePattern } | Select-Object -Unique)
  foreach ($volume in $volumes) {
    $handle = [OmarchyVolumeLock]::CreateFileW($volume.TrimEnd('\'), 3221225472, 3,
      [IntPtr]::Zero, 3, 0, [IntPtr]::Zero)
    if ($handle.IsInvalid) { $handle.Dispose(); throw 'Cannot open a target volume exclusively for locking.' }
    $heldVolumes.Add($handle)
    [OmarchyVolumeLock]::Control($handle, 589848) # FSCTL_LOCK_VOLUME
    [OmarchyVolumeLock]::Control($handle, 589856) # FSCTL_DISMOUNT_VOLUME
  }
  [Console]::WriteLine('{"ready":true}')
  [Console]::Out.Flush()
  while ($null -ne [Console]::ReadLine()) { } # No command input is accepted.
} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
} finally {
  foreach ($handle in $heldVolumes) { $handle.Dispose() }
}
