# Deletion is a distinct, explicitly confirmed allocation mode. Discovery is
# read-only; Remove-Partition is reachable only from confirmed deployment.
function Get-DeletionContext([string[]]$ProtectedPaths=@()) {
    try {
        $volumes=@(Get-CimInstance Win32_Volume -ErrorAction Stop)
        $protected=New-Object 'Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
        foreach ($v in $volumes) {
            if ($v.BootVolume -or $v.SystemVolume -or $v.PageFilePresent) { [void]$protected.Add([string]$v.DeviceID) }
        }
        # Recovery can be placed on a basic data partition as well as the
        # dedicated recovery GPT type. Protect every matching offset; an
        # ambiguous disk mapping may overprotect, but must never permit deletion.
        $recoveryPath=Join-Path $env:SystemRoot 'System32\Recovery\ReAgent.xml'
        [xml]$recovery=Get-Content -LiteralPath $recoveryPath -Raw -ErrorAction Stop
        $location=$recovery.SelectSingleNode('/WindowsRE/WinreLocation')
        if ($null -eq $location) { throw 'Windows recovery location is unknown.' }
        $recoveryOffset=[long]::Parse($location.GetAttribute('offset'))
        # The privileged helper intentionally clears inherited environment
        # variables. Resolve this system path independently of ProgramData.
        $programData=[Environment]::GetFolderPath('CommonApplicationData')
        $paths=@($env:SystemRoot,$programData,$PSHOME,$script:providerRoot,$script:builderRoot)+@($ProtectedPaths)
        foreach ($page in @(Get-CimInstance Win32_PageFileUsage -ErrorAction Stop)) { $paths += [string]$page.Name }
        $crash=Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\CrashControl' -ErrorAction Stop
        foreach ($field in @('DumpFile','MinidumpDir','DedicatedDumpFile')) {
            if (@($crash.PSObject.Properties.Name) -contains $field -and -not [string]::IsNullOrWhiteSpace([string]$crash.$field)) {
                $path=[Environment]::ExpandEnvironmentVariables([string]$crash.$field)
                if ($path.StartsWith('\??\')) { $path=$path.Substring(4) }
                if ($path.StartsWith('\SystemRoot\',[StringComparison]::OrdinalIgnoreCase)) { $path=Join-Path $env:SystemRoot $path.Substring(12) }
                $paths += $path
            }
        }
        foreach ($path in $paths) {
            if ([string]::IsNullOrWhiteSpace($path) -or $path -notmatch '^(?:[A-Za-z]:\\|\\\\\?\\(?:[A-Za-z]:\\|Volume\{[0-9a-fA-F-]{36}\}\\))') { throw 'An installer or Windows path could not be identified.' }
            [void]$protected.Add([Omarchy.DirectX86.NativeDisk]::VolumeForPath($path))
        }
        return [ordered]@{known=$true;volumes=$volumes;protectedVolumes=$protected;recoveryOffset=$recoveryOffset;error=''}
    } catch { return [ordered]@{known=$false;volumes=@();protectedVolumes=$null;error='Deletion is unavailable because Windows system, paging, recovery or installer storage could not be fully identified.'} }
}
function Get-DeleteCandidate($Partition,$Context,$BitLocker) {
    $issues=New-Object 'Collections.Generic.List[string]'
    $fs='Unknown'; $label=''; $volumeId=''; $maximum=[long]0
    $type=([string]$Partition.GptType).Trim('{}').ToLowerInvariant()
    $basic=$type -eq 'ebd0a0a2-b9e5-4433-87c0-68b6b72699c7'
    $linux=$type -eq '0fc63daf-8483-4772-8e79-3d69d8477de4'
    try {
        if (-not $basic -and -not $linux) { throw 'Boot, recovery, reserved, encrypted-container and unknown partition types are protected.' }
        foreach ($flag in @('IsBoot','IsSystem','IsReadOnly','IsOffline','IsHidden','IsShadowCopy')) {
            if ($null -eq $Partition.$flag) { throw 'The partition role or access state is unknown.' }
            if ($Partition.$flag) { throw 'This partition is used by Windows, protected, hidden, offline or read-only.' }
        }
        if (-not $Context.known) { throw $Context.error }
        if ($Context.recoveryOffset -gt 0 -and [long]$Partition.Offset -eq $Context.recoveryOffset) { throw 'This partition may contain Windows recovery tools and is protected.' }
        if (-not $BitLocker.known) { throw 'Deletion requires a successful administrator check of Windows encryption.' }
        if ([guid]::Parse([string]$Partition.Guid) -eq [guid]::Empty -or [long]$Partition.Offset -lt 1048576 -or [long]$Partition.Offset % 1048576 -ne 0) { throw 'The partition identity or alignment is unsupported.' }
        $maximum=Align-Down ([long]$Partition.Size)
        $label=[Omarchy.DirectX86.NativeDisk]::ReadDeletionPartitionName([int]$Partition.DiskNumber,[int]$Partition.PartitionNumber,[guid]$Partition.Guid,[long]$Partition.Offset,[long]$Partition.Size)
        if ($basic) {
            $matches=@($Context.volumes | Where-Object { @($Partition.AccessPaths) -contains [string]$_.DeviceID })
            if ($matches.Count -ne 1) { throw 'The partition has no uniquely identified Windows volume.' }
            $v=$matches[0]; $volumeId=[string]$v.DeviceID; $fs=[string]$v.FileSystem
            if (-not [string]::IsNullOrWhiteSpace([string]$v.Label)) { $label=[string]$v.Label }
            if ($Context.protectedVolumes.Contains($volumeId)) { throw 'This partition contains Windows system files, paging/crash-dump files or installer files.' }
            if ($null -eq $v.BootVolume -or $null -eq $v.SystemVolume -or $null -eq $v.PageFilePresent -or $v.BootVolume -or $v.SystemVolume -or $v.PageFilePresent) { throw 'The volume is in use by Windows or its role is unknown.' }
            if ($fs -notin @('NTFS','exFAT','FAT32','FAT')) { throw 'The filesystem or storage layout cannot be safely identified for deletion.' }
            $encrypted=@($BitLocker.volumes | Where-Object { $_.partitionGuid -ieq [string]$Partition.Guid -and $_.diskNumber -eq $Partition.DiskNumber })
            foreach ($entry in $encrypted) {
                if (-not $entry.supported -or $entry.isOsVolume -or $entry.isBootVolume -or $entry.conversionStatus -ne 0 -or $entry.protectionStatus -ne 0 -or $entry.lockStatus -ne 0) { throw 'Encrypted, locked or Windows system volumes cannot be deleted here.' }
            }
        } else {
            if (@($Partition.AccessPaths | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }).Count -ne 0) { throw 'Unmount the Linux partition before deleting it.' }
            $fs=[Omarchy.DirectX86.NativeDisk]::InspectLinuxFilesystem([int]$Partition.DiskNumber,[int]$Partition.PartitionNumber,[guid]$Partition.Guid,[long]$Partition.Offset,[long]$Partition.Size)
            if ($fs -notin @('ext4','ext2/ext3','Btrfs')) { throw 'Encrypted, multi-device or unknown Linux storage must be managed from Linux.' }
        }
        if ($maximum -lt $script:minimumBytes) { throw 'Deleting this partition alone would not provide enough space for Omarchy.' }
    } catch { $issues.Add($_.Exception.Message) }
    return [ordered]@{partitionNumber=[int]$Partition.PartitionNumber;partitionGuid=[string]$Partition.Guid;gptType=[string]$Partition.GptType;offsetBytes=[long]$Partition.Offset;sizeBytes=[long]$Partition.Size;maximumAllocationBytes=$maximum;volumeId=$volumeId;fileSystem=$fs;label=$label;eligible=($issues.Count -eq 0);blockers=@($issues.ToArray())}
}
function Select-Deletion($Disk,$Request) {
    $expected='Disk '+$Disk.diskNumber+' Partition '+$Request.deletePartitionNumber
    if ([string]$Request.deleteConfirmation -cne $expected) { Fail 'delete_confirmation_required' ('Type '+$expected+' in the partition deletion warning before continuing.') }
    $candidates=@($Disk.deleteCandidates | Where-Object { $_.partitionNumber -eq $Request.deletePartitionNumber -and $_.partitionGuid -ceq $Request.deletePartitionGuid })
    if ($candidates.Count -ne 1 -or -not $candidates[0].eligible) { Fail 'delete_unsupported' 'The selected partition is protected, in use or no longer eligible for deletion.' }
    $c=$candidates[0]
    if ([long]$c.offsetBytes -ne [long]$Request.deleteOffsetBytes -or [long]$c.sizeBytes -ne [long]$Request.deleteSizeBytes -or [long]$Request.allocationBytes -gt [long]$c.maximumAllocationBytes) { Fail 'delete_target_changed' 'The confirmed partition identity, size or extent changed.' }
    return [ordered]@{disk=$Disk;startOffsetBytes=[long]$c.offsetBytes;shrink=$null;delete=[ordered]@{partitionNumber=$c.partitionNumber;partitionGuid=$c.partitionGuid;gptType=$c.gptType;offsetBytes=$c.offsetBytes;sizeBytes=$c.sizeBytes;volumeId=$c.volumeId;fileSystem=$c.fileSystem;label=$c.label;confirmation=$expected;allocationBytes=[long]$Request.allocationBytes}}
}
function Invoke-ConfirmedDeletion($Plan,[string]$Directory) {
    $d=$Plan.delete
    $p=Get-Partition -DiskNumber $Plan.diskNumber -PartitionNumber $d.partitionNumber -ErrorAction Stop
    if ([string]$p.Guid -cne $d.partitionGuid -or [long]$p.Offset -ne $d.offsetBytes -or [long]$p.Size -ne $d.sizeBytes) { Fail 'delete_target_changed' 'The confirmed partition changed immediately before deletion.' }
    $candidate=Get-DeleteCandidate $p (Get-DeletionContext $Plan.protectedPaths) (Get-BitLockerSnapshot)
    if (-not $candidate.eligible) { Fail 'delete_unsupported' ($candidate.blockers -join ' ') }
    foreach ($field in @('partitionNumber','partitionGuid','gptType','offsetBytes','sizeBytes','volumeId','fileSystem','label')) {
        if ([string]$candidate.$field -cne [string]$d.$field) { Fail 'delete_target_changed' ('The deletion target changed: '+$field) }
    }
    if ($d.confirmation -cne ('Disk '+$Plan.diskNumber+' Partition '+$d.partitionNumber)) { Fail 'delete_confirmation_required' 'The partition deletion confirmation is missing.' }
    $lease=[Omarchy.DirectX86.NativeDisk]::LockDeletionTarget($Plan.diskNumber,$d.partitionNumber,[guid]$d.partitionGuid,$d.offsetBytes,$d.sizeBytes,$d.volumeId)
    try {
        Assert-PhysicalDiskIdentity $Plan
        [byte[]]$before=[Omarchy.DirectX86.NativeDisk]::ReadLayout($Plan.diskNumber)
        if ([Omarchy.DirectX86.NativeDisk]::Hash($before) -ne $Plan.layoutSha256) { Fail 'target_changed' 'GPT layout changed before the confirmed deletion.' }
        Write-Json (Join-Path $Directory 'deletion-started.json') @{operationId=$Plan.operationId;delete=$d;layoutSha256=$Plan.layoutSha256;startedAt=[DateTime]::UtcNow.ToString('o')}
        # Retain Windows' own system/boot/paging restrictions. Never override
        # partition types, force a dismount, or delete an entire disk layout.
        Remove-Partition -InputObject $p -Confirm:$false -ErrorAction Stop
        [byte[]]$after=[Omarchy.DirectX86.NativeDisk]::ReadLayout($Plan.diskNumber)
        $hash=[Omarchy.DirectX86.NativeDisk]::VerifyOnlyDelete($before,$after,[guid]$d.partitionGuid,$d.offsetBytes,$d.sizeBytes)
        Write-Json (Join-Path $Directory 'deletion-completed.json') @{operationId=$Plan.operationId;delete=$d;layoutSha256=$hash;completedAt=[DateTime]::UtcNow.ToString('o')}
        return $hash
    } finally { $lease.Dispose() }
}
