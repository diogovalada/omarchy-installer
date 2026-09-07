# Actual planner functions against synthetic storage only. No Windows disk,
# firmware, encryption, filesystem inspection, or mutation APIs are called.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
function Fail($Code,$Message) { throw ($Code+': '+$Message) }
function Emit($Kind,$Stage,$Fields) { }
function Assert($Condition,$Message) { if (-not $Condition) { throw $Message } }
function Reject($Action,$Message) { $rejected=$false; try { & $Action } catch { $rejected=$true }; Assert $rejected $Message }
Add-Type @'
namespace Omarchy.DirectX86 {
 public static class NativeDisk {
  public static string Firmware() { return "uefi"; }
  public static string VolumeForPath(string path) {
   if (string.IsNullOrWhiteSpace(path)) throw new System.Exception("Empty protected path");
   return @"\\?\Volume{11111111-1111-1111-1111-111111111111}\";
  }
 }
}
'@
$script:minimumBytes=[long]42947575808
$script:espBytes=[long]2147483648
$script:rootBytes=[long]40800092160
$script:providerRoot='C:\fixture\direct-x86'
$script:builderRoot='C:\fixture\image-builder-x86'
. (Join-Path $root 'providers/direct-x86/StoragePlan.ps1')
. (Join-Path $root 'providers/direct-x86/DeletionPlan.ps1')
$realDeletionContext=${function:Get-DeletionContext}
$script:partitions=@()
$script:partitionQueryFails=$false
$script:disk=[pscustomobject]@{Number=9;NumberOfPartitions=0;PartitionStyle='GPT';IsOffline=$false;IsReadOnly=$false;LogicalSectorSize=512;PhysicalSectorSize=4096;BusType='NVMe';UniqueId='fixture-disk';SerialNumber='fixture-serial';FriendlyName='Synthetic blank GPT disk';Size=[long]500GB}
function Confirm-SecureBootUEFI { return $false }
function Get-VerifiedWindowsBoot { return @{} }
function Get-BitLockerSnapshot { return @{known=$true;volumes=@();blockers=@()} }
function Get-AffectedBitLocker { return @() }
function Get-DeletionContext { return @{known=$true} }
function Get-Disk { return $script:disk }
function Get-Partition { throw 'Windows Get-Partition reports not-found for this blank disk.' }
function Get-CimInstance {
 param($ClassName,$Namespace,$Filter,$ErrorAction)
 switch ($ClassName) {
  'MSFT_Partition' {
   Assert ($Namespace -eq 'root/Microsoft/Windows/Storage') 'Unexpected partition namespace.'
   Assert ($Filter -eq 'DiskNumber = 9') 'Partition query was not bound to the selected disk.'
   if ($script:partitionQueryFails) { throw 'Injected partition query failure' }
   return $script:partitions
  }
  'Win32_OperatingSystem' { return [pscustomobject]@{FreePhysicalMemory=[long](16GB/1024)} }
  'Win32_Volume' { return @() }
  'Win32_PageFileUsage' { return @() }
  default { throw ('Unexpected CIM call: '+$ClassName) }
 }
}
function Get-RuntimeDistribution { return @{packaged=$true} }
function Get-Runtime { return @{imageId='sha256:fixture'} }
function Get-Docker { return 'Invoke-FixtureDocker' }
function Invoke-FixtureDocker { $global:LASTEXITCODE=0; if ($args[0] -eq 'info') { 'linux' } else { 'sha256:fixture' } }
function Get-ItemProperty { return [pscustomobject]@{DumpFile=''} }
function Get-Content { return '<WindowsRE><WinreLocation offset="0"/></WindowsRE>' }

$probe=Get-Probe
Assert ($probe.disks.Count -eq 1 -and $probe.disks[0].eligible) 'Blank GPT disk must remain eligible after a successful empty partition query.'
Assert ($probe.disks[0].partitions.Count -eq 0 -and $probe.disks[0].freeExtents.Count -eq 1) 'Blank disk must expose one bounded free extent.'
Assert ($probe.disks[0].freeExtents[0].offsetBytes -eq 1MB) 'Free extent must exclude the GPT header.'
Assert ($probe.disks[0].freeExtents[0].sizeBytes -eq 500GB-2MB) 'Free extent must exclude both GPT ends.'
$script:partitionQueryFails=$true
Assert (-not (Get-Probe).disks[0].eligible) 'Partition query errors must not become empty disks.'
$script:partitionQueryFails=$false
$script:disk.NumberOfPartitions=1
Assert (-not (Get-Probe).disks[0].eligible) 'Missing partitions must block installation.'
$script:disk.NumberOfPartitions=$null
Assert (-not (Get-Probe).disks[0].eligible) 'Unknown partition counts must block installation.'
$script:disk.NumberOfPartitions=0
foreach ($physical in @(0,511,768,131072)) {
 $script:disk.PhysicalSectorSize=$physical
 Assert (-not (Get-Probe).disks[0].eligible) 'Unknown or unsupported physical alignment must block installation.'
}
$script:disk.PhysicalSectorSize=4096

$savedProgramData=$env:ProgramData
try {
 $env:ProgramData=$null
 $context=& $realDeletionContext @('C:\fixture\installer.iso')
 Assert $context.known ('Protected storage discovery must work in the helper environment without ProgramData. Last error: '+$Error[0])
 Assert ($context.protectedVolumes.Count -gt 0) 'Protected system storage must still be identified.'
} finally { $env:ProgramData=$savedProgramData }

$planDisk=[pscustomobject]@{diskNumber=9;diskUniqueId='fixture-disk';eligible=$true;blockers=@();freeExtents=@([pscustomobject]@{offsetBytes=[long]1MB;sizeBytes=[long]100GB});shrinkCandidates=@()}
$planProbe=[pscustomobject]@{disks=@($planDisk)}
$request=[pscustomobject]@{targetKind='free';diskNumber=9;diskUniqueId='fixture-disk';startOffsetBytes=[long]1MB;allocationBytes=[long]64GB}
$allocation=Select-Allocation $planProbe $request
Assert ($allocation.startOffsetBytes -eq 1MB -and $null -eq $allocation.shrink) 'Free-space selection changed its requested extent.'
$request.allocationBytes=101GB
Reject { Select-Allocation $planProbe $request } 'An oversized allocation was accepted.'
$request.allocationBytes=64GB+1
Reject { Select-Allocation $planProbe $request } 'An unaligned allocation was accepted.'
$request.allocationBytes=64GB; $request.diskUniqueId='changed'
Reject { Select-Allocation $planProbe $request } 'A changed disk identity was accepted.'
Write-Output 'Passed blank-disk, failed-inventory, protected-environment and exact allocation regressions; all host boundaries mocked.'
