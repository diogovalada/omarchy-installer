# Storage inspection and exact free-space/NTFS-shrink planning. Mutations exist
# only in Invoke-ConfirmedShrink, called after final approval by deploy.
function Align-Up([long]$Bytes) { return [long]([decimal]::Ceiling([decimal]$Bytes/1048576)*1048576) }
function Align-Down([long]$Bytes) { return [long]([decimal]::Floor([decimal]$Bytes/1048576)*1048576) }
function Get-ShrinkCandidate($Partition) {
    $issues=New-Object 'Collections.Generic.List[string]'
    $sizeMin=[long]0; $reserve=[long]0; $minimum=[long]$Partition.Size; $maximum=[long]0; $volumeId=''; $letter=''; $fs=''; $free=[long]0
    try {
        $volumes=@($Partition | Get-Volume -ErrorAction Stop)
        if ($volumes.Count -ne 1) { throw 'Partition has no uniquely identified Windows filesystem.' }
        $volume=$volumes[0]; $volumeId=[string]$volume.Path; $letter=[string]$volume.DriveLetter; $fs=[string]$volume.FileSystemType; $free=[long]$volume.SizeRemaining
        if ($fs -ne 'NTFS' -and @($volume.PSObject.Properties | ForEach-Object { $_.Name }) -contains 'FileSystem') { $fs=[string]$volume.FileSystem }
        if ($fs -ne 'NTFS' -or [string]$Partition.GptType -ine '{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}') { throw 'Only basic Windows NTFS data/OS partitions can be shrunk.' }
        if ($Partition.IsReadOnly -or [string]$volume.HealthStatus -notin @('Healthy','0') -or @($volume.OperationalStatus | Where-Object { [string]$_ -notin @('OK','2') }).Count -ne 0) { throw 'NTFS volume must be writable, healthy and operational.' }
        if ([long]$Partition.Offset % 1048576 -ne 0) { throw 'NTFS partition start is not MiB aligned.' }
        # This supported sizing analysis can start Windows Optimize Drives;
        # it does not resize or move the partition. Do not invoke in static QA.
        $supported=Get-PartitionSupportedSize -DiskNumber $Partition.DiskNumber -PartitionNumber $Partition.PartitionNumber -ErrorAction Stop
        $sizeMin=[long]$supported.SizeMin
        if ($sizeMin -le 0 -or $sizeMin -gt [long]$Partition.Size) { throw 'Windows returned unsupported NTFS shrink limits.' }
        $reserve=Align-Up ([long][Math]::Max(20GB,[decimal]$volume.Size/10))
        $used=[long]$volume.Size-$free
        $minimum=Align-Up ([long][Math]::Max($sizeMin,$used+$reserve))
        $maximum=[Math]::Max([long]0,(Align-Down ([long]$Partition.Offset+[long]$Partition.Size))-([long]$Partition.Offset+$minimum))
        if ($maximum -lt $script:minimumBytes) { $issues.Add('Windows cannot release the minimum installation size while retaining the free-space reserve.') }
    } catch { $issues.Add($_.Exception.Message) }
    return [ordered]@{partitionNumber=[int]$Partition.PartitionNumber;partitionGuid=[string]$Partition.Guid;volumeId=$volumeId;driveLetter=$letter;fileSystem=$fs;offsetBytes=[long]$Partition.Offset;sizeBytes=[long]$Partition.Size;windowsMinimumSizeBytes=$sizeMin;minimumSizeBytes=$minimum;reserveBytes=$reserve;freeBytes=$free;maximumAllocationBytes=[long]$maximum;eligible=($issues.Count -eq 0);blockers=@($issues.ToArray())}
}
function Get-Probe([string[]]$ProtectedPaths=@()) {
    $firmware=[Omarchy.DirectX86.NativeDisk]::Firmware(); $secure='unknown'
    try { $secure=if (Confirm-SecureBootUEFI) { 'enabled' } else { 'disabled' } } catch { }
    $bitLocker=Get-BitLockerSnapshot
    $bootEvidence=$null; $bootIssue=''
    if ($secure -eq 'disabled') {
        try { $bootEvidence=Get-VerifiedWindowsBoot } catch { $bootIssue=$_.Exception.Message }
    }
    $deletionContext=Get-DeletionContext $ProtectedPaths
    $disks=@()
    foreach ($disk in @(Get-Disk | Sort-Object Number)) {
        $blockers=New-Object 'Collections.Generic.List[string]'
        if ($firmware -ne 'uefi') { $blockers.Add('UEFI firmware is required.') }
        if ($secure -ne 'disabled') { $blockers.Add('Turn off Secure Boot in firmware before installing: this Omarchy release does not provide a compatible signed boot chain.') }
        if ($bootIssue) { $blockers.Add($bootIssue) }
        if ([string]$disk.PartitionStyle -ne 'GPT') { $blockers.Add('Only GPT partition tables are supported.') }
        if ($disk.IsOffline -or $disk.IsReadOnly) { $blockers.Add('The disk must already be online and writable.') }
        if ($disk.LogicalSectorSize -ne 512) { $blockers.Add('This image recipe requires 512-byte logical sectors.') }
        if ([string]$disk.BusType -notin @('NVMe','SATA','ATA','SCSI')) { $blockers.Add('Only basic local NVMe/SATA/ATA/SCSI disks are supported.') }
        if ([string]::IsNullOrWhiteSpace([string]$disk.UniqueId)) { $blockers.Add('A stable disk identity is required.') }
        try { [void](Get-AffectedBitLocker $bitLocker $disk.Number) } catch { $blockers.Add($_.Exception.Message) }
        $partitions=@(); $shrink=@(); $delete=@()
        try {
            foreach ($p in @(Get-Partition -DiskNumber $disk.Number -ErrorAction Stop | Sort-Object Offset)) {
                $partitions += [ordered]@{partitionNumber=[int]$p.PartitionNumber;guid=[string]$p.Guid;gptType=[string]$p.GptType;offsetBytes=[long]$p.Offset;sizeBytes=[long]$p.Size}
                if ([string]$p.GptType -in @('{5808c8aa-7e8f-42e0-85d2-e1e90434cfb3}','{af9b60a0-1431-4f62-bc68-3311714a69ad}')) { $blockers.Add('Dynamic Windows disks are unsupported.') }
                if ([string]$p.GptType -ieq '{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}') { $shrink += Get-ShrinkCandidate $p }
                $delete += Get-DeleteCandidate $p $deletionContext $bitLocker
            }
        } catch { $blockers.Add('Complete partition inspection failed: '+$_.Exception.Message) }
        $free=@(); $cursor=[long]1048576; $end=Align-Down ([long]$disk.Size-1048576)
        foreach ($p in $partitions) {
            $freeEnd=Align-Down $p.offsetBytes
            if ($freeEnd -gt $cursor) { $free += [ordered]@{offsetBytes=$cursor;sizeBytes=$freeEnd-$cursor} }
            $cursor=[Math]::Max($cursor,(Align-Up ([long]$p.offsetBytes+[long]$p.sizeBytes)))
        }
        if ($end -gt $cursor) { $free += [ordered]@{offsetBytes=$cursor;sizeBytes=$end-$cursor} }
        $hasSpace=@($free | Where-Object { $_.sizeBytes -ge $script:minimumBytes }).Count -gt 0 -or @($shrink | Where-Object { $_.eligible }).Count -gt 0 -or @($delete | Where-Object { $_.eligible }).Count -gt 0
        if (-not $hasSpace) { $blockers.Add('No unallocated space, supported NTFS shrink or eligible partition deletion can fit Omarchy.') }
        $disks += [ordered]@{diskNumber=[int]$disk.Number;diskUniqueId=[string]$disk.UniqueId;serialNumber=[string]$disk.SerialNumber;friendlyName=[string]$disk.FriendlyName;sizeBytes=[long]$disk.Size;logicalSectorBytes=[int]$disk.LogicalSectorSize;physicalSectorBytes=[int]$disk.PhysicalSectorSize;partitionStyle=[string]$disk.PartitionStyle;isOffline=[bool]$disk.IsOffline;isReadOnly=[bool]$disk.IsReadOnly;eligible=($blockers.Count -eq 0);blockers=@($blockers.ToArray());partitions=$partitions;freeExtents=$free;shrinkCandidates=$shrink;deleteCandidates=$delete}
    }
    $prerequisites=@(); $runtimePackaged=$false
    try { $runtimePackaged=$null -ne (Get-RuntimeDistribution) } catch { $prerequisites += @{code='runtime_distribution';available=$false;message=$_.Exception.Message} }
    try {
        $docker=Get-Docker; $runtime=Get-Runtime
        $engine=& $docker info --format '{{.OSType}}' 2>$null
        $prerequisites += @{code='linux_docker_engine';available=($LASTEXITCODE -eq 0 -and ($engine -join '').Trim() -eq 'linux');message='An already running Linux Docker engine is required.'}
        $image=& $docker image inspect $runtime.imageId --format '{{.Id}}' 2>$null
        $prerequisites += @{code='pinned_runtime';available=($LASTEXITCODE -eq 0 -and ($image -join '').Trim() -eq $runtime.imageId);message=('Required local image: '+$runtime.imageId)}
    } catch { $prerequisites += @{code='build_runtime';available=$false;message=$_.Exception.Message} }
    $freeMemory=[long]((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory*1024)
    $prerequisites += @{code='free_memory';available=($freeMemory -ge 10GB);message='Construction needs 10 GiB currently free memory.';availableBytes=$freeMemory;requiredBytes=[long]10GB}
    return [ordered]@{platform='windows-x64';firmware=$firmware;secureBoot=$secure;windowsBoot=$bootEvidence;bootIssue=$bootIssue;bitLocker=$bitLocker;prerequisites=$prerequisites;runtimePackaged=$runtimePackaged;minimumUnallocatedBytes=$script:minimumBytes;alignmentBytes=1048576;virtualDiskBytes=[long]40GB;buildFreeBytes=[long]85GB;espBytes=$script:espBytes;rootBytes=$script:rootBytes;encryption='luks2';protectionState='owner-setup-required';disks=$disks;qualification='Implementation is not yet qualified by a completed encrypted build, Windows shrink, BitLocker lifecycle, physical deployment or first boot.'}
}
function Select-Disk($Probe,[int]$Number,[string]$Identity) {
    $matches=@($Probe.disks | Where-Object { $_.diskNumber -eq $Number -and $_.diskUniqueId -ceq $Identity })
    if ($matches.Count -ne 1) { Fail 'target_changed' 'Selected disk identity no longer matches inspection.' }
    if (-not $matches[0].eligible) { Fail 'target_unsupported' ($matches[0].blockers -join ' ') }
    return $matches[0]
}
function Assert-AllocationBytes([long]$Size) {
    if ($Size -lt $script:minimumBytes -or $Size % 1048576 -ne 0) { Fail 'invalid_size' 'Installation size must be MiB aligned and at least the base image size.' }
}
function Select-Target($Probe,[int]$Number,[string]$Identity,[long]$Start,[long]$AllocationBytes) {
    Assert-AllocationBytes $AllocationBytes
    $disk=Select-Disk $Probe $Number $Identity
    if ($Start -lt 1048576 -or $Start % 1048576 -ne 0 -or $Start -gt [long]::MaxValue-$AllocationBytes) { Fail 'invalid_extent' 'Start offset must be an aligned positive byte offset.' }
    $fits=@($disk.freeExtents | Where-Object { $Start -ge $_.offsetBytes -and $Start+$AllocationBytes -le $_.offsetBytes+$_.sizeBytes })
    if ($fits.Count -ne 1) { Fail 'target_changed' 'Confirmed allocation is no longer entirely unallocated.' }
    return $disk
}
function Select-Allocation($Probe,$Request) {
    Assert-AllocationBytes $Request.allocationBytes
    $disk=Select-Disk $Probe $Request.diskNumber $Request.diskUniqueId
    if ($Request.targetKind -eq 'free') {
        [void](Select-Target $Probe $Request.diskNumber $Request.diskUniqueId $Request.startOffsetBytes $Request.allocationBytes)
        return [ordered]@{disk=$disk;startOffsetBytes=[long]$Request.startOffsetBytes;shrink=$null;delete=$null}
    }
    if ($Request.targetKind -eq 'delete') { return Select-Deletion $disk $Request }
    if ($Request.targetKind -ne 'shrink') { Fail 'invalid_target_kind' 'Target kind must be free or shrink.' }
    $candidates=@($disk.shrinkCandidates | Where-Object { $_.partitionNumber -eq $Request.shrinkPartitionNumber -and $_.partitionGuid -ceq $Request.shrinkPartitionGuid })
    if ($candidates.Count -ne 1 -or -not $candidates[0].eligible) { Fail 'shrink_unsupported' 'Selected NTFS volume no longer offers a supported shrink.' }
    $c=$candidates[0]
    if ([long]$Request.allocationBytes -gt [long]$c.maximumAllocationBytes) { Fail 'shrink_too_large' 'Selected size exceeds the current Windows shrink limit or free-space reserve.' }
    $start=(Align-Down ([long]$c.offsetBytes+[long]$c.sizeBytes))-[long]$Request.allocationBytes
    $newSize=$start-[long]$c.offsetBytes
    if ($newSize -lt [long]$c.minimumSizeBytes) { Fail 'shrink_too_large' 'Selected size would violate the Windows reserve.' }
    return [ordered]@{disk=$disk;startOffsetBytes=$start;delete=$null;shrink=[ordered]@{partitionNumber=$c.partitionNumber;partitionGuid=$c.partitionGuid;volumeId=$c.volumeId;driveLetter=$c.driveLetter;offsetBytes=$c.offsetBytes;beforeSizeBytes=$c.sizeBytes;afterSizeBytes=$newSize;shrinkBytes=([long]$c.sizeBytes-$newSize);reserveBytes=$c.reserveBytes;minimumSizeBytes=$c.minimumSizeBytes;freedOffsetBytes=$start;allocationBytes=[long]$Request.allocationBytes}}
}
function Invoke-ConfirmedShrink($Plan,[string]$Directory) {
    $s=$Plan.shrink
    $partition=Get-Partition -DiskNumber $Plan.diskNumber -PartitionNumber $s.partitionNumber -ErrorAction Stop
    if ([string]$partition.Guid -cne $s.partitionGuid -or [long]$partition.Offset -ne $s.offsetBytes -or [long]$partition.Size -ne $s.beforeSizeBytes) { Fail 'shrink_target_changed' 'The confirmed NTFS partition changed before resizing.' }
    $candidate=Get-ShrinkCandidate $partition
    if (-not $candidate.eligible -or $candidate.volumeId -ine $s.volumeId -or [long]$s.afterSizeBytes -lt $candidate.minimumSizeBytes -or $s.freedOffsetBytes -ne $s.offsetBytes+$s.afterSizeBytes) { Fail 'shrink_limit_changed' 'Windows volume identity, shrink limits or free-space reserve changed after confirmation.' }
    [byte[]]$before=[Omarchy.DirectX86.NativeDisk]::ReadLayout($Plan.diskNumber)
    if ([Omarchy.DirectX86.NativeDisk]::Hash($before) -ne $Plan.layoutSha256) { Fail 'target_changed' 'GPT layout changed before the confirmed shrink.' }
    Write-Json (Join-Path $Directory 'shrink-started.json') @{operationId=$Plan.operationId;shrink=$s;layoutSha256=$Plan.layoutSha256;startedAt=[DateTime]::UtcNow.ToString('o')}
    Resize-Partition -InputObject $partition -Size ([uint64]$s.afterSizeBytes) -Confirm:$false -ErrorAction Stop
    [byte[]]$after=[Omarchy.DirectX86.NativeDisk]::ReadLayout($Plan.diskNumber)
    $refreshedHash=[Omarchy.DirectX86.NativeDisk]::VerifyOnlyShrink($before,$after,[guid]$s.partitionGuid,[long]$s.offsetBytes,[long]$s.beforeSizeBytes,[long]$s.afterSizeBytes)
    $actual=Get-Partition -DiskNumber $Plan.diskNumber -PartitionNumber $s.partitionNumber -ErrorAction Stop
    if ([string]$actual.Guid -cne $s.partitionGuid -or [long]$actual.Offset -ne $s.offsetBytes -or [long]$actual.Size -ne $s.afterSizeBytes) { Fail 'shrink_readback_failed' 'Native NTFS shrink did not match the exact approved result.' }
    Write-Json (Join-Path $Directory 'shrink-completed.json') @{operationId=$Plan.operationId;shrink=$s;layoutSha256=$refreshedHash;completedAt=[DateTime]::UtcNow.ToString('o')}
    return $refreshedHash
}
function Assert-PhysicalDiskIdentity($Plan) {
    $disk=Get-Disk -Number $Plan.diskNumber -ErrorAction Stop
    if ([string]$disk.UniqueId -cne $Plan.diskUniqueId -or [string]$disk.SerialNumber -cne $Plan.serialNumber -or [long]$disk.Size -ne $Plan.diskSizeBytes -or [int]$disk.LogicalSectorSize -ne $Plan.logicalSectorBytes -or $disk.IsOffline -or $disk.IsReadOnly -or [string]$disk.PartitionStyle -ne 'GPT' -or [Omarchy.DirectX86.NativeDisk]::LayoutHash($Plan.diskNumber) -ne $Plan.layoutSha256) { Fail 'target_changed' 'Physical disk identity, geometry or GPT layout changed immediately before Windows changes.' }
}
