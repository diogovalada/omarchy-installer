# Read-only: runs the staged provider's disk check (Get-StagingInspection), the
# same one the app runs, and saves the stagedIso value for
# apps/desktop/test-fixtures/staged-layouts/. It reads disks, partitions,
# volumes, BitLocker and Secure Boot status and Windows' shrink limits; it
# changes nothing. It relaunches itself elevated, as the app's check does.
#
# Run from the repo, or copy it to another PC in one folder with
# Invoke-StagedIso.ps1, Staging.ps1, StoragePlan.ps1 and BitLocker.ps1:
#   powershell -ExecutionPolicy Bypass -File capture-staged-layout.ps1 -Out layout.json
# Disk IDs and partition GUIDs are replaced unless -KeepIdentifiers is given.
# Volume labels are kept and printed; replace personal ones before committing.
# A failure leaves -Out untouched and writes <Out>.error.txt instead.
param([Parameter(Mandatory=$true)][string]$Out,[switch]$KeepIdentifiers,[switch]$Relaunched)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'
# Resolve against PowerShell's location, which .NET's current directory may not follow.
$Out=$ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Out)
$errorPath="$Out.error.txt"
function Show-Labels([string]$Path) {
    $layout=[IO.File]::ReadAllText($Path) | ConvertFrom-Json
    $labels=@(@($layout.disks) | ForEach-Object { @($_.regions) | ForEach-Object { $_.label } })
    Write-Host "Saved $Path"
    Write-Host ('Labels: '+($labels -join ', '))
}
$principal=[Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    if (Test-Path -LiteralPath $errorPath) { Remove-Item -LiteralPath $errorPath }
    $arguments=@('-NoProfile','-ExecutionPolicy','Bypass','-File',"`"$PSCommandPath`"",'-Out',"`"$Out`"",'-Relaunched')
    if ($KeepIdentifiers) { $arguments+='-KeepIdentifiers' }
    $process=Start-Process powershell.exe -Verb RunAs -Wait -PassThru -ArgumentList $arguments
    if ($process.ExitCode -ne 0) {
        $detail=if (Test-Path -LiteralPath $errorPath) { [IO.File]::ReadAllText($errorPath) } else { 'No details were written.' }
        throw "The elevated disk check failed: $detail"
    }
    Show-Labels $Out
    return
}
try {
    foreach ($file in @('providers/staged-iso/Invoke-StagedIso.ps1','providers/staged-iso/Staging.ps1','providers/direct-x86/StoragePlan.ps1','providers/direct-x86/BitLocker.ps1')) {
        $path=@((Join-Path $PSScriptRoot "../$file"),(Join-Path $PSScriptRoot (Split-Path $file -Leaf))) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        if (-not $path) { throw "Missing $(Split-Path $file -Leaf) next to this script." }
        $tokens=$null; $errors=$null
        $ast=[Management.Automation.Language.Parser]::ParseFile($path,[ref]$tokens,[ref]$errors)
        if ($errors.Count) { throw ($errors | Out-String) }
        # Load only function definitions; the provider's entry point never runs.
        foreach ($definition in $ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$false)) { . ([scriptblock]::Create($definition.Extent.Text)) }
    }
    $script:minimumBytes=[long]0; $script:testingBuild=$true
    function Emit([string]$Type,[string]$Stage,[hashtable]$Fields) { if ($Fields -and $Fields.ContainsKey('message')) { Write-Host $Fields.message } }
    # Stand-in for the ISO: a 6 GiB image gives a 7.5 GiB temporary installer.
    $result=Get-StagingInspection ([pscustomobject]@{sizeBytes=6GB;minimumLinuxBytes=32GB})
    if (-not $KeepIdentifiers) {
        $ids=@{}; $guids=@{}
        $id={ param($v) if (-not $ids.ContainsKey($v)) { $ids[$v]='anonymized-disk-'+$ids.Count }; $ids[$v] }
        $guid={ param($v) $k=([string]$v).ToLowerInvariant(); if (-not $guids.ContainsKey($k)) { $guids[$k]='00000000-0000-0000-0000-'+('{0:D12}' -f ($guids.Count+1)) }; $guids[$k] }
        foreach ($disk in @($result.disks)) {
            $disk.diskUniqueId=& $id $disk.diskUniqueId
            foreach ($region in @($disk.regions)) { if ($region.ContainsKey('partitionGuid')) { $region.partitionGuid=& $guid $region.partitionGuid } }
        }
        foreach ($disk in @($result.blocked)) { $disk.diskUniqueId=& $id $disk.diskUniqueId }
        foreach ($choice in @($result.choices)) {
            $choice.diskUniqueId=& $id $choice.diskUniqueId
            if ($choice.target.ContainsKey('partition_guid')) { $choice.target.partition_guid=& $guid $choice.target.partition_guid }
        }
    }
    [IO.File]::WriteAllText($Out,($result | ConvertTo-Json -Depth 12),(New-Object Text.UTF8Encoding $false))
    if (-not $Relaunched) { Show-Labels $Out }
} catch {
    $message=$_.Exception.Message
    try { [IO.File]::WriteAllText($errorPath,$message,(New-Object Text.UTF8Encoding $false)) } catch { }
    Write-Host $message
    exit 1
}
