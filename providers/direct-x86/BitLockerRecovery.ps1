[CmdletBinding()]
param([switch]$RestoreFromHelper)
# Fixed SYSTEM recovery action. No user-selected file, command or executable.
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$env:PSModulePath=(Join-Path ([Environment]::GetFolderPath('Windows')) 'System32/WindowsPowerShell/v1.0/Modules')

function Assert-RecoveryPath([string]$Path,[bool]$Directory) {
    $item=Get-Item -LiteralPath $Path -Force
    if (($item -is [IO.DirectoryInfo]) -ne $Directory) { throw 'Unexpected recovery object type.' }
    $cursor=$item
    while ($null -ne $cursor) {
        if (($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Recovery path contains a reparse point.' }
        $cursor=if ($cursor -is [IO.DirectoryInfo]) { $cursor.Parent } else { $cursor.Directory }
    }
    $acl=Get-Acl -LiteralPath $Path
    if (-not $acl.AreAccessRulesProtected -or $acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin @('S-1-5-18','S-1-5-32-544')) { throw 'Recovery state ownership is not protected.' }
    foreach ($rule in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
        if ($rule.AccessControlType -eq 'Allow' -and (([long]$rule.FileSystemRights -band 0xD0156) -ne 0) -and $rule.IdentityReference.Value -notin @('S-1-5-18','S-1-5-32-544')) { throw 'Unprivileged mutation of recovery state is possible.' }
    }
}
function Write-RecoveryState([string]$Path,$Value) {
    $temporary=$Path+'.'+[guid]::NewGuid().ToString('N')+'.pending'
    $acl=New-Object Security.AccessControl.FileSecurity
    $acl.SetAccessRuleProtection($true,$false)
    $admin=New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')
    $acl.SetOwner($admin)
    foreach ($sid in @($admin,(New-Object Security.Principal.SecurityIdentifier('S-1-5-18')))) { $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($sid,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow))) }
    $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($Value | ConvertTo-Json -Depth 24)+"`n")
    try {
        $file=New-Object IO.FileStream($temporary,[IO.FileMode]::CreateNew,[Security.AccessControl.FileSystemRights]::Write,[IO.FileShare]::None,4096,[IO.FileOptions]::WriteThrough,$acl)
        try { $file.Write($bytes,0,$bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
        [IO.File]::Replace($temporary,$Path,$null)
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force } }
}
function Recovery-Query($Volume,[string]$Method) {
    $answer=Invoke-CimMethod -InputObject $Volume -MethodName $Method -ErrorAction Stop
    if ($answer.ReturnValue -ne 0) { throw "BitLocker $Method failed: $($answer.ReturnValue)" }
    return $answer
}
function Recovery-Mutex([string]$Name) {
    $acl=New-Object Security.AccessControl.MutexSecurity; $acl.SetAccessRuleProtection($true,$false)
    $admin=New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544'); $acl.SetOwner($admin)
    foreach ($sid in @($admin,(New-Object Security.Principal.SecurityIdentifier('S-1-5-18')))) { $acl.AddAccessRule((New-Object Security.AccessControl.MutexAccessRule($sid,[Security.AccessControl.MutexRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow))) }
    $created=$false; $mutex=[Threading.Mutex]::new($false,$Name,[ref]$created,$acl)
    try {
        $actual=$mutex.GetAccessControl()
        if (-not $actual.AreAccessRulesProtected -or $actual.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin @('S-1-5-18','S-1-5-32-544')) { throw 'Recovery mutex ownership is untrusted.' }
        foreach ($rule in $actual.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) { if ($rule.AccessControlType -eq 'Allow' -and $rule.IdentityReference.Value -notin @('S-1-5-18','S-1-5-32-544')) { throw 'Recovery mutex access is untrusted.' } }
        return $mutex
    } catch { $mutex.Dispose(); throw }
}
function Recovery-Protectors($Volume) {
    $answer=Invoke-CimMethod -InputObject $Volume -MethodName GetKeyProtectors -Arguments @{KeyProtectorType=[uint32]0} -ErrorAction Stop
    if ($answer.ReturnValue -ne 0) { throw 'Could not reidentify Windows key protectors.' }
    return @($answer.VolumeKeyProtectorID | ForEach-Object { ([guid]::Parse($_)).ToString() } | Sort-Object)
}
function Test-RecoveryWait($State, [bool]$OwnerAlive, [bool]$SameFirmwareBoot, [DateTime]$NowUtc) {
    if ($State.completed -or $NowUtc -ge [DateTime]::Parse($State.deadlineUtc).ToUniversalTime()) { return $false }
    if ($State.phase -eq 'firmware-restart') { return $SameFirmwareBoot }
    return $OwnerAlive
}

$principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Protected BitLocker recovery requires SYSTEM or an administrator.' }
$operation=[guid]::Parse((Split-Path -Leaf $PSScriptRoot)).ToString()
$expectedRoot=Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'OmarchyDirectRecovery'
$expected=Join-Path $expectedRoot $operation
if ([IO.Path]::GetFullPath($PSScriptRoot) -cne [IO.Path]::GetFullPath($expected)) { throw 'Recovery script is outside its fixed operation directory.' }
Assert-RecoveryPath $expectedRoot $true
Assert-RecoveryPath $expected $true
Assert-RecoveryPath $PSCommandPath $false
$statePath=Join-Path $expected 'bitlocker-state.json'
Assert-RecoveryPath $statePath $false
if ((Get-Item -LiteralPath $statePath).Length -gt 1048576) { throw 'Recovery state exceeds its fixed bound.' }
$mutex=Recovery-Mutex ('Global\OmarchyBitLockerRecovery-'+$operation)
$acquired=$false
try {
    try { $acquired=$mutex.WaitOne(30000) } catch [Threading.AbandonedMutexException] { $acquired=$true }
    if (-not $acquired) { throw 'Another recovery pass is still active.' }
    $state=Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
    if ($state.schemaVersion -ne 1 -or $state.operationId -cne $operation -or $state.scriptSha256 -cne (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Recovery identity or program digest changed.' }
    $taskName='OmarchyDirectBitLocker-'+$operation
    $alive=$false
    try {
        $owner=Get-Process -Id $state.processId -ErrorAction Stop
        $alive=$owner.StartTime.ToUniversalTime().Ticks -eq [long]$state.processStartUtcTicks
    } catch { }
    $sameFirmwareBoot=$false
    if ($state.phase -eq 'firmware-restart') {
        $sameFirmwareBoot=(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o') -ceq $state.firmwareBootTimeUtc
    }
    if ($RestoreFromHelper) {
        $self=Get-Process -Id $PID
        if ($PID -ne [int]$state.processId -or $self.StartTime.ToUniversalTime().Ticks -ne [long]$state.processStartUtcTicks) { throw 'Only the original live helper may request immediate restoration.' }
    } elseif (Test-RecoveryWait $state $alive $sameFirmwareBoot ([DateTime]::UtcNow)) {
        # The initiating process exits during shutdown. Do not reseal against
        # the old Secure Boot state merely because that process has exited.
        # On the next Windows startup, restore against that startup's state.
        return
    }
    if (-not $state.completed) {
        $state.phase='restoring'; Write-RecoveryState $statePath $state
        $instances=@(Get-CimInstance -Namespace 'root/CIMV2/Security/MicrosoftVolumeEncryption' -ClassName Win32_EncryptableVolume -ErrorAction Stop)
        $partitions=@(Get-Partition -ErrorAction Stop)
        foreach ($entry in $state.volumes) {
            if ($entry.originalProtectionStatus -ne 1 -or $entry.originalConversionStatus -ne 1) { throw 'Recovery record requests an unowned protection transition.' }
            if (-not $entry.resumeRequired) { $entry.restored=$true; continue }
            $matches=@($instances | Where-Object { $_.DeviceID -ieq $entry.volumeId -and $_.PersistentVolumeID -ceq $entry.persistentVolumeId })
            if ($matches.Count -ne 1) { throw 'The suspended volume identity is unavailable.' }
            $v=$matches[0]
            $partition=@($partitions | Where-Object { @($_.AccessPaths) -contains [string]$v.DeviceID })
            if ($partition.Count -ne 1 -or [string]$partition[0].Guid -cne $entry.partitionGuid -or [string](Get-Disk -Number $partition[0].DiskNumber).UniqueId -cne $entry.diskUniqueId) { throw 'The suspended volume no longer matches its original disk and partition.' }
            if ((Recovery-Query $v 'GetLockStatus').LockStatus -ne 0 -or (Recovery-Query $v 'GetConversionStatus').ConversionStatus -ne 1) { throw 'The suspended volume must remain fully encrypted and unlocked.' }
            $beforeKeys=@(Recovery-Protectors $v)
            if (($beforeKeys -join ',') -cne (@($entry.keyProtectorIds | Sort-Object) -join ',')) {
                # Do not leave app-suspended protection off because another
                # authorized Windows component rotated a protector. Restore
                # protection, record the difference, and let the live helper's
                # final snapshot check report the concurrent configuration change.
                $entry.protectorSetPreserved=$false
            }
            $protection=[int](Recovery-Query $v 'GetProtectionStatus').ProtectionStatus
            if ($protection -eq 0) { [void](Recovery-Query $v 'EnableKeyProtectors') }
            elseif ($protection -ne 1) { throw 'BitLocker protection status cannot be established.' }
            if ((Recovery-Query $v 'GetProtectionStatus').ProtectionStatus -ne 1 -or (Recovery-Query $v 'GetConversionStatus').ConversionStatus -ne 1) { throw 'BitLocker restoration did not verify with encryption intact.' }
            if (((Recovery-Protectors $v) -join ',') -cne ($beforeKeys -join ',')) { throw 'Windows key protector identities changed during restoration.' }
            $entry.restored=$true
            Write-RecoveryState $statePath $state
        }
        $state.completed=$true; $state.phase='restored'
        Write-RecoveryState $statePath $state
    }
    # State remains as evidence; remove only this operation's now-unneeded task.
    $scheduler=New-Object -ComObject 'Schedule.Service'; $scheduler.Connect()
    try { $scheduler.GetFolder('\').DeleteTask($taskName,0) }
    catch {
        $cause=$_.Exception
        while ($null -ne $cause.InnerException) { $cause=$cause.InnerException }
        if ($cause.HResult -ne -2147024894) { throw } # ERROR_FILE_NOT_FOUND: already removed after verified completion
    }
} catch {
    # Do not remove the task on failure: startup and one-minute retries remain,
    # with DisableCount=1 as the separate Windows reboot backstop. No decrypt,
    # protector deletion, or resume of originally suspended volumes exists here.
    throw
} finally {
    if ($acquired) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
