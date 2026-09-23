[CmdletBinding()]
param([Parameter(Mandatory=$true)][ValidateSet('inspect','plan','stage','status','arm','firmware','cleanup')][string]$Action,
      [Parameter(Mandatory=$true)][string]$RequestPath,
      [switch]$TestingBuild)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'
$env:PSModulePath=Join-Path ([Environment]::GetFolderPath('Windows')) 'System32/WindowsPowerShell/v1.0/Modules'
$script:minimumBytes=[long]0
$script:testingBuild=[bool]$TestingBuild
function Fail([string]$Code,[string]$Message) { throw $Message }
function Emit([string]$Type,[string]$Stage,[hashtable]$Fields) {
    $event=@{protocolVersion=1;type=$Type;stage=$Stage;cancelAvailable=$false}
    foreach ($key in $Fields.Keys) { $event[$key]=$Fields[$key] }
    [Console]::Out.WriteLine(($event | ConvertTo-Json -Depth 24 -Compress))
}
function Assert-Fields($Value,[string[]]$Allowed,[string[]]$Required) {
    $names=@($Value.PSObject.Properties | ForEach-Object { $_.Name })
    foreach ($name in $names) { if ($name -notin $Allowed) { throw "Unknown field: $name" } }
    foreach ($name in $Required) { if ($name -notin $names -or $null -eq $Value.$name) { throw "Missing field: $name" } }
}
function Assert-Path([string]$Path,[bool]$Directory=$false) {
    if ($Path -notmatch '^[A-Za-z]:[\\/]' -or $Path.Substring(2).Contains(':')) { throw 'A local absolute path is required.' }
    $item=Get-Item -LiteralPath ([IO.Path]::GetFullPath($Path)) -Force
    if (($item -is [IO.DirectoryInfo]) -ne $Directory) { throw 'Unexpected storage type.' }
    for ($cursor=$item; $null -ne $cursor;) {
        if (($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Links are not allowed in staging inputs or records.' }
        $cursor=if ($cursor -is [IO.DirectoryInfo]) { $cursor.Parent } else { $cursor.Directory }
    }
    return $item.FullName
}
function Assert-ProtectedDirectory([string]$Path) {
    [void](Assert-Path $Path $true); $acl=Get-Acl -LiteralPath $Path
    if (-not $acl.AreAccessRulesProtected -or $acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin @('S-1-5-18','S-1-5-32-544')) { throw 'Staging storage must be administrator/SYSTEM owned and protected.' }
    foreach ($rule in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
        if ($rule.AccessControlType -eq 'Allow' -and (([long]$rule.FileSystemRights -band 0xD0156) -ne 0) -and $rule.IdentityReference.Value -notin @('S-1-5-18','S-1-5-32-544')) { throw 'Staging storage grants untrusted write access.' }
    }
}
function Read-Json([string]$Path) {
    [void](Assert-Path $Path)
    if ((Get-Item -LiteralPath $Path).Length -gt 16777216) { throw 'Staging record exceeds its size limit.' }
    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}
function Sha([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Write-Json([string]$Path,$Value) {
    if (Test-Path -LiteralPath $Path) { throw 'Staging journal entry already exists.' }
    Write-DurableJson $Path $Value
}
$mutex=$null; $acquired=$false
try {
    if (-not [Environment]::Is64BitProcess -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'Staged installation requires Windows x64.' }
    $principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'The authenticated helper must invoke staging elevated.' }
    Assert-ProtectedDirectory (Split-Path -Parent $PSScriptRoot)
    Assert-ProtectedDirectory (Split-Path -Parent $RequestPath)
    $direct=Join-Path $PSScriptRoot '../direct-x86'
    Emit 'progress' 'loading' @{message='Loading Windows disk tools...'}
    Add-Type -Path @((Join-Path $direct 'NativeDisk.cs'),(Join-Path $PSScriptRoot 'NativeStaged.cs'),(Join-Path $direct 'NativeSource.cs'))
    . (Join-Path $direct 'BitLocker.ps1')
    . (Join-Path $direct 'StoragePlan.ps1')
    . (Join-Path $PSScriptRoot 'Staging.ps1')
    $mutex=New-ProtectedMutex 'Global\OmarchyDirectX86Deployment'
    try { $acquired=$mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $acquired=$true }
    if (-not $acquired) { throw 'Another disk operation is active.' }
    $request=Read-Json $RequestPath
    switch ($Action) {
        'inspect' {
            Assert-Fields $request @('sourceSha256','sourceLength','resize') @('sourceSha256','sourceLength')
            $resize=if ($request.PSObject.Properties['resize']) { $request.resize } else { $null }
            $release=Get-StagingRelease $request.sourceSha256 $request.sourceLength
            $result=Get-StagingInspection $release $resize
        }
        'plan' {
            $plan=Get-StagingPlan $request
            $path=Join-Path (Split-Path -Parent $RequestPath) 'staged-plan.json'
            Write-Json $path $plan
            $result=@{plan=$plan;planPath=$path;planSha256=(Sha $path)}
        }
        'stage' {
            Assert-Fields $request @('planPath','planSha256','desktopProcessId','cancelPath') @('planPath','planSha256','desktopProcessId')
            $path=Assert-Path $request.planPath
            if ((Split-Path -Parent $path) -ine (Split-Path -Parent $RequestPath) -or [IO.Path]::GetFileName($path) -cne 'staged-plan.json' -or (Sha $path) -cne $request.planSha256) { throw 'The confirmed protected plan changed.' }
            # The helper creates this file to request cooperative cancellation.
            $cancel=if ($request.PSObject.Properties['cancelPath']) { [string]$request.cancelPath } else { '' }
            if ($cancel -and ((Split-Path -Parent $cancel) -ine (Split-Path -Parent $RequestPath) -or [IO.Path]::GetFileName($cancel) -cne 'staged-cancel')) { throw 'Invalid cancellation request.' }
            $worker=Join-Path $PSScriptRoot '../windows-bitlocker/BitLockerSetup.ps1'
            $result=& {
                param($Plan,[uint32]$AuthenticatedPid,[string]$WorkerPath,[string]$CancelPath)
                . $WorkerPath -LibraryOnly
                Invoke-StagingWrite $Plan $AuthenticatedPid $CancelPath
            } (Read-Json $path) $request.desktopProcessId $worker $cancel
        }
        'status' {
            $result=Get-StagingStatus $request
        }
        default {
            Assert-Fields $request @('operationId') @('operationId')
            $state=Read-StagingState $request.operationId
            $result=switch ($Action) {
                'arm' { Invoke-StagingArm $state }
                'firmware' { Invoke-StagingFirmware $state }
                default { Invoke-StagingCleanup $state }
            }
        }
    }
    Emit 'result' $Action @{result=$result}
} catch {
    # Only staging can leave partial partitions behind for reviewed removal.
    $hint=if ($Action -eq 'stage') { ' If preparation already started, use Remove temporary installer before trying again.' } else { '' }
    Emit 'error' $Action @{code='staged_iso_failed';message=($_.Exception.Message+$hint)}
    exit 1
} finally { if ($acquired) { $mutex.ReleaseMutex() }; if ($null -ne $mutex) { $mutex.Dispose() } }
