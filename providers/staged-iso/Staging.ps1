# Loaded by the authenticated provider; definitions have no entrypoint effects.
function Get-StagingRelease([string]$Hash,[long]$Length) {
    if ($Hash -cnotmatch '^[0-9a-f]{64}$' -or $Length -le 0 -or $Length -gt 63GB) { throw 'Invalid staged ISO identity or unsupported size.' }
    $catalog=Read-Json (Join-Path $PSScriptRoot 'qualified-releases.json')
    if ($catalog.schema -ne 1) { throw 'Unknown staged ISO qualification catalog.' }
    $found=@($catalog.releases | Where-Object { $_.sha256 -ceq $Hash -and $_.sizeBytes -eq $Length })
    if ($found.Count -eq 0 -and $script:testingBuild) {
        # Only the separately compiled testing helper supplies this switch.
        # The helper still authenticates the downloaded official ISO; staging
        # rehashes it and discovers its unmodified boot files before allocation.
        return [pscustomobject]@{sha256=$Hash;sizeBytes=$Length;minimumLinuxBytes=[long]32GB;kernelPath='';initrdPath='';bootQualified=$false;sourceProtection='unqualified';ntfsSource=$false}
    }
    if ($found.Count -ne 1) { throw 'No released official ISO has been qualified for staging. Installation without USB remains disabled.' }
    $release=$found[0]
    if ($release.sourceProtection -cne 'direct-gpt-partition' -or $release.ntfsSource -ne $true -or $release.bootQualified -ne $true -or $release.minimumLinuxBytes -lt 32GB) { throw 'The official ISO lacks the required staged boot qualification.' }
    foreach ($path in @($release.kernelPath,$release.initrdPath)) { Assert-StagingRelativePath $path }
    if ($release.kernelPath -notmatch '^arch/boot/x86_64/[a-zA-Z0-9._-]+$' -or $release.initrdPath -notmatch '^arch/boot/x86_64/[a-zA-Z0-9._-]+$') { throw 'Unsupported official ISO boot files.' }
    return $release
}
function Assert-StagingRelativePath([string]$Path) {
    if (-not $Path -or $Path.Length -gt 220 -or $Path -match '[\\:\x00-\x1f*?"<>|]' -or $Path.StartsWith('/')) { throw 'Unsafe staged filename.' }
    foreach ($piece in $Path.Split('/')) {
        if (-not $piece -or $piece -in @('.','..') -or $piece -match '[. ]$' -or $piece -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') { throw 'Unsafe staged filename component.' }
    }
}
function Get-StagingEncryption([int]$DiskNumber) {
    $snapshot=Get-BitLockerSnapshot -ForStaging
    $volumes=@(Get-AffectedBitLocker $snapshot $DiskNumber)
    foreach ($volume in $volumes) {
        if ($volume.conversionStatus -notin @(0,1) -or $volume.protectionStatus -notin @(0,1) -or $volume.lockStatus -ne 0) { throw 'Unlock affected Windows volumes and finish any encryption or decryption before preparing the installer.' }
    }
    return $volumes
}
function Assert-StagingEncryption([int]$DiskNumber) {
    foreach ($volume in @(Get-StagingEncryption $DiskNumber)) {
        if ($volume.protectionStatus -ne 0) { throw 'BitLocker protection is active. Review preparation again before writing or selecting installer startup.' }
    }
}
function Start-StagingSuspension($Plan,[uint32]$AuthenticatedPid) {
    $current=@(Get-StagingEncryption $Plan.diskNumber)
    Assert-BitLockerUnchanged @($Plan.bitLocker) $current
    $active=@($current | Where-Object { $_.protectionStatus -eq 1 })
    if ($active.Count) { Assert-EncryptionOwner $AuthenticatedPid }
    foreach ($volume in $active) {
        $facts=Get-EncryptionFacts $volume.volumeId
        Assert-SameEncryptionVolume $volume $facts
        $lock=Get-EncryptionLock $facts
        try {
            Emit 'progress' 'suspending-bitlocker' @{message=('Suspending BitLocker protection on '+$volume.driveLetter+'; data remains encrypted...')}
            [void](Start-EncryptionPreparation (Get-FactsHash $facts) 'suspend' $volume.volumeId)
        } finally { $lock.Dispose() }
    }
    Assert-StagingEncryption $Plan.diskNumber
}
function Get-StagingDisk([int]$Number,[string]$Identity) {
    $disk=Get-Disk -Number $Number -ErrorAction Stop
    if ([string]$disk.UniqueId -cne $Identity -or -not $Identity -or $disk.PartitionStyle -ne 'GPT' -or $disk.IsOffline -or $disk.IsReadOnly -or $disk.LogicalSectorSize -ne 512 -or [string]$disk.BusType -notin @('NVMe','SATA','ATA','SCSI')) { throw 'Unsupported or changed staging disk.' }
    if ($disk.PhysicalSectorSize -lt 512 -or $disk.PhysicalSectorSize -gt 65536 -or ($disk.PhysicalSectorSize -band ($disk.PhysicalSectorSize-1)) -ne 0) { throw 'Unsupported physical sector alignment.' }
    foreach ($p in @(Get-InspectedPartitions $disk)) {
        if ([string]$p.GptType -in @('{5808c8aa-7e8f-42e0-85d2-e1e90434cfb3}','{af9b60a0-1431-4f62-bc68-3311714a69ad}')) { throw 'Dynamic disks are not supported for staging.' }
    }
    return $disk
}
function Assert-StagingFree($Disk,[long]$Start,[long]$Size) {
    if ($Start -lt 1MB -or $Size -le 0 -or $Start % 1MB -ne 0 -or $Size % 1MB -ne 0 -or $Start -gt [long]::MaxValue-$Size -or $Start+$Size -gt [long]$Disk.Size-1MB) { throw 'Invalid staging extent.' }
    foreach ($p in @(Get-InspectedPartitions $Disk)) {
        if ($Start -lt [long]$p.Offset+[long]$p.Size -and [long]$p.Offset -lt $Start+$Size) { throw 'Staging or installation space overlaps an existing partition.' }
    }
}
function Get-StagingChoices($Release,$Resize=$null) {
    $choices=@(); $blocked=@(); $disks=@(); $overhead=512MB+(Align-Up ([long]$Release.sizeBytes+1GB))
    $script:minimumBytes=[long]$overhead
    if ($null -ne $Resize) {
        Assert-Fields $Resize @('diskNumber','diskUniqueId','partitionNumber','partitionGuid') @('diskNumber','diskUniqueId','partitionNumber','partitionGuid')
        if ($Resize.diskNumber -lt 0 -or $Resize.diskNumber -gt 4095 -or -not $Resize.diskUniqueId -or $Resize.partitionNumber -le 0) { throw 'Invalid resize inspection target.' }
        $resizeDisk=Get-StagingDisk $Resize.diskNumber $Resize.diskUniqueId
        $matches=@(Get-InspectedPartitions $resizeDisk | Where-Object { $_.PartitionNumber -eq $Resize.partitionNumber -and [guid]$_.Guid -eq [guid]$Resize.partitionGuid })
        if ($matches.Count -ne 1) { throw 'The partition changed. Refresh disks before checking resize options.' }
    }
    Emit 'progress' 'inspecting' @{message='Reading disks and Windows encryption status...'}
    $encryptionSnapshot=Get-BitLockerSnapshot -ForStaging
    foreach ($disk in @(Get-Disk)) {
        $encryption=@()
        try {
            [void](Get-StagingDisk $disk.Number $disk.UniqueId)
            # Inspection never changes encryption or requires suspension.
            if ($encryptionSnapshot.known) {
                $encryption=@($encryptionSnapshot.volumes | Where-Object {
                    ($_.diskNumber -eq $disk.Number -or $_.isOsVolume -or $_.isBootVolume) -and
                    ($_.conversionStatus -ne 0 -or $_.protectionStatus -ne 0 -or $_.lockStatus -ne 0)
                } | ForEach-Object {
                    @{driveLetter=[string]$_.driveLetter;isOsVolume=[bool]$_.isOsVolume;conversionStatus=[int]$_.conversionStatus;
                      protectionStatus=[int]$_.protectionStatus;lockStatus=[int]$_.lockStatus}
                })
            }
            Emit 'progress' 'inspecting' @{message=('Reading partitions on Disk '+$disk.Number+'...')}
            $diskChoices=@(); $regions=@(); $unallocated=[long]0; $largest=[long]0
            $parts=@(Get-InspectedPartitions $disk | Sort-Object Offset); $cursor=[long]1MB
            foreach ($p in @($parts)+@([pscustomobject]@{Offset=(Align-Down ([long]$disk.Size-1MB));Size=0;GptType=''})) {
                $gap=[Math]::Max([long]0,(Align-Down ([long]$p.Offset))-$cursor)
                $available=$gap-$overhead
                if ($gap -gt 0) {
                    $regions+=@{kind='free';offsetBytes=$cursor;sizeBytes=$gap;label='Unallocated space'}
                    $unallocated+=$gap; $largest=[Math]::Max($largest,$gap)
                }
                if ($available -ge 0) { $diskChoices+=@{diskNumber=[int]$disk.Number;diskUniqueId=[string]$disk.UniqueId;label=('Unallocated space ('+[Math]::Round($gap/1GB,1)+' GiB)');target=@{target_kind='free';start_offset_bytes=$cursor}} }
                $cursor=Align-Up ([long]$p.Offset+[long]$p.Size)
                if ($p.Size -eq 0) { continue }
                $region=@{kind='partition';partitionNumber=[int]$p.PartitionNumber;partitionGuid=[string]$p.Guid;offsetBytes=[long]$p.Offset;sizeBytes=[long]$p.Size;label=('Partition '+$p.PartitionNumber);fileSystem='';freeBytes=$null;resizeState='unavailable';resizeReason='System or unsupported partition; kept unchanged.';maximumReleaseBytes=$null;reserveBytes=$null}
                switch ([string]$p.GptType) {
                    '{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}' { $region.label='EFI system' }
                    '{e3c9e316-0b5c-4db8-817d-f92df00215ae}' { $region.label='Microsoft reserved' }
                    '{de94bba4-06d1-4d40-a16a-bfd50179d6ac}' { $region.label='Windows recovery' }
                }
                if ([string]$p.GptType -ieq '{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}') {
                    try {
                        $volumes=@($p | Get-Volume -ErrorAction Stop)
                        if ($volumes.Count -ne 1) { throw 'Filesystem information is unavailable.' }
                        $v=$volumes[0]; $region.fileSystem=[string]$v.FileSystemType
                        if ($region.fileSystem -ne 'NTFS' -and $v.PSObject.Properties['FileSystem']) { $region.fileSystem=[string]$v.FileSystem }
                        $volumeLabel=if ($v.PSObject.Properties['FileSystemLabel']) { ([string]$v.FileSystemLabel).Trim() } else { '' }
                        if ($v.DriveLetter) {
                            $drive=([string]$v.DriveLetter+':')
                            $region.label=if ($volumeLabel) { "$volumeLabel ($drive)" } else { $drive }
                        } elseif ($volumeLabel) { $region.label="$volumeLabel (Partition $($p.PartitionNumber))" }
                        $region.freeBytes=if ($region.fileSystem -in @('NTFS','FAT32','exFAT','ReFS')) { [long]$v.SizeRemaining } else { $null }
                        if ($region.fileSystem -eq 'NTFS') {
                            # A partition cannot release more than its unused bytes. Avoid
                            # Windows' slow exact shrink analysis when its unused bytes
                            # cannot fit the installer after the existing reserve.
                            $reserve=Align-Up ([long][Math]::Max(20GB,[decimal]$v.Size/10))
                            if ($region.freeBytes -lt $overhead+$reserve) {
                                $region.resizeState='insufficient'
                                $region.reserveBytes=$reserve
                                $region.resizeReason='Not enough space can be released while keeping the Windows free-space reserve. Free up space in Windows, then refresh.'
                            } else { $region.resizeState='unchecked'; $region.resizeReason='' }
                        }
                        else { $region.resizeReason='Only NTFS partitions can be resized here.' }
                    } catch { $region.resizeReason='Cannot read this filesystem. Unlock it if needed, then refresh.' }
                    if ($null -ne $Resize -and $Resize.diskNumber -eq $disk.Number -and $Resize.diskUniqueId -ceq [string]$disk.UniqueId -and $Resize.partitionNumber -eq $p.PartitionNumber -and [guid]$Resize.partitionGuid -eq [guid]$p.Guid -and $region.resizeState -eq 'unchecked') {
                        Emit 'progress' 'analyzing-resize' @{message=('Checking how much '+$region.label+' can shrink. Windows may take several minutes; no partitions are being changed.')}
                        $candidate=Get-ShrinkCandidate $p
                        $region.maximumReleaseBytes=if ($candidate.windowsMinimumSizeBytes -gt 0) { [long]$candidate.maximumAllocationBytes } else { $null }
                        $region.reserveBytes=[long]$candidate.reserveBytes
                        $region.resizeState=if ($candidate.windowsMinimumSizeBytes -gt 0) { 'checked' } else { 'failed' }
                        $region.resizeReason=($candidate.blockers -join ' ')
                        $available=$candidate.maximumAllocationBytes-$overhead
                        if ($candidate.eligible -and $available -ge 0) { $diskChoices+=@{diskNumber=[int]$disk.Number;diskUniqueId=[string]$disk.UniqueId;label=('Resize '+$region.label);target=@{target_kind='shrink';partition_number=[int]$p.PartitionNumber;partition_guid=[string]$p.Guid}} }
                    }
                }
                $regions+=$region
            }
            $disks+=@{diskNumber=[int]$disk.Number;diskUniqueId=[string]$disk.UniqueId;diskSizeBytes=[long]$disk.Size;regions=$regions;unallocatedBytes=$unallocated;largestFreeBytes=$largest;encryption=$encryption}
            foreach ($choice in $diskChoices) {
                $choice.diskSizeBytes=[long]$disk.Size; $choice.encryption=$encryption
                $remaining=@(foreach ($region in $regions) {
                    if ($region.kind -ne 'free') { continue }
                    if ($choice.target.target_kind -eq 'free' -and $region.offsetBytes -eq $choice.target.start_offset_bytes) { [long]($region.sizeBytes-$overhead) }
                    else { [long]$region.sizeBytes }
                })
                $choice.largestFreeAfterStagingBytes=if ($remaining.Count) { [long](($remaining | Measure-Object -Maximum).Maximum) } else { [long]0 }
            }
            $choices+=$diskChoices
        } catch { $blocked+=@{diskNumber=[int]$disk.Number;diskUniqueId=[string]$disk.UniqueId;diskSizeBytes=[long]$disk.Size;reason=$_.Exception.Message;encryption=$encryption} }
    }
    return @{choices=$choices;blocked=$blocked;disks=$disks;minimumLinuxBytes=$Release.minimumLinuxBytes;temporaryBytes=$overhead}
}
function Merge-StagingAnalysis($Inventory,$Measured,$Query) {
    $disk=@($Inventory.disks | Where-Object { $_.diskNumber -eq $Query.diskNumber -and $_.diskUniqueId -ceq $Query.diskUniqueId })
    $fresh=@($Measured.disks | Where-Object { $_.diskNumber -eq $Query.diskNumber -and $_.diskUniqueId -ceq $Query.diskUniqueId })
    if ($disk.Count -ne 1 -or $fresh.Count -ne 1 -or $disk[0].diskSizeBytes -ne $fresh[0].diskSizeBytes) { throw 'Disk layout changed during inspection. Refresh disks.' }
    $oldRegions=@($disk[0].regions); $newRegions=@($fresh[0].regions)
    if ($oldRegions.Count -ne $newRegions.Count) { throw 'Partitions changed during inspection. Refresh disks.' }
    for ($i=0; $i -lt $oldRegions.Count; $i++) {
        if ($oldRegions[$i].kind -cne $newRegions[$i].kind -or $oldRegions[$i].offsetBytes -ne $newRegions[$i].offsetBytes -or $oldRegions[$i].sizeBytes -ne $newRegions[$i].sizeBytes -or [string]$oldRegions[$i]['partitionGuid'] -cne [string]$newRegions[$i]['partitionGuid']) { throw 'Partitions changed during inspection. Refresh disks.' }
    }
    $measuredRegion=@($newRegions | Where-Object { $_.kind -eq 'partition' -and $_.partitionNumber -eq $Query.partitionNumber -and $_.partitionGuid -ceq $Query.partitionGuid })
    if ($measuredRegion.Count -ne 1 -or $measuredRegion[0].resizeState -notin @('checked','failed')) { throw 'Windows did not return the requested resize measurement.' }
    for ($i=0; $i -lt $oldRegions.Count; $i++) {
        if ($oldRegions[$i].kind -eq 'partition' -and $oldRegions[$i].partitionNumber -eq $Query.partitionNumber -and $oldRegions[$i].partitionGuid -ceq $Query.partitionGuid) { $disk[0].regions[$i]=$measuredRegion[0] }
    }
    $disk[0].encryption=$fresh[0].encryption
    $candidate=@($Measured.choices | Where-Object { $_.target.target_kind -eq 'shrink' -and $_.target.partition_number -eq $Query.partitionNumber -and $_.target.partition_guid -ceq $Query.partitionGuid })
    $Inventory.choices=@($Inventory.choices | Where-Object { $_.diskUniqueId -cne $Query.diskUniqueId -or $_.target.target_kind -ne 'shrink' -or $_.target.partition_guid -cne $Query.partitionGuid })+@($candidate)
    return $Inventory
}
function Get-StagingInspection($Release,$Resize=$null) {
    $result=Get-StagingChoices $Release $Resize
    if ($null -ne $Resize) { return $result }
    Emit 'progress' 'inspecting' @{message='Disks are ready. Checking Windows resize limits...';stagedIsoInspection=$result}
    foreach ($disk in @($result.disks)) {
        foreach ($region in @($disk.regions | Where-Object { $_.kind -eq 'partition' -and $_.resizeState -eq 'unchecked' })) {
            $query=[pscustomobject]@{diskNumber=$disk.diskNumber;diskUniqueId=$disk.diskUniqueId;partitionNumber=$region.partitionNumber;partitionGuid=$region.partitionGuid}
            $measured=Get-StagingChoices $Release $query
            $result=Merge-StagingAnalysis $result $measured $query
            Emit 'progress' 'analyzing-resize' @{message=('Checked resize limit for '+$region.label+' on Disk '+$disk.diskNumber+'.');stagedIsoInspection=$result}
        }
    }
    return $result
}
function Get-LargestFreeRegion($Partitions,[long]$DiskSize) {
    $cursor=[long]1MB; $largest=[long]0
    foreach ($p in @($Partitions | Sort-Object Offset)+@([pscustomobject]@{Offset=(Align-Down ($DiskSize-1MB));Size=0})) {
        $end=Align-Down ([long]$p.Offset); $gap=$end-$cursor
        $largest=[Math]::Max($largest,$gap)
        $cursor=[Math]::Max($cursor,(Align-Up ([long]$p.Offset+[long]$p.Size)))
    }
    return $largest
}
function Get-StagingPlan($Request) {
    Assert-Fields $Request @('sourceIsoPath','sourceSha256','sourceLength','diskNumber','diskUniqueId','targetKind','startOffsetBytes','shrinkPartitionNumber','shrinkPartitionGuid') @('sourceIsoPath','sourceSha256','sourceLength','diskNumber','diskUniqueId','targetKind')
    $release=Get-StagingRelease $Request.sourceSha256 $Request.sourceLength
    if ([Omarchy.DirectX86.NativeDisk]::Firmware() -ne 'uefi' -or (Confirm-SecureBootUEFI -ErrorAction Stop)) { throw 'Staging requires x64 UEFI with Secure Boot disabled.' }
    $windows=[Omarchy.DirectX86.NativeDisk]::InspectWindowsBoot()
    $disk=Get-StagingDisk $Request.diskNumber $Request.diskUniqueId
    $bitLocker=@(Get-StagingEncryption $disk.Number)
    $data=Align-Up ([long]$release.sizeBytes+1GB); $total=512MB+$data; $shrink=$null
    $script:minimumBytes=[long]$total
    if ($Request.targetKind -eq 'free') {
        $start=[long]$Request.startOffsetBytes; Assert-StagingFree $disk $start $total
    } elseif ($Request.targetKind -eq 'shrink') {
        $part=Get-Partition -DiskNumber $disk.Number -PartitionNumber $Request.shrinkPartitionNumber -ErrorAction Stop
        if ([guid]$part.Guid -ne [guid]$Request.shrinkPartitionGuid) { throw 'Shrink partition changed.' }
        $candidate=Get-ShrinkCandidate $part
        if (-not $candidate.eligible -or $candidate.maximumAllocationBytes -lt $total) { throw 'Windows cannot release the requested space while retaining its reserve.' }
        $start=(Align-Down ([long]$part.Offset+[long]$part.Size))-$total
        $shrink=[ordered]@{partitionNumber=[int]$part.PartitionNumber;partitionGuid=[string]$part.Guid;volumeId=$candidate.volumeId;offsetBytes=[long]$part.Offset;beforeSizeBytes=[long]$part.Size;afterSizeBytes=$start-[long]$part.Offset;freedOffsetBytes=$start;allocationBytes=$total}
    } else { throw 'Staging supports free space or an explicitly reviewed NTFS shrink; it never deletes existing partitions.' }
    $esp=[guid]::NewGuid().ToString(); $source=[guid]::NewGuid().ToString()
    $projected=@(foreach ($p in @(Get-InspectedPartitions $disk)) {
        $size=if ($null -ne $shrink -and [guid]$p.Guid -eq [guid]$shrink.partitionGuid) { $shrink.afterSizeBytes } else { [long]$p.Size }
        [pscustomobject]@{Offset=[long]$p.Offset;Size=$size}
    })+@([pscustomobject]@{Offset=$start;Size=[long]512MB},[pscustomobject]@{Offset=$start+512MB;Size=$data})
    $largest=Get-LargestFreeRegion $projected $disk.Size
    $minimumGiB=[Math]::Ceiling([decimal]$release.minimumLinuxBytes/1GB)
    # Keep zero-valued Linux fields in schema 1 recovery records. No Linux
    # space is reserved by Windows; the booted installer prepares that space.
    return [ordered]@{schema=1;operationId=[guid]::NewGuid().ToString();status='planned';diskNumber=[int]$disk.Number;diskUniqueId=[string]$disk.UniqueId;diskSizeBytes=[long]$disk.Size;serialNumber=[string]$disk.SerialNumber;logicalSectorBytes=512;layoutSha256=[Omarchy.DirectX86.NativeDisk]::LayoutHash($disk.Number);windowsBoot=$windows;bitLocker=$bitLocker;sourceIsoPath=(Assert-Path $Request.sourceIsoPath);sourceSha256=$Request.sourceSha256;sourceLength=[long]$Request.sourceLength;release=$release;targetKind=$Request.targetKind;shrink=$shrink;linuxOffsetBytes=$start;linuxBytes=[long]0;largestFreeAfterStagingBytes=$largest;partitions=@(@{role='efi';guid=$esp;gptType='{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}';offsetBytes=$start;sizeBytes=[long]512MB},@{role='source';guid=$source;gptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';offsetBytes=$start+512MB;sizeBytes=$data});boot=$null;files=@();message="Restart into the official installer. It will need at least $minimumGiB GiB of free space to install Omarchy; keep the two temporary installer partitions until installation is complete."}
}
function Get-StagingRoot {
    $common=[Environment]::GetFolderPath('CommonApplicationData')
    if (-not $common) { throw 'Windows could not locate ProgramData. The installer helper environment is incomplete.' }
    return Join-Path $common 'OmarchyStagedInstaller'
}
function Get-StagingDirectory([string]$Id) { return Join-Path (Get-StagingRoot) ([guid]::Parse($Id).ToString()) }
function Read-StagingState([string]$Id) {
    $directory=Get-StagingDirectory $Id
    Assert-ProtectedDirectory (Get-StagingRoot); Assert-ProtectedDirectory $directory
    $state=Read-Json (Join-Path $directory 'state.json')
    if ($state.schema -ne 1 -or $state.operationId -cne ([guid]$Id).ToString() -or @($state.partitions).Count -ne 2) { throw 'Invalid staging ownership record.' }
    return $state
}
function Save-StagingState($State) {
    Write-DurableJson (Join-Path (Get-StagingDirectory $State.operationId) 'state.json') $State
    Write-StagingSummary
}
function Write-StagingSummary {
    # This non-authoritative index permits discovery without elevation. Actual
    # operations always re-read the protected ownership record in the helper.
    $status=Get-StagingStatus (ConvertFrom-Json '{}')
    $summary=@{operations=@($status.operations | Where-Object { $_.status -ne 'cleaned' } | ForEach-Object {
        @{operationId=$_.operationId;status=$_.status;diskNumber=$_.diskNumber;temporaryBytes=$_.temporaryBytes}
    });recordErrors=$status.recordErrors}
    $path=Join-Path (Get-StagingRoot) 'summary.json'
    $pending=$path+'.'+[guid]::NewGuid().ToString('N')+'.pending'
    $acl=New-RecoveryFileSecurity
    $users=New-Object Security.Principal.SecurityIdentifier('S-1-5-32-545')
    $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($users,[Security.AccessControl.FileSystemRights]::Read,[Security.AccessControl.AccessControlType]::Allow)))
    $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($summary | ConvertTo-Json -Depth 8))
    try {
        $stream=New-Object IO.FileStream($pending,[IO.FileMode]::CreateNew,[Security.AccessControl.FileSystemRights]::Write,[IO.FileShare]::None,4096,[IO.FileOptions]::WriteThrough,$acl)
        try { $stream.Write($bytes,0,$bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
        if (Test-Path -LiteralPath $path) { [IO.File]::Replace($pending,$path,[NullString]::Value) } else { [IO.File]::Move($pending,$path) }
    } finally { if (Test-Path -LiteralPath $pending) { [IO.File]::Delete($pending) } }
}
function Get-StagingStatus($Request) {
    Assert-Fields $Request @() @()
    $records=@(); $recordErrors=@(); $root=Get-StagingRoot
    if (Test-Path -LiteralPath $root) {
        Assert-ProtectedDirectory $root
        foreach ($directory in @(Get-ChildItem -LiteralPath $root -Directory)) {
            if ($directory.Name -notmatch '^[0-9a-f-]{36}$') { continue }
            try { $records+=Get-StagingSummary (Read-StagingState $directory.Name) }
            catch {
                # An interrupted initial save must not hide unrelated recoverable
                # operations. No mutation may use a missing/invalid ownership record.
                $recordErrors+=@{operationId=$directory.Name;message=$_.Exception.Message}
            }
        }
    }
    return @{operations=$records;recordErrors=$recordErrors}
}
function Get-OwnedStagingPartition($State,$Owned,[bool]$AllowMissing=$false) {
    # Disk numbers may change across a reboot; resolve by the recorded identity.
    $disks=@(Get-Disk | Where-Object { [string]$_.UniqueId -ceq $State.diskUniqueId -and [long]$_.Size -eq $State.diskSizeBytes -and [string]$_.SerialNumber -ceq $State.serialNumber })
    if ($disks.Count -ne 1) { throw 'The recorded staging disk cannot be identified uniquely.' }
    $disk=Get-StagingDisk $disks[0].Number $State.diskUniqueId
    $parts=@(Get-InspectedPartitions $disk | Where-Object { $_.Guid -and [guid]$_.Guid -eq [guid]$Owned.guid })
    if ($parts.Count -eq 0 -and $AllowMissing) { return $null }
    if ($parts.Count -ne 1) { throw 'The owned staging partition is missing or ambiguous.' }
    $p=$parts[0]
    # Windows can classify any ESP as IsSystem. Our recorded ESP is permitted,
    # but never the recorded Windows ESP or the running Windows OS partition.
    if ([long]$p.Offset -ne $Owned.offsetBytes -or [long]$p.Size -ne $Owned.sizeBytes -or [guid]$p.GptType -ne [guid]$Owned.gptType -or $p.IsBoot -or ($p.IsSystem -and $Owned.role -ne 'efi') -or [guid]$p.Guid -eq [guid]$State.windowsBoot.partitionGuid) { throw 'A staging partition changed or belongs to Windows. It will not be modified.' }
    return Get-Partition -DiskNumber $disk.Number -PartitionNumber $p.PartitionNumber -ErrorAction Stop
}
function Get-StagingVolume($Part) {
    $paths=@($Part.AccessPaths | Where-Object { $_ -match '^\\\\\?\\Volume\{[0-9a-fA-F-]{36}\}\\$' })
    if ($paths.Count -ne 1) { throw 'Staging partition has no unique volume GUID path.' }
    # Get-Partition | Get-Volume omits ESPs on Windows. Win32_Volume includes
    # their GUID paths and offers the same documented Format method.
    $volumes=@(Get-CimInstance Win32_Volume -ErrorAction Stop | Where-Object { $_.DeviceID -ieq $paths[0] })
    if ($volumes.Count -ne 1) { throw 'Staging volume could not be identified.' }
    return $volumes[0]
}
function Mount-StagingPartition($State,$Owned) {
    $part=Get-OwnedStagingPartition $State $Owned
    $path=Join-Path (Get-StagingDirectory $State.operationId) ('mount-'+$Owned.role)
    if (Test-Path -LiteralPath $path) {
        if (@($part.AccessPaths) -contains ($path+'\')) { return $path }
        [void](Assert-Path $path $true)
        if (@(Get-ChildItem -LiteralPath $path -Force).Count) { throw 'The staging mount directory is occupied.' }
    } else { [void][IO.Directory]::CreateDirectory($path) }
    $volume=Get-StagingVolume $part
    try { [Omarchy.DirectX86.NativeDisk]::MountStagingVolume($path,$volume.DeviceID) } catch { if ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw }; [IO.Directory]::Delete($path); throw }
    return $path
}
function Dismount-StagingPartition($State,$Owned,[string]$Path) {
    $part=Get-OwnedStagingPartition $State $Owned
    [Omarchy.DirectX86.NativeDisk]::UnmountStagingVolume($Path,(Get-StagingVolume $part).DeviceID)
    [IO.Directory]::Delete($Path)
}
function Get-IsoInventory([string]$Root) {
    $items=New-Object 'Collections.Generic.List[object]'
    $seen=New-Object 'Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    foreach ($item in @(Get-ChildItem -LiteralPath $Root -Force -Recurse -ErrorAction Stop)) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'ISO contains a link.' }
        if ($item.PSIsContainer) { continue }
        $relative=$item.FullName.Substring($Root.TrimEnd('\').Length+1).Replace('\','/')
        Assert-StagingRelativePath $relative
        if (-not $seen.Add($relative) -or $items.Count -ge 10000) { throw 'Duplicate or excessive ISO files.' }
        $items.Add(@{path=$relative;sizeBytes=[long]$item.Length;sha256=(Sha $item.FullName)})
    }
    return $items.ToArray()
}
function Get-StagingConfig($State) {
    $guid=([guid]$State.partitions[1].guid).ToString()
    $kernel=$State.release.kernelPath; $initrd=$State.release.initrdPath
    Assert-StagingRelativePath $kernel; Assert-StagingRelativePath $initrd
    # Find only this operation's marker; the kernel uses its exact GPT GUID.
    return "set timeout=5`nmenuentry 'Start official Omarchy installer' {`n  search --no-floppy --file --set=root /omarchy-stage-$guid`n  linux /$kernel archisobasedir=arch archisodevice=/dev/disk/by-partuuid/$guid copytoram=n checksum=y quiet splash xe.enable_panel_replay=0 initramfs_async=0`n  initrd /$initrd`n}`n"
}
function Copy-StagedFile([string]$Source,[string]$Destination,[string]$ExpectedHash) {
    if (Test-Path -LiteralPath $Destination) { throw 'Refusing to overwrite a staged file.' }
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Destination))
    $input=[IO.File]::OpenRead($Source)
    try {
        $output=New-Object IO.FileStream($Destination,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None,1048576,[IO.FileOptions]::WriteThrough)
        try { $input.CopyTo($output,1048576); $output.Flush($true) } finally { $output.Dispose() }
    } finally { $input.Dispose() }
    if ((Sha $Destination) -cne $ExpectedHash) { throw 'Staged file readback failed.' }
}
function Write-StagedText([string]$Path,[string]$Text) {
    $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes($Text)
    $stream=New-Object IO.FileStream($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None,4096,[IO.FileOptions]::WriteThrough)
    try { $stream.Write($bytes,0,$bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
    if ([IO.File]::ReadAllText($Path) -cne $Text) { throw 'Staged boot metadata readback failed.' }
}
function Write-StagingBootFiles($State,[string]$Mount,[string]$Loader) {
    # Keep incomplete writes outside EFI. Publish only verified artifacts by
    # same-volume, non-overwriting rename; cleanup still rejects replaced final
    # files regardless of the journal status. Leftover scratch files are harmless.
    $scratch=Join-Path $Mount ('.omarchy-stage-'+([guid]$State.operationId).ToString())
    [void][IO.Directory]::CreateDirectory($scratch)
    $pendingLoader=Join-Path $scratch 'BOOTX64.EFI'
    $pendingConfig=Join-Path $scratch 'grub.cfg'
    Copy-StagedFile $Loader $pendingLoader $State.loaderSha256
    Write-StagedText $pendingConfig $State.configText
    $boot=Join-Path $Mount 'EFI/BOOT'; $menu=Join-Path $Mount 'EFI/Omarchy'
    [void][IO.Directory]::CreateDirectory($boot)
    [void][IO.Directory]::CreateDirectory($menu)
    [IO.File]::Move($pendingLoader,(Join-Path $boot 'BOOTX64.EFI'))
    [IO.File]::Move($pendingConfig,(Join-Path $menu 'grub.cfg'))
    [IO.Directory]::Delete($scratch)
}
function Invoke-StagingWrite($Plan,[uint32]$AuthenticatedPid) {
    [void](Get-StagingRelease $Plan.sourceSha256 $Plan.sourceLength)
    Assert-PhysicalDiskIdentity $Plan
    Assert-BitLockerUnchanged @($Plan.bitLocker) @(Get-StagingEncryption $Plan.diskNumber)
    $current=[Omarchy.DirectX86.NativeDisk]::InspectWindowsBoot()
    if ($current.entrySha256 -cne $Plan.windowsBoot.entrySha256 -or $current.bootOrderSha256 -cne $Plan.windowsBoot.bootOrderSha256) { throw 'Windows boot configuration changed after review.' }
    $root=Get-StagingRoot; New-ProtectedRecoveryDirectory $root
    $directory=Get-StagingDirectory $Plan.operationId
    if (Test-Path -LiteralPath $directory) { throw 'This staging operation already started; inspect its recovery record.' }
    New-ProtectedRecoveryDirectory $directory
    $Plan | Add-Member -NotePropertyName loaderSha256 -NotePropertyValue (Sha (Join-Path $PSScriptRoot '../usb-preserve/boot/BOOTX64.EFI'))
    $Plan | Add-Member -NotePropertyName configText -NotePropertyValue ''
    $Plan.status='preparing'; Save-StagingState $Plan
    $lease=New-Object Omarchy.DirectX86.NativeSource($Plan.sourceIsoPath,[long]$Plan.sourceLength)
    $mounted=$false
    try {
        Emit 'progress' 'verifying-source' @{message='Verifying the qualified official ISO before disk changes...'}
        if ((Sha $Plan.sourceIsoPath) -cne $Plan.sourceSha256) { throw 'Official ISO changed after verification.' }
        $image=Get-DiskImage -ImagePath $Plan.sourceIsoPath -ErrorAction Stop
        if ($image.Attached) { throw 'Unmount this ISO before staging; the installer only removes mounts it creates.' }
        $image=Mount-DiskImage -ImagePath $Plan.sourceIsoPath -Access ReadOnly -PassThru -ErrorAction Stop; $mounted=$true
        $volumes=@($image | Get-Volume -ErrorAction Stop)
        if ($volumes.Count -ne 1 -or -not $volumes[0].DriveLetter) { throw 'Mounted ISO has no unique readable volume.' }
        $isoRoot=[string]$volumes[0].DriveLetter+':\'
        Emit 'progress' 'checking-files' @{message='Checking installer files and staging capacity...'}
        $files=@(Get-IsoInventory $isoRoot)
        if (-not $Plan.release.bootQualified) {
            Set-TestingBootPaths $Plan.release $files
        }
        $Plan.configText=Get-StagingConfig $Plan
        foreach ($needed in @($Plan.release.kernelPath,$Plan.release.initrdPath,'arch/x86_64/airootfs.sfs','arch/x86_64/airootfs.sha512')) {
            if (@($files | Where-Object { $_.path -ceq $needed }).Count -ne 1) { throw "Required official ISO file is missing: $needed" }
        }
        if (($files | Measure-Object sizeBytes -Sum).Sum -gt $Plan.partitions[1].sizeBytes-512MB) { throw 'Extracted ISO exceeds the planned staging capacity.' }
        $Plan.files=$files; $Plan.status='allocation-starting'; Save-StagingState $Plan
        Assert-PhysicalDiskIdentity $Plan
        if (Confirm-SecureBootUEFI -ErrorAction Stop) { throw 'Secure Boot changed after review.' }
        Start-StagingSuspension $Plan $AuthenticatedPid
        $layout=$Plan.layoutSha256
        Emit 'progress' 'allocating-staging' @{message='Preparing the reviewed free space and temporary installer partitions...'}
        if ($null -ne $Plan.shrink) { $layout=Invoke-ConfirmedShrink $Plan $directory }
        Assert-StagingFree (Get-StagingDisk $Plan.diskNumber $Plan.diskUniqueId) $Plan.linuxOffsetBytes ($Plan.linuxBytes+512MB+$Plan.partitions[1].sizeBytes)
        [void][Omarchy.DirectX86.NativeDisk]::AllocateStaging($Plan.diskNumber,$layout,$Plan.partitions[0].offsetBytes,$Plan.partitions[1].sizeBytes,[guid]$Plan.partitions[0].guid,[guid]$Plan.partitions[1].guid)
        $Plan.status='copying'; Save-StagingState $Plan
        foreach ($owned in $Plan.partitions) {
            $part=Get-OwnedStagingPartition $Plan $owned
            $volume=Get-StagingVolume $part
            if ([string]$volume.FileSystem -notin @('','Unknown','RAW')) { throw 'New staging partition is not an empty raw volume.' }
            $fs=if ($owned.role -eq 'efi') { 'FAT32' } else { 'NTFS' }
            $formatted=Invoke-CimMethod -InputObject $volume -MethodName Format -Arguments @{FileSystem=$fs;QuickFormat=$true;Label='OMARCHY_TMP'} -ErrorAction Stop
            if ($formatted.ReturnValue -ne 0 -or (Get-StagingVolume (Get-OwnedStagingPartition $Plan $owned)).FileSystem -cne $fs) { throw 'Staging filesystem format failed or did not verify.' }
            $mount=Mount-StagingPartition $Plan $owned
            try {
                if ($owned.role -eq 'source') {
                    $copied=[long]0; $total=[long]($files | Measure-Object sizeBytes -Sum).Sum
                    foreach ($file in $files) {
                        Copy-StagedFile (Join-Path $isoRoot $file.path) (Join-Path $mount $file.path) $file.sha256
                        $copied+=$file.sizeBytes
                        Emit 'progress' 'copying-installer' @{message='Copying and verifying the official installer...';completedBytes=$copied;totalBytes=$total}
                    }
                    Write-StagedText (Join-Path $mount ('omarchy-stage-'+$owned.guid)) $Plan.operationId
                } else {
                    $loader=Join-Path $PSScriptRoot '../usb-preserve/boot/BOOTX64.EFI'
                    Write-StagingBootFiles $Plan $mount $loader
                }
            } finally { Dismount-StagingPartition $Plan $owned $mount }
        }
        Assert-StagingEncryption $Plan.diskNumber
        $efi=Get-OwnedStagingPartition $Plan $Plan.partitions[0]
        $option=[Omarchy.DirectX86.NativeDisk]::StagingBootOption($efi.PartitionNumber,$efi.Offset,[guid]$efi.Guid)
        $Plan.boot=@{name=[Omarchy.DirectX86.NativeDisk]::ReserveStagingBootName();option=[Convert]::ToBase64String($option)}
        $Plan.status='registering-boot'; Save-StagingState $Plan
        [Omarchy.DirectX86.NativeDisk]::RegisterStagingBoot($Plan.boot.name,$option,$Plan.windowsBoot.bootOrderSha256)
        $Plan.status='staged'; Save-StagingState $Plan
        return Get-StagingSummary $Plan
    } finally { try { if ($mounted) { Dismount-DiskImage -ImagePath $Plan.sourceIsoPath -ErrorAction Stop } } finally { $lease.Dispose() } }
}
function Get-StagingSummary($State) {
    return @{operationId=$State.operationId;status=$State.status;diskUniqueId=$State.diskUniqueId;diskNumber=$State.diskNumber;linuxOffsetBytes=$State.linuxOffsetBytes;linuxBytes=$State.linuxBytes;message=$State.message;temporaryBytes=512MB+$State.partitions[1].sizeBytes}
}
function Set-TestingBootPaths($Release,$Files) {
    if (-not $script:testingBuild) { throw 'Unqualified ISO staging requires a testing build.' }
    $pairs=@(foreach ($file in $Files) {
        if ($file.path -cmatch '^arch/boot/x86_64/vmlinuz-([a-zA-Z0-9._-]+)$') {
            $initrd='arch/boot/x86_64/initramfs-'+$Matches[1]+'.img'
            if (@($Files | Where-Object { $_.path -ceq $initrd }).Count -eq 1) { @{kernel=$file.path;initrd=$initrd} }
        }
    })
    if ($pairs.Count -ne 1) { throw 'The official ISO must contain one unambiguous kernel/initramfs pair for this testing build.' }
    $Release.kernelPath=$pairs[0].kernel; $Release.initrdPath=$pairs[0].initrd
}
function Invoke-StagingArm($State) {
    if ($State.status -notin @('staged','arming','boot-scheduled')) { throw 'Only a completely staged installer can be selected for startup.' }
    [void](Get-StagingRelease $State.sourceSha256 $State.sourceLength)
    foreach ($owned in $State.partitions) { [void](Get-OwnedStagingPartition $State $owned) }
    $disk=(Get-OwnedStagingPartition $State $State.partitions[0]).DiskNumber
    Assert-StagingEncryption $disk
    [void](Get-StagingDisk $disk $State.diskUniqueId)
    if (Confirm-SecureBootUEFI -ErrorAction Stop) { throw 'Secure Boot must be disabled before starting this installer.' }
    $windows=[Omarchy.DirectX86.NativeDisk]::InspectWindowsBoot()
    if ($windows.entrySha256 -cne $State.windowsBoot.entrySha256) { throw 'Windows Boot Manager changed since staging.' }
    $efi=Get-OwnedStagingPartition $State $State.partitions[0]
    if ([Convert]::ToBase64String([Omarchy.DirectX86.NativeDisk]::StagingBootOption($efi.PartitionNumber,$efi.Offset,[guid]$efi.Guid)) -cne $State.boot.option) { throw 'The temporary firmware target no longer matches its partition.' }
    foreach ($owned in $State.partitions) {
        Emit 'progress' 'verifying-staging' @{message='Rechecking staged files before selecting the next startup...'}
        $mount=Mount-StagingPartition $State $owned
        try {
            if ($owned.role -eq 'source') {
                foreach ($file in $State.files) {
                    Assert-StagingRelativePath $file.path
                    $path=Join-Path $mount $file.path
                    if ((Get-Item -LiteralPath $path).Length -ne $file.sizeBytes -or (Sha $path) -cne $file.sha256) { throw 'Installer files changed since staging.' }
                }
                if ([IO.File]::ReadAllText((Join-Path $mount ('omarchy-stage-'+$owned.guid))) -cne $State.operationId) { throw 'Staging source marker changed.' }
            } else {
                if ((Sha (Join-Path $mount 'EFI/BOOT/BOOTX64.EFI')) -cne $State.loaderSha256 -or [IO.File]::ReadAllText((Join-Path $mount 'EFI/Omarchy/grub.cfg')) -cne $State.configText) { throw 'Temporary EFI boot files changed.' }
            }
        } finally { Dismount-StagingPartition $State $owned $mount }
    }
    $State.status='arming'; Save-StagingState $State
    [Omarchy.DirectX86.NativeDisk]::ArmStagingBoot($State.boot.name,[Convert]::FromBase64String($State.boot.option))
    $State.status='boot-scheduled'; Save-StagingState $State
    return @{operationId=$State.operationId;status=$State.status;message='Restart Windows when ready. The next startup selects the official installer once; the existing boot order is unchanged. Returning to Windows does not prove Linux installation completed.'}
}
function Invoke-StagingCleanup($State) {
    if ($State.status -eq 'cleaned') { return Get-StagingSummary $State }
    # Validate BOTH partitions before changing either. Missing owned partitions
    # are acceptable on an interrupted retry; replaced GUIDs are never deleted.
    foreach ($owned in $State.partitions) { [void](Get-OwnedStagingPartition $State $owned $true) }
    Assert-StagingCleanupBootFiles $State
    if ($null -ne $State.boot) { Remove-StagingBoot $State.boot }
    $State.status='cleaning'; Save-StagingState $State
    foreach ($owned in @($State.partitions[1],$State.partitions[0])) {
        $part=Get-OwnedStagingPartition $State $owned $true
        if ($null -eq $part) { continue }
        $mount=Join-Path (Get-StagingDirectory $State.operationId) ('mount-'+$owned.role)
        if (@($part.AccessPaths) -contains ($mount+'\')) {
            [Omarchy.DirectX86.NativeDisk]::UnmountStagingVolume($mount,(Get-StagingVolume $part).DeviceID)
            [IO.Directory]::Delete($mount)
        }
        $before=Read-StagingLayout $part.DiskNumber
        Remove-OwnedStagingPartition $part $owned $before
        Confirm-StagingDeletion $before $part.DiskNumber $owned
    }
    $State.status='cleaned'; $State.message='Temporary installer partitions and its firmware entry were removed. The reclaimed space remains unallocated; no Windows or Linux filesystem was resized.'; Save-StagingState $State
    return Get-StagingSummary $State
}
function Read-StagingLayout([int]$Disk) { return ,[Omarchy.DirectX86.NativeDisk]::ReadLayout($Disk) }
function Remove-OwnedStagingPartition($Part,$Owned,[byte[]]$Before) {
    $volume=Get-StagingVolume $Part
    $raw=[string]$volume.FileSystem -in @('','Unknown','RAW')
    [Omarchy.DirectX86.NativeDisk]::DeleteStagingPartition($Part.DiskNumber,$Part.PartitionNumber,[guid]$Owned.guid,$Owned.offsetBytes,$Owned.sizeBytes,[guid]$Owned.gptType,$volume.DeviceID,[Omarchy.DirectX86.NativeDisk]::Hash($Before),$raw)
}
function Confirm-StagingDeletion([byte[]]$Before,[int]$Disk,$Owned) {
    [void][Omarchy.DirectX86.NativeDisk]::VerifyOnlyDelete($Before,[Omarchy.DirectX86.NativeDisk]::ReadLayout($Disk),[guid]$Owned.guid,$Owned.offsetBytes,$Owned.sizeBytes)
}
function Remove-StagingBoot($Boot) { [Omarchy.DirectX86.NativeDisk]::RemoveStagingBoot($Boot.name,[Convert]::FromBase64String($Boot.option)) }
function Assert-StagingCleanupBootFiles($State) {
    $owned=$State.partitions[0]; $part=Get-OwnedStagingPartition $State $owned $true
    if ($null -eq $part) { return }
    $volume=Get-StagingVolume $part
    if ([string]$volume.FileSystem -in @('','RAW','Unknown')) { return }
    if ([string]$volume.FileSystem -ne 'FAT32') { throw 'The temporary EFI partition has been repurposed.' }
    $mount=Mount-StagingPartition $State $owned
    try {
        $efi=Join-Path $mount 'EFI'
        if (Test-Path -LiteralPath $efi) {
            foreach ($file in @(Get-ChildItem -LiteralPath $efi -Recurse -Force -File)) {
                $relative=$file.FullName.Substring($mount.Length+1).Replace('\','/')
                if ($relative -notin @('EFI/BOOT/BOOTX64.EFI','EFI/Omarchy/grub.cfg')) { throw 'Additional EFI files may belong to an installed OS. Automatic staging cleanup is blocked.' }
                if ($relative -ieq 'EFI/BOOT/BOOTX64.EFI' -and (Sha $file.FullName) -cne $State.loaderSha256) { throw 'The temporary loader was replaced; it may now boot an installed OS.' }
                if ($relative -ieq 'EFI/Omarchy/grub.cfg' -and [IO.File]::ReadAllText($file.FullName) -cne $State.configText) { throw 'The temporary boot menu was changed. Inspect it before cleanup.' }
            }
        }
    } finally { Dismount-StagingPartition $State $owned $mount }
}
