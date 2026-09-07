# Internal functions for the fixed direct-install protocol. Not a general CLI.
function Invoke-BitLockerQuery($Volume, [string]$Method, [hashtable]$Arguments=@{}) {
    $answer = Invoke-CimMethod -InputObject $Volume -MethodName $Method -Arguments $Arguments -ErrorAction Stop
    if ($answer.ReturnValue -ne 0) { Fail 'bitlocker_query_failed' ("BitLocker $Method failed with status " + $answer.ReturnValue) }
    return $answer
}
function Get-KeyProtectorDetails($Volume, $Ids) {
    $details=@()
    foreach ($id in @($Ids | Sort-Object)) {
        $keyId=([guid]$id).ToString('B')
        $type=[int](Invoke-BitLockerQuery $Volume 'GetKeyProtectorType' @{VolumeKeyProtectorID=$keyId}).KeyProtectorType
        $profile=@()
        if ($type -in @(1,4,5,6)) {
            $profile=@((Invoke-BitLockerQuery $Volume 'GetKeyProtectorPlatformValidationProfile' @{VolumeKeyProtectorID=$keyId}).PlatformValidationProfile | ForEach-Object { [int]$_ } | Sort-Object)
            if ($profile.Count -eq 0 -or @($profile | Select-Object -Unique).Count -ne $profile.Count -or @($profile | Where-Object { $_ -lt 0 -or $_ -gt 23 }).Count) {
                Fail 'bitlocker_profile_invalid' 'Windows returned an invalid TPM validation profile.'
            }
        }
        $details += [ordered]@{id=([guid]$id).ToString();type=$type;pcrProfile=$profile}
    }
    return $details
}
function Get-OsProtectorBlockers($Details) {
    # Native UEFI with Secure Boot already off. Never alter a custom PCR policy
    # to make it fit our installer. Recovery/startup passwords and external keys
    # are retained; unknown/certificate protectors need separate qualification.
    foreach ($key in @($Details)) {
        if ($key.type -in @(1,4,5,6)) {
            if ((@($key.pcrProfile) -join ',') -cne '0,2,4,11') {
                'Windows TPM protection uses a profile not supported by this installation. Finish Secure Boot preparation or ask your administrator; this app does not change TPM policy.'
            }
        } elseif ($key.type -notin @(2,3,8)) {
            'Windows uses a key protector type that this installer has not qualified.'
        }
    }
}
function Get-BitLockerSnapshot {
    try {
        $instances = @(Get-CimInstance -Namespace 'root/CIMV2/Security/MicrosoftVolumeEncryption' -ClassName Win32_EncryptableVolume -ErrorAction Stop)
        $partitions = @(Get-Partition -ErrorAction Stop)
        $systemDrive = [string](Get-CimInstance Win32_OperatingSystem -ErrorAction Stop).SystemDrive
    } catch { return [ordered]@{known=$false;volumes=@();blockers=@('Windows encryption status requires a successful elevated BitLocker inspection.')} }
    $volumes = @()
    foreach ($v in $instances) {
        $problems = New-Object 'Collections.Generic.List[string]'
        $conversion = -1; $protection = 2; $locked = -1; $keys=@(); $details=@()
        try {
            $conversion = [int](Invoke-BitLockerQuery $v 'GetConversionStatus').ConversionStatus
            $protection = [int](Invoke-BitLockerQuery $v 'GetProtectionStatus').ProtectionStatus
            $locked = [int](Invoke-BitLockerQuery $v 'GetLockStatus').LockStatus
            if ($conversion -notin @(0,1)) { $problems.Add('Encryption or decryption is incomplete or paused.') }
            if ($protection -notin @(0,1)) { $problems.Add('BitLocker protection status is unknown.') }
            if ($locked -ne 0) { $problems.Add('The Windows volume is locked or its accessibility is unknown.') }
            if ($conversion -eq 1 -and $locked -eq 0) {
                $keys = @((Invoke-BitLockerQuery $v 'GetKeyProtectors' @{KeyProtectorType=[uint32]0}).VolumeKeyProtectorID | ForEach-Object { ([guid]::Parse($_)).ToString() } | Sort-Object)
                if ($keys.Count -eq 0) { $problems.Add('An encrypted volume must have existing persistent key protectors.') }
                $details=@(Get-KeyProtectorDetails $v $keys)
            }
        } catch { $problems.Add($_.Exception.Message) }
        $matches = @($partitions | Where-Object { @($_.AccessPaths) -contains [string]$v.DeviceID })
        $diskNumber=-1; $partitionNumber=-1; $partitionGuid=''; $diskUniqueId=''
        if ($matches.Count -eq 1) {
            $diskNumber=[int]$matches[0].DiskNumber; $partitionNumber=[int]$matches[0].PartitionNumber; $partitionGuid=[string]$matches[0].Guid
            $diskUniqueId=[string](Get-Disk -Number $diskNumber).UniqueId
        } else { $problems.Add('Windows volume cannot be uniquely associated with a GPT partition.') }
        $isOs = [string]$v.DriveLetter -ieq $systemDrive
        $isBoot = $matches.Count -eq 1 -and ($matches[0].IsBoot -or $matches[0].IsSystem)
        if (@($v.PSObject.Properties | ForEach-Object { $_.Name }) -contains 'VolumeType') { $isOs = $isOs -or [int]$v.VolumeType -eq 0 }
        if ($isOs -and $conversion -eq 1) { foreach ($issue in @(Get-OsProtectorBlockers $details)) { $problems.Add($issue) } }
        if ($isBoot -and -not $isOs -and $conversion -eq 1 -and $protection -eq 1) { $problems.Add('An encrypted boot volume was not identified as an OS volume; bounded suspension is unsupported for this layout.') }
        $volumes += [ordered]@{volumeId=[string]$v.DeviceID;persistentVolumeId=[string]$v.PersistentVolumeID;driveLetter=[string]$v.DriveLetter;diskNumber=$diskNumber;diskUniqueId=$diskUniqueId;partitionNumber=$partitionNumber;partitionGuid=$partitionGuid;isOsVolume=[bool]$isOs;isBootVolume=[bool]$isBoot;conversionStatus=$conversion;protectionStatus=$protection;lockStatus=$locked;keyProtectorIds=$keys;keyProtectors=$details;supported=($problems.Count -eq 0);blockers=@($problems.ToArray())}
    }
    $known = @($volumes | Where-Object { $_.isOsVolume }).Count -ge 1
    return [ordered]@{known=$known;volumes=$volumes;blockers=$(if ($known) { @() } else { @('The current Windows OS volume could not be identified in BitLocker inventory.') })}
}
function Get-AffectedBitLocker($Snapshot, [int]$TargetDisk) {
    if (-not $Snapshot.known) { Fail 'bitlocker_unknown' ($Snapshot.blockers -join ' ') }
    $affected = @($Snapshot.volumes | Where-Object { $_.diskNumber -eq $TargetDisk -or $_.isOsVolume -or $_.isBootVolume } | Sort-Object volumeId)
    foreach ($v in $affected) { if (-not $v.supported) { Fail 'bitlocker_unsupported' (($v.driveLetter+' '+($v.blockers -join ' ')).Trim()) } }
    return $affected
}
function Assert-BitLockerUnchanged($Before, $Now) {
    $beforeList=@($Before); $nowList=@($Now)
    if ($beforeList.Count -ne $nowList.Count) { Fail 'bitlocker_changed' 'The affected Windows volume set changed after confirmation.' }
    foreach ($v in $beforeList) {
        $match=@($nowList | Where-Object { $_.volumeId -ieq $v.volumeId })
        if ($match.Count -ne 1) { Fail 'bitlocker_changed' 'A Windows volume identity changed.' }
        $n=$match[0]
        foreach ($field in @('persistentVolumeId','diskUniqueId','partitionGuid','isOsVolume','isBootVolume','conversionStatus','protectionStatus','lockStatus')) {
            if ([string]$n.$field -cne [string]$v.$field) { Fail 'bitlocker_changed' "Windows encryption state changed: $field" }
        }
        if ((@($n.keyProtectorIds | Sort-Object) -join ',') -cne (@($v.keyProtectorIds | Sort-Object) -join ',')) { Fail 'bitlocker_changed' 'Windows key protector identities changed.' }
        if ((ConvertTo-Json -InputObject @($n.keyProtectors) -Depth 6 -Compress) -cne (ConvertTo-Json -InputObject @($v.keyProtectors) -Depth 6 -Compress)) { Fail 'bitlocker_changed' 'Windows key protector types or TPM validation profiles changed.' }
    }
}
function New-ProtectedMutex([string]$Name) {
    $acl=New-Object Security.AccessControl.MutexSecurity
    $acl.SetAccessRuleProtection($true,$false)
    $admin=New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')
    $acl.SetOwner($admin)
    foreach ($sid in @($admin,(New-Object Security.Principal.SecurityIdentifier('S-1-5-18')))) { $acl.AddAccessRule((New-Object Security.AccessControl.MutexAccessRule($sid,[Security.AccessControl.MutexRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow))) }
    $created=$false
    $mutex=[Threading.Mutex]::new($false,$Name,[ref]$created,$acl)
    try {
        $actual=$mutex.GetAccessControl()
        if (-not $actual.AreAccessRulesProtected -or $actual.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin @('S-1-5-18','S-1-5-32-544')) { throw 'Named operation mutex has untrusted ownership.' }
        foreach ($rule in $actual.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
            if ($rule.AccessControlType -eq 'Allow' -and $rule.IdentityReference.Value -notin @('S-1-5-18','S-1-5-32-544')) { throw 'Named operation mutex has untrusted access.' }
        }
        return $mutex
    } catch { $mutex.Dispose(); throw }
}
function Write-DurableJson([string]$Path, $Value) {
    $temporary = $Path+'.'+[guid]::NewGuid().ToString('N')+'.pending'
    $bytes = (New-Object Text.UTF8Encoding($false)).GetBytes(($Value | ConvertTo-Json -Depth 24)+"`n")
    try {
        $file = New-Object IO.FileStream($temporary,[IO.FileMode]::CreateNew,[Security.AccessControl.FileSystemRights]::Write,[IO.FileShare]::None,4096,[IO.FileOptions]::WriteThrough,(New-RecoveryFileSecurity))
        try { $file.Write($bytes,0,$bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
        if (Test-Path -LiteralPath $Path) { [IO.File]::Replace($temporary,$Path,$null) } else { [IO.File]::Move($temporary,$Path) }
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force } }
}
function New-RecoveryFileSecurity {
    $acl=New-Object Security.AccessControl.FileSecurity
    $acl.SetAccessRuleProtection($true,$false)
    $admin=New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')
    $system=New-Object Security.Principal.SecurityIdentifier('S-1-5-18')
    $acl.SetOwner($admin)
    foreach ($sid in @($admin,$system)) { $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($sid,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow))) }
    return $acl
}
function New-ProtectedRecoveryDirectory([string]$Path) {
    if (Test-Path -LiteralPath $Path) { Assert-ProtectedDirectory $Path; return }
    $acl=New-Object Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true,$false)
    $admin=New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')
    $system=New-Object Security.Principal.SecurityIdentifier('S-1-5-18')
    $acl.SetOwner($admin)
    foreach ($sid in @($admin,$system)) {
        $rule=New-Object Security.AccessControl.FileSystemAccessRule($sid,[Security.AccessControl.FileSystemRights]::FullControl,([Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [Security.AccessControl.InheritanceFlags]::ObjectInherit),[Security.AccessControl.PropagationFlags]::None,[Security.AccessControl.AccessControlType]::Allow)
        $acl.AddAccessRule($rule)
    }
    # Windows PowerShell/.NET Framework creates the directory with its ACL in
    # the same call, avoiding an unprotected directory creation window.
    [void][IO.Directory]::CreateDirectory($Path,$acl)
    Assert-ProtectedDirectory $Path
}
function New-BitLockerRecovery($Plan) {
    $toSuspend=@($Plan.bitLocker.volumes | Where-Object { $_.isOsVolume -and $_.conversionStatus -eq 1 -and $_.protectionStatus -eq 1 })
    if ($toSuspend.Count -eq 0) { return $null }
    $recoveryRoot=Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'OmarchyDirectRecovery'
    New-ProtectedRecoveryDirectory $recoveryRoot
    $recoveryDirectory=Join-Path $recoveryRoot $script:operationId
    if (Test-Path -LiteralPath $recoveryDirectory) { Fail 'recovery_exists' 'A recovery record already exists for this operation.' }
    New-ProtectedRecoveryDirectory $recoveryDirectory
    $source=Assert-Path (Join-Path $script:providerRoot 'BitLockerRecovery.ps1')
    $scriptPath=Join-Path $recoveryDirectory 'BitLockerRecovery.ps1'
    $scriptBytes=[IO.File]::ReadAllBytes($source)
    $scriptFile=New-Object IO.FileStream($scriptPath,[IO.FileMode]::CreateNew,[Security.AccessControl.FileSystemRights]::Write,[IO.FileShare]::None,4096,[IO.FileOptions]::WriteThrough,(New-RecoveryFileSecurity))
    try { $scriptFile.Write($scriptBytes,0,$scriptBytes.Length); $scriptFile.Flush($true) } finally { $scriptFile.Dispose() }
    if ((Sha $source) -ne (Sha $scriptPath)) { Fail 'recovery_copy_failed' 'BitLocker recovery program could not be staged intact.' }
    $process=Get-Process -Id $PID
    $items=@()
    foreach ($v in $toSuspend) {
        $items += [ordered]@{volumeId=$v.volumeId;persistentVolumeId=$v.persistentVolumeId;diskUniqueId=$v.diskUniqueId;partitionGuid=$v.partitionGuid;originalProtectionStatus=1;originalConversionStatus=1;keyProtectorIds=$v.keyProtectorIds;protectorSetPreserved=$true;resumeRequired=$false;restored=$false}
    }
    $record=[ordered]@{schemaVersion=1;operationId=$script:operationId;model='gpt-6-astra';phase='armed';completed=$false;processId=$PID;processStartUtcTicks=$process.StartTime.ToUniversalTime().Ticks;deadlineUtc=[DateTime]::UtcNow.AddHours(2).ToString('o');volumes=$items;scriptSha256=(Sha $scriptPath)}
    $statePath=Join-Path $recoveryDirectory 'bitlocker-state.json'; Write-DurableJson $statePath $record
    $taskName='OmarchyDirectBitLocker-'+$script:operationId
    $exe=Join-Path ([Environment]::GetFolderPath('Windows')) 'System32/WindowsPowerShell/v1.0/powershell.exe'
    $arguments='-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "'+$scriptPath+'"'
    $xmlExe=[Security.SecurityElement]::Escape($exe); $xmlArgs=[Security.SecurityElement]::Escape($arguments)
    $start=[DateTime]::Now.AddMinutes(1).ToString('s')
    $xml=@"
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Restore only BitLocker protectors suspended by this Omarchy installation.</Description></RegistrationInfo>
  <Triggers><BootTrigger><Enabled>true</Enabled></BootTrigger><TimeTrigger><Repetition><Interval>PT1M</Interval><StopAtDurationEnd>false</StopAtDurationEnd></Repetition><StartBoundary>$start</StartBoundary><Enabled>true</Enabled></TimeTrigger></Triggers>
  <Principals><Principal id="System"><UserId>S-1-5-18</UserId><LogonType>ServiceAccount</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><AllowHardTerminate>false</AllowHardTerminate><StartWhenAvailable>true</StartWhenAvailable><Enabled>true</Enabled><Hidden>true</Hidden><ExecutionTimeLimit>PT5M</ExecutionTimeLimit></Settings>
  <Actions Context="System"><Exec><Command>$xmlExe</Command><Arguments>$xmlArgs</Arguments></Exec></Actions>
</Task>
"@
    $scheduler=New-Object -ComObject 'Schedule.Service'; $scheduler.Connect()
    $folder=$scheduler.GetFolder('\')
    # TASK_CREATE only; never overwrite an unrelated task. Protected task DACL
    # prevents the ordinary desktop user from replacing a SYSTEM action.
    $task=$folder.RegisterTask($taskName,$xml,2,'SYSTEM',$null,5,'D:P(A;;FA;;;SY)(A;;FA;;;BA)')
    $actual=$folder.GetTask($taskName)
    if ($actual.Definition.Actions.Count -ne 1 -or $actual.Definition.Actions.Item(1).Path -ine $exe -or $actual.Definition.Actions.Item(1).Arguments -cne $arguments -or $actual.Definition.Principal.UserId -notin @('SYSTEM','S-1-5-18') -or -not $actual.Enabled) { Fail 'recovery_task_failed' 'BitLocker recovery task did not read back with the fixed SYSTEM action.' }
    $security=New-Object Security.AccessControl.RawSecurityDescriptor($actual.GetSecurityDescriptor(4))
    foreach ($ace in $security.DiscretionaryAcl) {
        if ($ace.AceType -eq [Security.AccessControl.AceType]::AccessAllowed -and $ace.SecurityIdentifier.Value -notin @('S-1-5-18','S-1-5-32-544')) { Fail 'recovery_task_failed' 'BitLocker recovery task grants an unexpected identity access.' }
    }
    $mutex=New-ProtectedMutex ('Global\OmarchyBitLockerRecovery-'+$script:operationId)
    return [ordered]@{directory=$recoveryDirectory;statePath=$statePath;taskName=$taskName;record=$record;mutex=$mutex}
}
function Suspend-PlannedBitLocker($Context) {
    if ($null -eq $Context) { return }
    $acquired=$false
    try {
      try { $acquired=$Context.mutex.WaitOne(30000) } catch [Threading.AbandonedMutexException] { $acquired=$true }
      if (-not $acquired) { Fail 'recovery_busy' 'BitLocker recovery is still active.' }
      $Context.record=Read-Json $Context.statePath
      if ($Context.record.completed -or [DateTime]::UtcNow -ge [DateTime]::Parse($Context.record.deadlineUtc).ToUniversalTime()) { Fail 'suspension_expired' 'The bounded BitLocker suspension window has expired.' }
      foreach ($entry in $Context.record.volumes) {
        $matches=@(Get-CimInstance -Namespace 'root/CIMV2/Security/MicrosoftVolumeEncryption' -ClassName Win32_EncryptableVolume | Where-Object { $_.DeviceID -ieq $entry.volumeId -and $_.PersistentVolumeID -ceq $entry.persistentVolumeId })
        if ($matches.Count -ne 1) { Fail 'bitlocker_changed' 'OS volume identity changed before protection suspension.' }
        $v=$matches[0]
        if ((Invoke-BitLockerQuery $v 'GetProtectionStatus').ProtectionStatus -ne 1 -or (Invoke-BitLockerQuery $v 'GetConversionStatus').ConversionStatus -ne 1 -or (Invoke-BitLockerQuery $v 'GetLockStatus').LockStatus -ne 0) { Fail 'bitlocker_changed' 'OS volume is no longer fully encrypted, unlocked and protected.' }
        $keys=@((Invoke-BitLockerQuery $v 'GetKeyProtectors' @{KeyProtectorType=[uint32]0}).VolumeKeyProtectorID | ForEach-Object { ([guid]::Parse($_)).ToString() } | Sort-Object)
        if (($keys -join ',') -cne (@($entry.keyProtectorIds | Sort-Object) -join ',')) { Fail 'bitlocker_changed' 'OS key protector identities changed before suspension.' }
        if ([DateTime]::UtcNow -ge [DateTime]::Parse($Context.record.deadlineUtc).ToUniversalTime()) { Fail 'suspension_expired' 'The bounded BitLocker suspension window expired before the next volume.' }
        # Durable intent precedes the call, covering a crash after suspension but
        # before a post-call journal update. Preexisting suspension is excluded.
        $entry.resumeRequired=$true; $Context.record.phase='suspending'
        Write-DurableJson $Context.statePath $Context.record
        [void](Invoke-BitLockerQuery $v 'DisableKeyProtectors' @{DisableCount=[uint32]1})
        if ((Invoke-BitLockerQuery $v 'GetProtectionStatus').ProtectionStatus -ne 0 -or (Invoke-BitLockerQuery $v 'GetConversionStatus').ConversionStatus -ne 1) { Fail 'bitlocker_suspend_failed' 'BitLocker did not report suspended protection with encryption intact.' }
      }
      $Context.record.phase='suspended'; Write-DurableJson $Context.statePath $Context.record
    } finally { if ($acquired) { $Context.mutex.ReleaseMutex() } }
}
function Restore-PlannedBitLocker($Context) {
    if ($null -eq $Context) { return [ordered]@{required=$false;verified=$true;volumes=@()} }
    # The shipped worker has a narrowly scoped internal force-restore mode for
    # this live helper. The scheduled invocation passes no arguments.
    $worker=Join-Path $Context.directory 'BitLockerRecovery.ps1'
    & $worker -RestoreFromHelper
    $record=Read-Json $Context.statePath
    if (-not $record.completed) { Fail 'bitlocker_restore_failed' ('BitLocker restoration needs attention. SYSTEM recovery remains active: '+$Context.taskName) }
    return [ordered]@{required=$true;verified=$true;recoveryTask=$Context.taskName;statePath=$Context.statePath;volumes=$record.volumes}
}
