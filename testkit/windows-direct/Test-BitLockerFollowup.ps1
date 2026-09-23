# Pure policy and mocked preparation tests. Never invoke host encryption/tasks.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $root 'providers/windows-bitlocker/BitLockerSetup.ps1'),[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($definition in $ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst]},$false)) { . ([scriptblock]::Create($definition.Extent.Text)) }
function Assert($Value,[string]$Message) { if (-not $Value) { throw $Message } }
function Reject($Work,[string]$Message) { $rejected=$false; try { & $Work } catch { $rejected=$true }; Assert $rejected $Message }
function Facts([int]$Conversion=1,[int]$Protection=1) {
    return [pscustomobject]@{volumeId='volume';persistentVolumeId='persistent';partitionGuid='22222222-2222-2222-2222-222222222222';diskUniqueId='disk';driveLetter='C:';isOsVolume=$true;conversion=$Conversion;protection=$Protection;locked=0;percentage=100;bootUtc='new-boot'}
}
$state=[pscustomobject]@{schema=1;origin='installer-decryption';status='pending';volume=(Facts);createdBootUtc='old-boot'}
Get-EncryptionEligibility (Facts) 'suspend'
Reject { Get-EncryptionEligibility (Facts 0 0) 'suspend' } 'Already-off encryption must not create an app-owned decryption reminder.'
Reject { Get-EncryptionEligibility (Facts 1 0) 'suspend' } 'An existing suspension must be preserved.'
Get-EncryptionEligibility (Facts 0 0) 'remind'
Get-EncryptionEligibility (Facts 3 0) 'remind'
Reject { Get-EncryptionEligibility (Facts) 'remind' } 'No manual off reminder for active encryption.'
Assert ((Get-FollowupDecision $state (Facts 0 0)) -eq 'ask') 'Owned decryption needs a prompt.'
$state.origin='manual-opt-in'
Assert ((Get-FollowupDecision $state (Facts 0 0)) -eq 'ask') 'Explicit opt-in needs a prompt.'
$state.origin='observed-off'
Reject { Get-FollowupDecision $state (Facts 0 0) } 'Observation alone must not own a reminder.'
$state.origin='installer-decryption'
Assert ((Get-FollowupDecision $state (Facts 1 1)) -eq 'restored') 'User-restored encryption must retire the reminder.'
Assert ((Get-FollowupDecision $state (Facts 1 0)) -eq 'suspended') 'Suspension is not verified restoration.'
Assert ((Get-FollowupDecision $state (Facts 2 1)) -eq 'encrypting') 'Starting encryption is not completion.'
Assert ((Get-FollowupDecision $state (Facts 4 0)) -eq 'encrypting') 'Paused encryption is not completion.'
Assert ((Get-FollowupDecision $state (Facts 3 0)) -eq 'decrypting') 'Do not ask to re-enable while decrypting.'
$same=Facts 0 0; $same.bootUtc='old-boot'
Assert ((Get-FollowupDecision $state $same) -eq 'ask') 'Fast Startup or a new sign-in must not suppress reminders because boot time is unchanged.'
$state.status='armed'
Assert ((Get-FollowupDecision $state (Facts 0 0)) -eq 'uncertain') 'A missing action receipt must not assert app ownership.'
$state.status='declined'
Assert ((Get-FollowupDecision $state (Facts 0 0)) -eq 'finished') 'Explicit no must stop reminders.'
$changed=Facts 0 0; $changed.diskUniqueId='replacement'
Reject { Get-FollowupDecision $state $changed } 'Drive letters are not sufficient volume identity.'
$state.status='pending'

# Exercise the real registration/reconciliation code using disposable files and
# a fake scheduler. No host task, encryption API, or administrator ACL is used.
& {
    $testRoot=Join-Path $env:TEMP ('omarchy-reminder-test-'+[guid]::NewGuid().ToString('N'))
    $script:EncryptionWorkerPath=Join-Path $root 'providers/windows-bitlocker/BitLockerSetup.ps1'
    $fixture=@{failure='';task=$null;events=(New-Object 'Collections.Generic.List[string]')}
    $scheduler=[pscustomobject]@{}
    $folder=[pscustomobject]@{}
    $scheduler | Add-Member ScriptMethod Connect {
        $fixture.events.Add('connect')
        if ($fixture.failure -eq 'connect') { throw 'Mock scheduler connection failure' }
    }
    $scheduler | Add-Member ScriptMethod GetFolder {
        param($Path)
        if ($fixture.failure -eq 'folder') { throw 'Mock scheduler folder failure' }
        return $folder
    }
    $folder | Add-Member ScriptMethod RegisterTask {
        param($Name,$Xml,$Flags,$Sid,$Password,$Logon,$Security)
        $fixture.events.Add('register')
        if ($fixture.failure -eq 'register') { throw 'Mock task registration failure' }
        $xmlTask=([xml]$Xml).Task
        $triggers=[pscustomobject]@{Count=1;value=[pscustomobject]@{UserId=$Sid}}
        $triggers | Add-Member ScriptMethod Item { param($Index) return $this.value }
        $actions=[pscustomobject]@{Count=1;value=[pscustomobject]@{Path=[string]$xmlTask.Actions.Exec.Command;Arguments=[string]$xmlTask.Actions.Exec.Arguments}}
        $actions | Add-Member ScriptMethod Item { param($Index) return $this.value }
        $fixture.task=[pscustomobject]@{Enabled=$true;Definition=[pscustomobject]@{Principal=[pscustomobject]@{UserId=$Sid;LogonType=3;RunLevel=1};Triggers=$triggers;Actions=$actions}}
        $fixture.task | Add-Member ScriptMethod GetSecurityDescriptor { param($Flags) return 'D:P(A;;FA;;;SY)(A;;FA;;;BA)' }
    }
    $folder | Add-Member ScriptMethod GetTask { param($Name) return $fixture.task }
    $folder | Add-Member ScriptMethod DeleteTask {
        param($Name,$Flags)
        $fixture.events.Add('delete')
        if ($null -eq $fixture.task) { throw (New-Object Runtime.InteropServices.COMException('Task not found',-2147024894)) }
        $fixture.task=$null
    }
    function New-Object {
        param([string]$TypeName,[object[]]$ArgumentList,[string]$ComObject)
        if ($ComObject) {
            Assert ($ComObject -eq 'Schedule.Service') 'Unexpected COM access'
            if ($fixture.failure -eq 'create') { throw 'Mock scheduler creation failure' }
            return $scheduler
        }
        Microsoft.PowerShell.Utility\New-Object @PSBoundParameters
    }
    function Get-FollowupRoot { return $testRoot }
    function New-FollowupDirectory($Path) { [void][IO.Directory]::CreateDirectory($Path) }
    function Assert-FollowupPath { }
    function New-FollowupAcl($Directory) {
        $acl=New-Object Security.AccessControl.FileSecurity
        $user=[Security.Principal.WindowsIdentity]::GetCurrent().User
        $acl.SetOwner($user); $acl.SetAccessRuleProtection($true,$false)
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($user,'FullControl','Allow')))
        return $acl
    }
    $facts=Facts
    $directory=Join-Path $testRoot $facts.partitionGuid
    $statePath=Join-Path $directory 'state.json'
    try {
        foreach ($failure in @('create','connect','folder','register')) {
            $fixture.failure=$failure
            Reject { Register-EncryptionReminder $facts 'suspend' } 'Scheduler failure must propagate'
            Assert ((Read-FollowupState $statePath).status -eq 'not-started') 'Scheduler failure stranded an armed record'
            $fixture.failure=''
            $retry=Register-EncryptionReminder $facts 'suspend'
            Assert ($retry.state.status -eq 'armed' -and $null -ne $fixture.task) 'Retry failed to register a reminder'
            [IO.File]::Delete($statePath); $fixture.task=$null
        }
        # A hard stop bypasses catch: reconstruct its persisted armed record.
        $registered=Register-EncryptionReminder $facts 'suspend'
        $fixture.task=$null; $fixture.events.Clear()
        $retry=Register-EncryptionReminder $facts 'suspend'
        Assert ($retry.state.status -eq 'armed' -and $null -ne $fixture.task) 'Interrupted registration was not recovered'
        Assert (($fixture.events -join ',') -eq 'connect,delete,connect,register') 'Retry failed to reconcile the previous task before registration'

        $existing=Read-FollowupState $statePath; $existing.status='pending'
        Write-FollowupState $statePath $existing
        $fixture.events.Clear()
        Reject { Register-EncryptionReminder (Facts 1 0) 'suspend' } 'Existing suspension was adopted'
        Reject { Register-EncryptionReminder (Facts 0 0) 'remind' } 'An unresolved prior change was discarded'
        Assert ($fixture.events.Count -eq 0) 'Unresolved change reached the scheduler'

        $existing.userSid='S-1-5-18'; Write-FollowupState $statePath $existing
        Reject { Register-EncryptionReminder $facts 'suspend' } 'Another account reminder was replaced'
        $existing.userSid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        $existing.volume.diskUniqueId='different-disk'; Write-FollowupState $statePath $existing
        Reject { Register-EncryptionReminder $facts 'suspend' } 'A different volume reminder was replaced'
        Assert ($fixture.events.Count -eq 0) 'Rejected ownership reached the scheduler'
    } finally {
        # Delete only these named disposable fixture files/directories.
        foreach ($name in @('state.json','BitLockerSetup.ps1')) {
            $path=Join-Path $directory $name
            if (Test-Path -LiteralPath $path) { [IO.File]::Delete($path) }
        }
        if (Test-Path -LiteralPath $directory) { [IO.Directory]::Delete($directory) }
        if (Test-Path -LiteralPath $testRoot) { [IO.Directory]::Delete($testRoot) }
    }
}

# Exercise action ordering, decryption errors, and explicit manual opt-in through
# the real preparation function, replacing every mutating/external boundary.
$script:events=New-Object 'Collections.Generic.List[string]'
$script:current=Facts
$script:failRegistration=$false; $script:failSuspend=$false
function Get-EncryptionFacts { return $script:current }
function Register-EncryptionReminder($Facts,$Mode) {
    Get-EncryptionEligibility $Facts $Mode
    $script:events.Add('register')
    if ($script:failRegistration) { throw 'Task registration failed' }
    return @{state=[pscustomobject]@{status='armed'};path='mock-state'}
}
function Get-CimInstance { return [pscustomobject]@{DeviceID='volume';PersistentVolumeID='persistent'} }
function Invoke-EncryptionQuery($Volume,$Method,$Arguments) {
    Assert ($Method -eq 'DisableKeyProtectors') 'Only suspension is permitted during preparation.'
    if ($script:current.isOsVolume) { Assert ($Arguments.DisableCount -eq 0) 'Suspension must survive installer reboots.' } else { Assert ($Arguments.Count -eq 0) 'Data volume must not receive OS-only DisableCount.' }
    $script:events.Add('suspend')
    if ($script:failSuspend) { throw 'Decryption failed' }
    $script:current.protection=0
    return @{ReturnValue=0}
}
function Write-FollowupState($Path,$Value) { $script:events.Add('write-'+$Value.status) }
$expected=Get-FactsHash $script:current
$script:failRegistration=$true
Reject { Start-EncryptionPreparation $expected 'suspend' } 'Registration failure must abort.'
Assert (($script:events -join ',') -eq 'register') 'Decryption happened without a registered reminder.'
$script:failRegistration=$false; $script:events.Clear(); $script:failSuspend=$true
Reject { Start-EncryptionPreparation $expected 'suspend' } 'Decryption failure must not report success.'
Assert (($script:events -join ',') -eq 'register,suspend') 'Failed decryption was recorded as confirmed.'
$script:failSuspend=$false; $script:events.Clear()
[void](Start-EncryptionPreparation $expected 'suspend')
Assert (($script:events -join ',') -eq 'register,suspend,write-pending') 'Reminder must be armed before mutation and confirmed afterward.'
$script:events.Clear(); $script:current=Facts; $script:current.isOsVolume=$false
[void](Start-EncryptionPreparation (Get-FactsHash $script:current) 'suspend')
Assert (($script:events -join ',') -eq 'register,suspend,write-pending') 'Data suspension did not complete.'
$script:events.Clear(); $script:current=Facts 0 0
[void](Start-EncryptionPreparation (Get-FactsHash $script:current) 'remind')
Assert (($script:events -join ',') -eq 'register') 'Manual reminder must not mutate encryption.'
$script:events.Clear()
Reject { Start-EncryptionPreparation $expected 'suspend' } 'Stale confirmation must fail.'
Assert ($script:events.Count -eq 0) 'Stale inspection registered a task or changed encryption.'

# Resumption is a separate explicit action, restricted to confirmed ownership.
$state.origin='installer-suspension'; $state.status='pending'; $script:current=Facts 1 0
Assert ((Get-FollowupDecision $state $script:current) -eq 'suspended') 'Owned suspension must offer resumption.'
Assert ($script:events.Count -eq 0) 'Checking a reminder must never resume protection.'
function Invoke-EncryptionQuery($Volume,$Method,$Arguments) {
    Assert ($Method -eq 'EnableKeyProtectors') 'Resume must not decrypt or recreate protectors.'
    $script:events.Add('resume'); $script:current.protection=1
    return @{ReturnValue=0}
}
function Remove-EncryptionReminder($Guid) { $script:events.Add('remove') }
$state.origin='manual-opt-in'
Reject { Resume-OwnedProtection $state 'mock' } 'Manual suspension was adopted.'
$state.origin='installer-suspension'; $state.status='armed'
Reject { Resume-OwnedProtection $state 'mock' } 'Unconfirmed suspension was adopted.'
Assert ($script:events.Count -eq 0) 'Rejected resume changed protection.'
$state.status='pending'
Resume-OwnedProtection $state 'mock'
Assert (($script:events -join ',') -eq 'resume,write-restored,remove') 'Only verified resumption may retire the reminder.'
# A prepared installer that has not been selected for startup hides Resume;
# unreadable or missing summaries keep the normal choices.
& {
    $summary=Join-Path $env:TEMP ('omarchy-staged-summary-'+[guid]::NewGuid().ToString('N')+'.json')
    try {
        Assert (-not (Test-StagedInstallerWaiting $summary)) 'A missing summary hid Resume.'
        foreach ($case in @(@{status='staged';waiting=$true},@{status='arming';waiting=$true},@{status='boot-scheduled';waiting=$false},@{status='copying';waiting=$false})) {
            [IO.File]::WriteAllText($summary,(@{operations=@(@{status=$case.status})} | ConvertTo-Json -Depth 4))
            Assert ((Test-StagedInstallerWaiting $summary) -eq $case.waiting) ('Wrong waiting state for '+$case.status)
        }
        [IO.File]::WriteAllText($summary,'not json')
        Assert (-not (Test-StagedInstallerWaiting $summary)) 'An unreadable summary hid Resume.'
    } finally { if (Test-Path -LiteralPath $summary) { [IO.File]::Delete($summary) } }
}
Write-Output 'Passed BitLocker follow-up policy and mocked preparation checks; no host changes.'
