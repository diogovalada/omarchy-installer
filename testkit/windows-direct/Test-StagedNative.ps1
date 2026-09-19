$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
Add-Type -Path @((Join-Path $root 'providers/direct-x86/NativeDisk.cs'),(Join-Path $root 'providers/staged-iso/NativeStaged.cs'))
function Assert($Value,[string]$Message) { if (-not $Value) { throw $Message } }
function Reject($Work,[string]$Message) { $failed=$false; try { & $Work | Out-Null } catch { $failed=$true }; Assert $failed $Message }
function Put([byte[]]$Buffer,[int]$Offset,[byte[]]$Value) { [Array]::Copy($Value,0,$Buffer,$Offset,$Value.Length) }
$before=New-Object byte[] (48+144)
Put $before 0 ([BitConverter]::GetBytes([int]1)); Put $before 4 ([BitConverter]::GetBytes([int]1))
Put $before 24 ([BitConverter]::GetBytes([long]1MB)); Put $before 32 ([BitConverter]::GetBytes([long](500GB-2MB))); Put $before 40 ([BitConverter]::GetBytes([int]128))
Put $before 48 ([BitConverter]::GetBytes([int]1)); Put $before 56 ([BitConverter]::GetBytes([long]1MB)); Put $before 64 ([BitConverter]::GetBytes([long]100GB)); Put $before 72 ([BitConverter]::GetBytes([int]3))
Put $before 80 ([guid]'ebd0a0a2-b9e5-4433-87c0-68b6b72699c7').ToByteArray()
$existing=[guid]::NewGuid(); Put $before 96 $existing.ToByteArray()
Put $before 120 ([Text.Encoding]::Unicode.GetBytes('Windows - keep'))
$esp=[guid]::NewGuid(); $data=[guid]::NewGuid()
$after=[Omarchy.DirectX86.NativeDisk]::PlanStagingLayout($before,200GB,8GB,$esp,$data)
Assert ([BitConverter]::ToInt32($after,4) -eq 3) 'Must create only two staging partitions.'
Assert ([Convert]::ToBase64String($before,48,144) -ceq [Convert]::ToBase64String($after,48,144)) 'Existing Windows partition metadata must survive byte-for-byte.'
Assert ([BitConverter]::ToInt64($after,48+144+8) -eq 200GB) 'EFI start must match approved extent.'
Assert ([BitConverter]::ToInt64($after,48+288+8) -eq 200GB+512MB) 'Source follows EFI without overlap.'
Reject { [Omarchy.DirectX86.NativeDisk]::PlanStagingLayout($before,50GB,8GB,$esp,$data) } 'Existing Windows overlap accepted.'
Reject { [Omarchy.DirectX86.NativeDisk]::PlanStagingLayout($before,499GB,8GB,$esp,$data) } 'GPT end overrun accepted.'
Reject { [Omarchy.DirectX86.NativeDisk]::PlanStagingLayout($before,200GB,8GB,$existing,$data) } 'Reused GUID accepted.'
Reject { [Omarchy.DirectX86.NativeDisk]::PlanStagingLayout($before,200GB,8GB,$esp,$esp) } 'Duplicate owned GUID accepted.'
Reject { [Omarchy.DirectX86.NativeDisk]::PlanStagingLayout($before,200GB+512,8GB,$esp,$data) } 'Misalignment accepted.'
$option=[Omarchy.DirectX86.NativeDisk]::StagingBootOption(4,200GB,$esp)
$number=[BitConverter]::ToInt32($after,48+144+24)
$deleted=[Omarchy.DirectX86.NativeDisk]::PlanStagingDeletion($after,$number,$esp,200GB,512MB,[guid]'c12a7328-f81f-11d2-ba4b-00a0c93ec93b')
Assert ([Convert]::ToBase64String($before,48,144) -ceq [Convert]::ToBase64String($deleted,48,144)) 'Cleanup changed retained Windows metadata.'
Reject { [Omarchy.DirectX86.NativeDisk]::PlanStagingDeletion($after,$number,$esp,201GB,512MB,[guid]'c12a7328-f81f-11d2-ba4b-00a0c93ec93b') } 'Cleanup accepted a moved partition.'
Assert ([Text.Encoding]::Unicode.GetString($option).Contains('\EFI\BOOT\BOOTX64.EFI')) 'Firmware entry must target the temporary EFI loader.'
Assert ([BitConverter]::ToInt32($option,0) -eq 1) 'Temporary firmware option must be active.'
Write-Output 'Passed staged GPT and EFI binary tests. No host disks or firmware were accessed.'
