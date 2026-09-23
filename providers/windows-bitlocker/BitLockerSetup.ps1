[CmdletBinding()]
param(
    [ValidateSet('inspect','remind','follow-up')][string]$Action='inspect',
    [string]$RequestPath,
    [uint32]$DesktopProcessId,
    [switch]$LibraryOnly
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$env:PSModulePath=Join-Path ([Environment]::GetFolderPath('Windows')) 'System32/WindowsPowerShell/v1.0/Modules'
$script:EncryptionWorkerPath=$PSCommandPath

function Invoke-EncryptionQuery($Volume,[string]$Method,[hashtable]$Arguments=@{}) {
    $answer=Invoke-CimMethod -InputObject $Volume -MethodName $Method -Arguments $Arguments -ErrorAction Stop
    if ($answer.ReturnValue -ne 0) { throw "Windows encryption query/action failed ($Method, $($answer.ReturnValue))." }
    return $answer
}
function Get-EncryptionFacts([string]$VolumeId='') {
    $os=Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
    $volumes=@(Get-CimInstance -Namespace 'root/CIMV2/Security/MicrosoftVolumeEncryption' -ClassName Win32_EncryptableVolume -ErrorAction Stop | Where-Object { if ($VolumeId) { $_.DeviceID -ceq $VolumeId } else { $_.DriveLetter -ieq $os.SystemDrive } })
    if ($volumes.Count -ne 1) { throw 'Windows encryption could not be identified. No encryption change was made.' }
    $volume=$volumes[0]
    $parts=@(Get-Partition -ErrorAction Stop | Where-Object { @($_.AccessPaths) -contains $volume.DeviceID })
    if ($parts.Count -ne 1 -or -not $parts[0].Guid) { throw 'The Windows volume must identify one GPT partition.' }
    $conversion=Invoke-EncryptionQuery $volume 'GetConversionStatus'
    return [ordered]@{
        volumeId=[string]$volume.DeviceID; persistentVolumeId=[string]$volume.PersistentVolumeID
        partitionGuid=([guid]$parts[0].Guid).ToString(); diskUniqueId=[string](Get-Disk -Number $parts[0].DiskNumber -ErrorAction Stop).UniqueId
        driveLetter=[string]$volume.DriveLetter; isOsVolume=([string]$volume.DriveLetter -ieq $os.SystemDrive); conversion=[int]$conversion.ConversionStatus
        percentage=[int]$conversion.EncryptionPercentage
        protection=[int](Invoke-EncryptionQuery $volume 'GetProtectionStatus').ProtectionStatus
        locked=[int](Invoke-EncryptionQuery $volume 'GetLockStatus').LockStatus
        bootUtc=$os.LastBootUpTime.ToUniversalTime().ToString('o')
    }
}
function Get-EncryptionEligibility($Facts,[string]$Mode) {
    if ($Facts.locked -ne 0 -or $Facts.protection -notin @(0,1)) { throw 'Windows encryption status is unavailable or locked.' }
    if ($Mode -eq 'suspend') {
        if ($Facts.conversion -ne 1 -or $Facts.protection -ne 1) { throw 'Only fully encrypted volumes with active protection can be suspended. Existing suspension is not owned by this installer.' }
    } elseif ($Mode -eq 'remind') {
        if ($Facts.conversion -notin @(0,3,5) -or $Facts.protection -ne 0) { throw 'A manual restoration reminder is available when Windows is decrypted or being decrypted.' }
    } else { throw 'Unknown encryption preparation choice.' }
}
function Assert-SameEncryptionVolume($Before,$Now) {
    if ([guid]$Before.partitionGuid -ne [guid]$Now.partitionGuid) { throw 'The Windows partition identity changed. No action was taken.' }
    foreach ($name in @('volumeId','persistentVolumeId','diskUniqueId')) {
        if ([string]$Before.$name -cne [string]$Now.$name) { throw 'The Windows volume identity changed. No action was taken.' }
    }
}
function Get-FollowupDecision($State,$Facts) {
    if ($State.schema -ne 1 -or $State.origin -notin @('installer-suspension','installer-decryption','manual-opt-in')) { throw 'Unrecognized reminder ownership.' }
    Assert-SameEncryptionVolume $State.volume $Facts
    if ($State.status -in @('declined','restored','not-started')) { return 'finished' }
    if ($State.status -notin @('armed','pending')) { throw 'Unrecognized reminder state.' }
    if ($Facts.conversion -eq 1 -and $Facts.protection -eq 1) { return 'restored' }
    # A crash between the protection change and its receipt must not be presented
    # as a confirmed app-owned change. Keep an explicit-request check instead.
    if ($State.status -eq 'armed') { return 'uncertain' }
    if ($Facts.locked -ne 0 -or $Facts.protection -notin @(0,1)) { throw 'Windows encryption status could not be checked.' }
    if ($Facts.conversion -in @(2,4)) { return 'encrypting' }
    if ($Facts.conversion -in @(3,5)) { return 'decrypting' }
    if ($Facts.conversion -eq 0 -and $Facts.protection -eq 0) { return 'ask' }
    if ($Facts.conversion -eq 1 -and $Facts.protection -eq 0) { return 'suspended' }
    throw 'Windows encryption returned an unsupported state.'
}
function Assert-FollowupPath([string]$Path,[bool]$Directory) {
    $item=Get-Item -LiteralPath $Path -Force
    if (($item -is [IO.DirectoryInfo]) -ne $Directory) { throw 'Unexpected reminder storage type.' }
    for ($cursor=$item; $null -ne $cursor;) {
        if (($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Reminder storage must not contain links.' }
        $cursor=if ($cursor -is [IO.DirectoryInfo]) { $cursor.Parent } else { $cursor.Directory }
    }
    $acl=Get-Acl -LiteralPath $Path
    if (-not $acl.AreAccessRulesProtected -or $acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin @('S-1-5-18','S-1-5-32-544')) { throw 'Reminder storage is not protected.' }
    foreach ($rule in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
        if ($rule.AccessControlType -eq 'Allow' -and (([long]$rule.FileSystemRights -band 0xD0156) -ne 0) -and $rule.IdentityReference.Value -notin @('S-1-5-18','S-1-5-32-544')) { throw 'Reminder storage has unexpected write access.' }
    }
}
function New-FollowupAcl([bool]$Directory) {
    $acl=if ($Directory) { New-Object Security.AccessControl.DirectorySecurity } else { New-Object Security.AccessControl.FileSecurity }
    $acl.SetAccessRuleProtection($true,$false)
    $acl.SetOwner((New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')))
    foreach ($value in @('S-1-5-18','S-1-5-32-544')) {
        $sid=New-Object Security.Principal.SecurityIdentifier($value)
        $rule=if ($Directory) { New-Object Security.AccessControl.FileSystemAccessRule($sid,'FullControl','ContainerInherit, ObjectInherit','None','Allow') } else { New-Object Security.AccessControl.FileSystemAccessRule($sid,'FullControl','Allow') }
        $acl.AddAccessRule($rule)
    }
    return $acl
}
function New-FollowupDirectory([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { [void][IO.Directory]::CreateDirectory($Path,(New-FollowupAcl $true)) }
    Assert-FollowupPath $Path $true
}
function Write-FollowupState([string]$Path,$State) {
    $temporary=$Path+'.'+[guid]::NewGuid().ToString('N')+'.pending'
    $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($State | ConvertTo-Json -Depth 8)+"`n")
    try {
        $file=New-Object IO.FileStream($temporary,[IO.FileMode]::CreateNew,[Security.AccessControl.FileSystemRights]::Write,[IO.FileShare]::None,4096,[IO.FileOptions]::WriteThrough,(New-FollowupAcl $false))
        try { $file.Write($bytes,0,$bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
        if (Test-Path -LiteralPath $Path) { Assert-FollowupPath $Path $false; [IO.File]::Replace($temporary,$Path,[NullString]::Value) } else { [IO.File]::Move($temporary,$Path) }
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force } }
}
function Read-FollowupState([string]$Path) {
    Assert-FollowupPath $Path $false
    if ((Get-Item -LiteralPath $Path).Length -gt 65536) { throw 'Reminder record is too large.' }
    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}
function Get-FollowupRoot { return Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'OmarchyBitLockerFollowup' }
function Get-EncryptionLock($Facts) {
    $root=Get-FollowupRoot; New-FollowupDirectory $root
    $directory=Join-Path $root ([guid]$Facts.partitionGuid).ToString(); New-FollowupDirectory $directory
    $path=Join-Path $directory 'operation.lock'
    if (Test-Path -LiteralPath $path) { Assert-FollowupPath $path $false }
    return New-Object IO.FileStream($path,[IO.FileMode]::OpenOrCreate,[Security.AccessControl.FileSystemRights]::Write,[IO.FileShare]::None,4096,[IO.FileOptions]::None,(New-FollowupAcl $false))
}
function Get-FactsHash($Facts) {
    $sha=[Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes(($Facts | ConvertTo-Json -Compress))))).Replace('-','').ToLowerInvariant() } finally { $sha.Dispose() }
}
function Register-EncryptionReminder($Facts,[string]$Mode) {
    Get-EncryptionEligibility $Facts $Mode
    $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $root=Get-FollowupRoot; New-FollowupDirectory $root
    $directory=Join-Path $root $Facts.partitionGuid; New-FollowupDirectory $directory
    $statePath=Join-Path $directory 'state.json'
    # Under the per-volume lock, reconcile interrupted registration against the
    # actual volume. Active protection means there is no suspension left to
    # restore, even if a crash left an armed record with no scheduled task.
    if (Test-Path -LiteralPath $statePath) {
        $existing=Read-FollowupState $statePath
        if ($existing.userSid -cne $sid) { throw 'This BitLocker reminder belongs to another Windows account.' }
        $decision=Get-FollowupDecision $existing $Facts
        if ($decision -notin @('finished','restored')) { throw 'A previous BitLocker change is unresolved. Check Windows BitLocker settings before retrying.' }
        if ($decision -eq 'restored') {
            $existing.status='restored'; Write-FollowupState $statePath $existing
        }
        Remove-EncryptionReminder $Facts.partitionGuid
    }
    $worker=Join-Path $directory 'BitLockerSetup.ps1'
    if (Test-Path -LiteralPath $worker) { Assert-FollowupPath $worker $false }
    $bytes=[IO.File]::ReadAllBytes($script:EncryptionWorkerPath)
    $file=New-Object IO.FileStream($worker,[IO.FileMode]::Create,[Security.AccessControl.FileSystemRights]::Write,[IO.FileShare]::None,4096,[IO.FileOptions]::WriteThrough,(New-FollowupAcl $false))
    try { $file.Write($bytes,0,$bytes.Length); $file.Flush($true) } finally { $file.Dispose() }
    $state=[ordered]@{schema=1;origin=$(if ($Mode -eq 'suspend') {'installer-suspension'} else {'manual-opt-in'});status=$(if ($Mode -eq 'suspend') {'armed'} else {'pending'});volume=$Facts;createdBootUtc=$Facts.bootUtc;userSid=$sid;scriptSha256=(Get-FileHash -LiteralPath $worker -Algorithm SHA256).Hash.ToLowerInvariant()}
    Write-FollowupState $statePath $state
    $exe=Join-Path ([Environment]::GetFolderPath('Windows')) 'System32/WindowsPowerShell/v1.0/powershell.exe'
    $arguments='-NoProfile -STA -WindowStyle Hidden -ExecutionPolicy Bypass -File "'+$worker+'" -Action follow-up'
    $xmlExe=[Security.SecurityElement]::Escape($exe); $xmlArguments=[Security.SecurityElement]::Escape($arguments)
    $xml=@"
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Ask about Windows encryption after Omarchy preparation. Does not automatically enable encryption.</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>$sid</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Owner"><UserId>$sid</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>true</StartWhenAvailable><Enabled>true</Enabled><ExecutionTimeLimit>PT1H</ExecutionTimeLimit></Settings>
  <Actions Context="Owner"><Exec><Command>$xmlExe</Command><Arguments>$xmlArguments</Arguments></Exec></Actions>
</Task>
"@
    $name='OmarchyBitLockerFollowup-'+$Facts.partitionGuid
    try {
        $scheduler=New-Object -ComObject Schedule.Service; $scheduler.Connect(); $folder=$scheduler.GetFolder('\')
        # TASK_CREATE | TASK_DONT_ADD_PRINCIPAL_ACE: retain the explicit
        # administrator/SYSTEM DACL instead of adding an account-specific ACE.
        [void]$folder.RegisterTask($name,$xml,0x12,$sid,$null,3,'D:P(A;;FA;;;SY)(A;;FA;;;BA)')
        $task=$folder.GetTask($name)
        if (-not $task.Enabled -or $task.Definition.Principal.UserId -ne $sid -or $task.Definition.Principal.LogonType -ne 3 -or $task.Definition.Principal.RunLevel -ne 1 -or $task.Definition.Triggers.Count -ne 1 -or $task.Definition.Triggers.Item(1).UserId -ne $sid -or $task.Definition.Actions.Count -ne 1 -or $task.Definition.Actions.Item(1).Path -ine $exe -or $task.Definition.Actions.Item(1).Arguments -cne $arguments) { throw 'The reminder task did not verify.' }
        $security=New-Object Security.AccessControl.RawSecurityDescriptor($task.GetSecurityDescriptor(4))
        foreach ($ace in $security.DiscretionaryAcl) {
            if ($ace.AceType -eq [Security.AccessControl.AceType]::AccessAllowed -and $ace.SecurityIdentifier.Value -notin @('S-1-5-18','S-1-5-32-544')) { throw 'The reminder task grants an unexpected identity access.' }
        }
    } catch {
        # No protection change has been attempted. Retain a terminal record, not a
        # pending restoration obligation for a change we never made.
        $state.status='not-started'; Write-FollowupState $statePath $state
        throw
    }
    return @{state=$state;path=$statePath}
}
function Remove-EncryptionReminder([string]$PartitionGuid) {
    $guid=([guid]$PartitionGuid).ToString()
    $scheduler=New-Object -ComObject Schedule.Service; $scheduler.Connect()
    try { $scheduler.GetFolder('\').DeleteTask(('OmarchyBitLockerFollowup-'+$guid),0) }
    catch {
        $cause=$_.Exception; while ($cause.InnerException) { $cause=$cause.InnerException }
        if ($cause.HResult -ne -2147024894) { throw }
    }
}
function Start-EncryptionPreparation($Expected,[string]$Mode,[string]$VolumeId='') {
    $facts=Get-EncryptionFacts $VolumeId
    if ((Get-FactsHash $facts) -cne $Expected) { throw 'Windows encryption changed after review. Check it again before continuing.' }
    $context=Register-EncryptionReminder $facts $Mode
    if ($Mode -eq 'suspend') {
        # Persist a reminder before suspension, including interrupted staging.
        # Never decrypt the volume or remove any persistent key protector.
        $fresh=Get-EncryptionFacts $VolumeId
        Assert-SameEncryptionVolume $facts $fresh; Get-EncryptionEligibility $fresh $Mode
        $matches=@(Get-CimInstance -Namespace 'root/CIMV2/Security/MicrosoftVolumeEncryption' -ClassName Win32_EncryptableVolume | Where-Object { $_.DeviceID -ceq $facts.volumeId -and $_.PersistentVolumeID -ceq $facts.persistentVolumeId })
        if ($matches.Count -ne 1) { throw 'Windows volume identity changed before suspension.' }
        $arguments=if ($facts.isOsVolume) { @{DisableCount=[uint32]0} } else { @{} }
        [void](Invoke-EncryptionQuery $matches[0] 'DisableKeyProtectors' $arguments)
        $after=Get-EncryptionFacts $VolumeId
        Assert-SameEncryptionVolume $facts $after
        if ($after.conversion -ne 1 -or $after.protection -ne 0 -or $after.locked -ne 0) { throw 'BitLocker suspension could not be verified. The reminder will help you check its state.' }
        $context.state.status='pending'; Write-FollowupState $context.path $context.state
    }
    return @{reminderRegistered=$true;facts=(Get-EncryptionFacts $VolumeId);message=$(if ($Mode -eq 'suspend') {'BitLocker protection is suspended; data remains encrypted. A reminder to resume protection will appear at your next Windows sign-in.'} else {'Reminder registered at your request. It will appear at your next Windows sign-in.'})}
}
function Assert-EncryptionOwner([uint32]$ProcessId) {
    if ($ProcessId -eq 0) { throw 'The authenticated desktop process is required.' }
    $desktop=Get-CimInstance Win32_Process -Filter ("ProcessId="+$ProcessId) -ErrorAction Stop
    $owner=Invoke-CimMethod -InputObject $desktop -MethodName GetOwnerSid -ErrorAction Stop
    if ($owner.ReturnValue -ne 0 -or $owner.Sid -cne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value) { throw 'Use the same administrator account running the installer for suspension and its sign-in reminder.' }
}
function Resume-OwnedProtection($State,[string]$Path) {
    $facts=Get-EncryptionFacts $State.volume.volumeId
    if ($State.origin -ne 'installer-suspension' -or $State.status -ne 'pending' -or (Get-FollowupDecision $State $facts) -ne 'suspended') { throw 'Only an app-owned, confirmed suspension can be resumed here. Check Windows settings.' }
    $matches=@(Get-CimInstance -Namespace 'root/CIMV2/Security/MicrosoftVolumeEncryption' -ClassName Win32_EncryptableVolume | Where-Object { $_.DeviceID -ceq $facts.volumeId -and $_.PersistentVolumeID -ceq $facts.persistentVolumeId })
    if ($matches.Count -ne 1) { throw 'Volume identity changed before resuming protection.' }
    [void](Invoke-EncryptionQuery $matches[0] 'EnableKeyProtectors')
    if ((Get-FollowupDecision $State (Get-EncryptionFacts $State.volume.volumeId)) -ne 'restored') { throw 'Protection could not be verified. Check Windows settings; the reminder remains active.' }
    $State.status='restored'; Write-FollowupState $Path $State
    Remove-EncryptionReminder $State.volume.partitionGuid
}
# A prepared installer that has not been selected for startup still needs
# protection suspended. The summary is display-only; failure to read it keeps
# the normal choices.
function Test-StagedInstallerWaiting([string]$Path=(Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'OmarchyStagedInstaller/summary.json')) {
    try {
        if (-not (Test-Path -LiteralPath $Path)) { return $false }
        $summary=Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
        return @($summary.operations | Where-Object { $_.status -in @('staged','arming') }).Count -gt 0
    } catch { return $false }
}
function Show-EncryptionFollowup {
    $root=Get-FollowupRoot
    $guid=([guid](Split-Path -Leaf $PSScriptRoot)).ToString()
    if ($PSScriptRoot -ine (Join-Path $root $guid)) { throw 'Reminder program is outside its fixed directory.' }
    Assert-FollowupPath $root $true; Assert-FollowupPath $PSScriptRoot $true; Assert-FollowupPath $PSCommandPath $false
    $path=Join-Path $PSScriptRoot 'state.json'; $state=Read-FollowupState $path
    if ($state.userSid -cne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value -or $state.volume.partitionGuid -cne $guid -or $state.scriptSha256 -cne (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Reminder owner or program identity changed.' }
    $facts=Get-EncryptionFacts $state.volume.volumeId; $decision=Get-FollowupDecision $state $facts
    $installerWaiting=$decision -eq 'suspended' -and (Test-StagedInstallerWaiting)
    if ($decision -in @('finished','restored')) {
        if ($decision -eq 'restored') { $state.status='restored'; Write-FollowupState $path $state }
        Remove-EncryptionReminder $guid; return
    }
    Add-Type -AssemblyName System.Windows.Forms
    $form=New-Object Windows.Forms.Form
    $form.Text='Omarchy Installer - Windows encryption'; $form.Width=620; $form.Height=285
    $form.StartPosition='CenterScreen'; $form.FormBorderStyle='FixedDialog'; $form.MaximizeBox=$false; $form.MinimizeBox=$false
    $label=New-Object Windows.Forms.Label; $label.SetBounds(20,20,565,110)
    $label.Text=switch ($decision) {
        'ask' { 'Windows encryption is off. Would you like to turn BitLocker back on? Start Windows through your final Omarchy boot menu before doing so. Windows settings will guide you through activation and recovery-key backup. Closing this window reminds you at your next sign-in.' }
        'uncertain' { 'Omarchy preparation was interrupted before the encryption change was confirmed. Check Windows encryption settings. Nothing will be re-enabled automatically; this reminder stays until protection is verified or you explicitly dismiss it.' }
        'decrypting' { 'Windows decryption is still running or paused. Wait for it to finish before using the official installer. You can check its progress in Windows settings. The restoration reminder remains active.' }
        'encrypting' { 'Windows encryption is running or paused. It is not yet verified as fully protected. Check progress in Windows settings; this reminder will retire when encryption and protection are both verified.' }
        'suspended' { if ($installerWaiting) { 'BitLocker protection is suspended on '+$facts.driveLetter+' for the Omarchy installer prepared on this computer. Keep it suspended until Omarchy is installed; the installer cannot start while protection is on. Closing this window reminds you next sign-in.' } else { 'BitLocker protection is suspended on '+$facts.driveLetter+'. If you finished installing, first start Windows the way you normally will. You can then resume protection. If you cancelled installation, resume it now. Closing this window reminds you next sign-in.' } }
    }
    $settings=New-Object Windows.Forms.Button; $settings.Text='Open BitLocker settings'; $settings.SetBounds(20,155,175,38)
    $canResume=$state.origin -eq 'installer-suspension' -and $state.status -eq 'pending' -and $decision -eq 'suspended' -and -not $installerWaiting
    if ($canResume) { $settings.Text='Resume protection' }
    $settings.Add_Click({
        if ($canResume) {
            try {
                if ([Windows.Forms.MessageBox]::Show('Resume BitLocker protection now? After installing, do this only once Windows has started the way you normally start it.','Resume protection','YesNo','Question','Button2') -ne 'Yes') { return }
                Resume-OwnedProtection $state $path; $form.Close()
            } catch { [void][Windows.Forms.MessageBox]::Show($_.Exception.Message,'BitLocker','OK','Error') }
            return
        }
        # Windows owns activation, recovery-key backup and any required consent.
        $control=Join-Path ([Environment]::GetFolderPath('Windows')) 'System32/control.exe'
        try {
            Start-Process -FilePath $control -ArgumentList '/name Microsoft.BitLockerDriveEncryption' -WindowStyle Normal -ErrorAction Stop
            $form.Close()
        } catch { [void][Windows.Forms.MessageBox]::Show('Could not open BitLocker settings. Open Windows Settings and search for Device encryption or BitLocker. The reminder remains active.','Windows encryption','OK','Error') }
    })
    $later=New-Object Windows.Forms.Button; $later.Text='Remind me next time'; $later.SetBounds(205,155,175,38); $later.Add_Click({$form.Close()})
    $decline=New-Object Windows.Forms.Button; $decline.Text='Keep BitLocker off'; $decline.SetBounds(390,155,175,38)
    $decline.Enabled=$decision -in @('ask','uncertain','suspended')
    if ($decision -eq 'suspended') { $decline.Text='Leave suspended' }
    if ($decision -eq 'uncertain') { $decline.Text='Stop reminders' }
    $decline.Add_Click({
        try {
            $warning=if ($decision -eq 'suspended') { 'Stop reminding? BitLocker protection will stay suspended until you resume it in Windows settings.' } elseif ($decision -eq 'ask') { 'Stop reminding? Windows will remain unencrypted. You can enable BitLocker later in Windows settings.' } else { 'Stop reminders for this interrupted preparation? This will not change Windows encryption. Check its status in Windows settings.' }
            if ([Windows.Forms.MessageBox]::Show($warning,'Stop encryption reminders?','YesNo','Warning','Button2') -ne 'Yes') { return }
            $fresh=Get-EncryptionFacts $state.volume.volumeId
            if ((Get-FollowupDecision $state $fresh) -notin @('ask','uncertain','suspended')) { throw 'Windows encryption changed. Close this window and check Windows settings.' }
            $state.status='declined'; Write-FollowupState $path $state; Remove-EncryptionReminder $guid; $form.Close()
        } catch { [void][Windows.Forms.MessageBox]::Show($_.Exception.Message,'Windows encryption','OK','Error') }
    })
    $form.Controls.AddRange(@($label,$settings,$later,$decline)); $form.CancelButton=$later
    try { [void]$form.ShowDialog() } finally { $form.Dispose() }
}

if ($LibraryOnly) { return }
try {
    $identity=[Security.Principal.WindowsIdentity]::GetCurrent()
    if (-not (New-Object Security.Principal.WindowsPrincipal($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Windows encryption preparation requires administrator access.' }
    if ($Action -eq 'follow-up') {
        $record=Read-FollowupState (Join-Path $PSScriptRoot 'state.json')
        $guard=Get-EncryptionLock (Get-EncryptionFacts $record.volume.volumeId)
        try { Show-EncryptionFollowup } finally { $guard.Dispose() }
        return
    }
    if ($DesktopProcessId -eq 0) { throw 'The authenticated desktop process is required.' }
    $desktop=Get-CimInstance Win32_Process -Filter ("ProcessId="+$DesktopProcessId) -ErrorAction Stop
    $owner=Invoke-CimMethod -InputObject $desktop -MethodName GetOwnerSid -ErrorAction Stop
    if ($owner.ReturnValue -ne 0 -or $owner.Sid -cne $identity.User.Value) { throw 'Use an administrator Windows account for encryption preparation. The reminder must belong to the same account running the installer, not a different account supplied at the elevation prompt.' }
    if ($Action -eq 'inspect') { $facts=Get-EncryptionFacts; $result=@{facts=$facts;sha256=(Get-FactsHash $facts)} }
    else {
        if (-not $RequestPath -or (Get-Item -LiteralPath $RequestPath).Length -gt 65536) { throw 'Invalid encryption request.' }
        $request=Get-Content -LiteralPath $RequestPath -Raw | ConvertFrom-Json
        if (@($request.PSObject.Properties.Name).Count -ne 1 -or $request.expectedSha256 -notmatch '^[0-9a-f]{64}$') { throw 'Invalid encryption confirmation.' }
        $guard=Get-EncryptionLock (Get-EncryptionFacts)
        try { $result=Start-EncryptionPreparation $request.expectedSha256 $Action } finally { $guard.Dispose() }
    }
    [Console]::Out.WriteLine((@{protocolVersion=1;type='result';result=$result} | ConvertTo-Json -Depth 10 -Compress))
} catch {
    if ($Action -eq 'follow-up') {
        Add-Type -AssemblyName System.Windows.Forms
        [void][Windows.Forms.MessageBox]::Show(('The Windows encryption follow-up could not finish. Check BitLocker in Windows settings. No automatic encryption change was made.'+"`n`n"+$_.Exception.Message),'Omarchy Installer','OK','Error')
    } else { [Console]::Out.WriteLine((@{protocolVersion=1;type='error';code='bitlocker_preparation_failed';message=$_.Exception.Message} | ConvertTo-Json -Compress)) }
    exit 1
}
