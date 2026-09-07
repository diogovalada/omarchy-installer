# Pure policy/parser tests. No host disk, TPM, BitLocker or firmware API calls.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
function Fail($Code,$Message) { throw ($Code+': '+$Message) }
function Invoke-CimMethod { throw 'Host CIM calls are forbidden in this test.' }
function Get-CimInstance { throw 'Host CIM calls are forbidden in this test.' }
. (Join-Path $root 'providers/direct-x86/BitLocker.ps1')
function Assert($Condition,$Message) { if (-not $Condition) { throw $Message } }
$keys=@([pscustomobject]@{type=1;pcrProfile=@(0,2,4,11)},[pscustomobject]@{type=3;pcrProfile=@()})
Assert (@(Get-OsProtectorBlockers $keys).Count -eq 0) 'Standard disabled-Secure-Boot profile must be accepted.'
foreach ($profile in @(@(7,11),@(0,1,2,4,11),@(0,2,4,5,11),@())) {
    Assert (@(Get-OsProtectorBlockers @([pscustomobject]@{type=1;pcrProfile=$profile})).Count -gt 0) 'Unsupported TPM policy was accepted.'
}
foreach ($type in @(0,7,9,10,99)) {
    Assert (@(Get-OsProtectorBlockers @([pscustomobject]@{type=$type;pcrProfile=@()})).Count -gt 0) 'Unknown protector was accepted.'
}
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $root 'providers/direct-x86/BitLockerRecovery.ps1'),[ref]$tokens,[ref]$errors)
Assert ($errors.Count -eq 0) 'Recovery worker must parse.'
$function=$ast.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Test-RecoveryWait'},$true)
# Load only the pure policy function; never execute the SYSTEM worker body.
. ([scriptblock]::Create($function.Extent.Text))
$now=[DateTime]::Parse('2026-09-06T12:00:00Z').ToUniversalTime()
$state=[pscustomobject]@{completed=$false;phase='suspended';deadlineUtc=$now.AddMinutes(5).ToString('o')}
Assert (Test-RecoveryWait $state $true $false $now) 'An active deployment must retain its maintenance window.'
Assert (-not (Test-RecoveryWait $state $false $false $now)) 'A crashed deployment must restore protection.'
$state.phase='firmware-restart'
Assert (Test-RecoveryWait $state $false $true $now) 'Shutdown process exit must not reseal against the old firmware state.'
Assert (-not (Test-RecoveryWait $state $false $false $now)) 'The next Windows boot must restore protection.'
Assert (-not (Test-RecoveryWait $state $true $true $now.AddMinutes(6))) 'A cancelled firmware restart must not leave protection suspended.'
$state.completed=$true
Assert (-not (Test-RecoveryWait $state $true $true $now)) 'Completed recovery must retire its task.'
Add-Type -Path @((Join-Path $root 'providers/direct-x86/NativeDisk.cs'),(Join-Path $root 'providers/direct-x86/NativeBootEvidence.cs'),(Join-Path $PSScriptRoot 'BootEvidenceTests.cs'))
$checks=[BootEvidenceTests]::Run()
Write-Output "Passed protector policy cases and $checks measured-boot parser checks; no host APIs invoked."
