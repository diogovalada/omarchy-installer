# Pure policies, disposable file copies, and mocked cleanup. No host disk calls.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$script:testingBuild=$false
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
foreach ($file in @('providers/staged-iso/Invoke-StagedIso.ps1','providers/staged-iso/Staging.ps1','providers/direct-x86/StoragePlan.ps1','providers/direct-x86/BitLocker.ps1')) {
    $tokens=$null; $errors=$null
    $ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $root $file),[ref]$tokens,[ref]$errors)
    if ($errors.Count) { throw ($errors | Out-String) }
    foreach ($definition in $ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$false)) { . ([scriptblock]::Create($definition.Extent.Text)) }
}
function Assert($Value,[string]$Message) { if (-not $Value) { throw $Message } }
& {
    # Staging needs suspension, not the retained image path's fixed TPM profile.
    $volume=[pscustomobject]@{DeviceID='volume';PersistentVolumeID='persistent';DriveLetter='C:'}
    function Get-CimInstance {
        if ($args -contains 'Win32_OperatingSystem') { return @{SystemDrive='C:'} }
        return $volume
    }
    function Get-Partition { return [pscustomobject]@{AccessPaths=@('volume');DiskNumber=0;PartitionNumber=3;Guid='22222222-2222-2222-2222-222222222222';IsBoot=$true;IsSystem=$false} }
    function Get-Disk { return @{UniqueId='disk'} }
    function Invoke-BitLockerQuery($Volume,$Method,$Arguments) {
        switch ($Method) {
            'GetConversionStatus' { return @{ConversionStatus=1} }
            'GetProtectionStatus' { return @{ProtectionStatus=1} }
            'GetLockStatus' { return @{LockStatus=0} }
            'GetKeyProtectors' { return @{VolumeKeyProtectorID=@('11111111-1111-1111-1111-111111111111')} }
            default { throw 'Unexpected query' }
        }
    }
    function Get-KeyProtectorDetails { throw 'Unsupported legacy PCR policy' }
    $staging=Get-BitLockerSnapshot -ForStaging
    Assert ($staging.known -and $staging.volumes[0].supported) 'Staging inherited the legacy PCR restriction.'
    $legacy=Get-BitLockerSnapshot
    Assert (-not $legacy.volumes[0].supported) 'Retained image deployment lost its original protector checks.'
}
. (Join-Path $root 'providers/staged-iso/Staging.ps1')
function Reject($Work,[string]$Message) { $failed=$false; try { & $Work | Out-Null } catch { $failed=$true }; Assert $failed $Message }
Assert-Fields (ConvertFrom-Json '{}') @() @()
Reject { Assert-Fields (ConvertFrom-Json '{"unexpected":true}') @() @() } 'Status accepted unknown fields.'
Reject { Assert-Fields (ConvertFrom-Json '{}') @('operationId') @('operationId') } 'Required fields were not enforced.'
& {
    $recordsRoot=Join-Path $env:TEMP ('omarchy-staged-record-test-'+[guid]::NewGuid().ToString('N'))
    function Get-StagingRoot { return $recordsRoot }
    # Only ACLs are mocked: exercise actual JSON loading and ownership validation
    # against disposable files, including the same {} request used by the helper.
    function Assert-ProtectedDirectory($Path) { }
    function New-RecoveryFileSecurity {
        # Use the test account for disposable fixtures, never request elevation.
        $acl=New-Object Security.AccessControl.FileSecurity
        $acl.SetAccessRuleProtection($true,$false)
        $user=[Security.Principal.WindowsIdentity]::GetCurrent().User
        $acl.SetOwner($user)
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($user,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow)))
        return $acl
    }
    $empty=Get-StagingStatus (ConvertFrom-Json '{}')
    Assert ($empty.operations.Count -eq 0 -and $empty.recordErrors.Count -eq 0) 'Empty status failed.'
    $valid=[guid]::NewGuid().ToString(); $missing=[guid]::NewGuid().ToString(); $invalid=[guid]::NewGuid().ToString()
    try {
        foreach ($id in @($valid,$missing,$invalid)) { [void][IO.Directory]::CreateDirectory((Join-Path $recordsRoot $id)) }
        $record=@{schema=1;operationId=$valid;status='copying';diskUniqueId='fixture';diskNumber=0;linuxOffsetBytes=1MB;linuxBytes=40GB;message='Interrupted';partitions=@(@{},@{sizeBytes=8GB})}
        [IO.File]::WriteAllText((Join-Path $recordsRoot "$valid/state.json"),($record | ConvertTo-Json -Depth 5))
        [IO.File]::WriteAllText((Join-Path $recordsRoot "$invalid/state.json"),'{')
        $status=Get-StagingStatus (ConvertFrom-Json '{}')
        Assert ($status.operations.Count -eq 1 -and $status.operations[0].operationId -eq $valid) 'An unreadable record hid a valid recovery operation.'
        Assert ($status.recordErrors.Count -eq 2 -and $missing -in $status.recordErrors.operationId -and $invalid -in $status.recordErrors.operationId) 'Unreadable records were not reported separately.'
        Reject { Read-StagingState $missing } 'Missing ownership record became usable for mutation.'
        Reject { Read-StagingState $invalid } 'Invalid ownership record became usable for mutation.'
        Write-StagingSummary
        $index=Read-Json (Join-Path $recordsRoot 'summary.json')
        Assert ($index.operations.Count -eq 1 -and $index.recordErrors.Count -eq 2) 'Passive summary omitted pending work.'
        $acl=Get-Acl -LiteralPath (Join-Path $recordsRoot 'summary.json')
        $readers=@($acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]) | Where-Object { $_.IdentityReference.Value -eq 'S-1-5-32-545' })
        Assert ($readers.Count -eq 1 -and ($readers[0].FileSystemRights -band [Security.AccessControl.FileSystemRights]::ReadData) -ne 0 -and ($readers[0].FileSystemRights -band [Security.AccessControl.FileSystemRights]::WriteData) -eq 0) 'Discovery summary must grant users read access only.'
        $record.status='cleaned'
        Save-StagingState ([pscustomobject]$record)
        Assert ((Read-StagingState $valid).status -eq 'cleaned') 'Repeated ownership record saves failed.'
        Assert ((Read-Json (Join-Path $recordsRoot 'summary.json')).operations.Count -eq 0) 'Completed cleanup remained visible as a pending operation.'
    } finally {
        $indexPath=Join-Path $recordsRoot 'summary.json'; if (Test-Path -LiteralPath $indexPath) { [IO.File]::Delete($indexPath) }
        foreach ($id in @($valid,$missing,$invalid)) {
            $dir=Join-Path $recordsRoot $id; $path=Join-Path $dir 'state.json'
            if (Test-Path -LiteralPath $path) { [IO.File]::Delete($path) }
            if (Test-Path -LiteralPath $dir) { [IO.Directory]::Delete($dir) }
        }
        if (Test-Path -LiteralPath $recordsRoot) { [IO.Directory]::Delete($recordsRoot) }
    }
}
foreach ($path in @('../escape','a/../b','/absolute','a\b','a:stream','CON','folder/LPT1.txt','trailing.','double//slash')) { Reject { Assert-StagingRelativePath $path } "Unsafe filename accepted: $path" }
Assert-StagingRelativePath 'arch/x86_64/airootfs.sfs'
$layout=@([pscustomobject]@{Offset=1MB;Size=100GB-1MB},[pscustomobject]@{Offset=200GB;Size=20GB},[pscustomobject]@{Offset=250GB;Size=50GB-1MB})
Assert ((Get-LargestFreeRegion $layout 300GB) -eq 100GB) 'Largest remaining free region was miscalculated.'
$competing=@([pscustomobject]@{Offset=1MB;Size=100GB-1MB},[pscustomobject]@{Offset=140GB;Size=10GB})
Assert ((Get-LargestFreeRegion $competing 300GB) -eq 150GB-1MB) 'A later free region was missed.'
function Read-Json($Path) { return [pscustomobject]@{schema=1;releases=@()} }
Reject { Get-StagingRelease ('a'*64) 6GB } 'An unqualified ISO bypassed the release gate.'
$script:testingBuild=$true
$testingRelease=Get-StagingRelease ('a'*64) 6GB
Assert (-not $testingRelease.bootQualified -and $testingRelease.sourceProtection -eq 'unqualified') 'Testing mode falsely claims upstream qualification.'
Reject { Get-StagingRelease 'bad-hash' 6GB } 'Testing mode bypassed ISO identity validation.'
$bootFiles=@([pscustomobject]@{path='arch/boot/x86_64/vmlinuz-linux-t2'},[pscustomobject]@{path='arch/boot/x86_64/initramfs-linux-t2.img'})
Set-TestingBootPaths $testingRelease $bootFiles
Assert ($testingRelease.kernelPath -eq $bootFiles[0].path) 'Official boot filenames were not discovered.'
Reject { Set-TestingBootPaths $testingRelease @($bootFiles[0]) } 'Missing initramfs was accepted.'
$ambiguous=$bootFiles+@([pscustomobject]@{path='arch/boot/x86_64/vmlinuz-linux'},[pscustomobject]@{path='arch/boot/x86_64/initramfs-linux.img'})
Reject { Set-TestingBootPaths $testingRelease $ambiguous } 'Ambiguous kernels were accepted.'
$script:testingBuild=$false
Reject { Set-TestingBootPaths $testingRelease $bootFiles } 'Normal builds discovered unqualified boot files.'
$script:snapshot=[pscustomobject]@{known=$true;volumes=@([pscustomobject]@{conversionStatus=0;protectionStatus=0;lockStatus=0})}
function Get-BitLockerSnapshot { return $script:snapshot }
function Get-AffectedBitLocker($Snapshot,$Disk) { return $Snapshot.volumes }
Assert-StagingEncryption 0
 $script:snapshot.volumes[0].conversionStatus=1; Assert-StagingEncryption 0
$script:snapshot.volumes[0].protectionStatus=1
Reject { Assert-StagingEncryption 0 } 'Active protection must be suspended before writing.'
[void](Get-StagingEncryption 0)
$script:snapshot.volumes[0].protectionStatus=0
foreach ($conversion in @(2,3,4,5)) { $script:snapshot.volumes[0].conversionStatus=$conversion; Reject { Assert-StagingEncryption 0 } 'Encrypted or in-progress volumes must be rejected.' }

# Disk discovery exposes encrypted destinations without changing protection.
& {
    $disk=[pscustomobject]@{Number=0;UniqueId='windows-disk';Size=[long]500GB}
    $volume=[pscustomobject]@{diskNumber=0;isOsVolume=$true;isBootVolume=$true;driveLetter='C:';conversionStatus=1;protectionStatus=1;lockStatus=0}
    function Get-Disk { return $disk }
    function Get-StagingDisk { return $disk }
    function Get-InspectedPartitions { return @() }
    function Get-BitLockerSnapshot { return @{known=$true;volumes=@($volume)} }
    $release=[pscustomobject]@{sizeBytes=6GB;minimumLinuxBytes=40GB}
    $found=Get-StagingChoices $release
    Assert ($found.choices.Count -eq 1 -and $found.blocked.Count -eq 0) 'Encryption must not block space selection.'
    Assert ($found.choices[0].encryption[0].protectionStatus -eq 1) 'Review lost the active protection state.'
    $volume.conversionStatus=3; $volume.protectionStatus=0
    $found=Get-StagingChoices $release
    Assert ($found.choices.Count -eq 1) 'Disk inspection must remain available during conversion.'
    Reject { Get-StagingEncryption 0 } 'Preparation must reject incomplete conversion.'
    $volume.conversionStatus=1; $volume.protectionStatus=0
    Assert-StagingEncryption 0
    $disk.Size=20GB
    $found=Get-StagingChoices $release
    Assert ($found.choices.Count -eq 1 -and $found.blocked.Count -eq 0 -and $found.disks.Count -eq 1) 'A disk fitting the temporary installer must remain selectable.'
    Assert ($found.disks[0].unallocatedBytes -eq 20GB-2MB) 'Small unallocated region lost its size.'
    Assert ($found.choices[0].largestFreeAfterStagingBytes -lt $release.minimumLinuxBytes) 'Small disk did not warn about later installation space.'
}

# Full disks remain browsable; slow Windows shrink analysis is explicit and
# bound to the exact selected disk/partition. All OS calls here are mocked.
& {
    $disk=[pscustomobject]@{Number=0;UniqueId='small-data';Size=[long](22GB+2MB)}
    $part=[pscustomobject]@{DiskNumber=0;PartitionNumber=5;Guid='55555555-5555-5555-5555-555555555555';Offset=[long]1MB;Size=[long]22GB;GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}'}
    function Emit { }
    function Get-Disk { return $disk }
    function Get-StagingDisk { return $disk }
    function Get-InspectedPartitions { return @($part) }
    function Get-BitLockerSnapshot { return @{known=$true;volumes=@()} }
    function Get-Volume { return [pscustomobject]@{Path='restore';DriveLetter='';FileSystemLabel='RESTORE';FileSystemType='NTFS';Size=[long]22GB;SizeRemaining=[long](8.6GB)} }
    function Get-PartitionSupportedSize { throw 'A slow resize query should not run.' }
    $release=[pscustomobject]@{sizeBytes=6GB;minimumLinuxBytes=32GB}
    $found=Get-StagingInspection $release
    Assert ($found.disks[0].regions[0].resizeState -eq 'insufficient' -and $found.choices.Count -eq 0) 'A small data partition bypassed the reserve precheck.'
}
& {
    $disk=[pscustomobject]@{Number=0;UniqueId='disk';Size=[long]200GB}
    $part=[pscustomobject]@{DiskNumber=0;PartitionNumber=3;Guid='33333333-3333-3333-3333-333333333333';Offset=[long]1MB;Size=[long](200GB-2MB);GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';IsReadOnly=$false}
    $script:shrinkCalls=0; $script:shrinkLimit=[long]120GB
    $script:minimumBytes=[long]40GB
    function Emit { }
    function Get-Disk { return $disk }
    function Get-StagingDisk($Number,$Identity) { if ($Number -ne 0 -or $Identity -cne 'disk') { throw 'Changed disk' }; return $disk }
    function Get-InspectedPartitions { return @($part) }
    function Get-BitLockerSnapshot { return @{known=$true;volumes=@()} }
    $script:volumeLetter='C'; $script:volumeLabel=''
    function Get-Volume { return [pscustomobject]@{Path='volume';DriveLetter=$script:volumeLetter;FileSystemLabel=$script:volumeLabel;FileSystemType='NTFS';Size=$part.Size;SizeRemaining=[long]100GB;HealthStatus='Healthy';OperationalStatus=@('OK')} }
    function Get-PartitionSupportedSize { $script:shrinkCalls++; return @{SizeMin=$script:shrinkLimit} }
    $release=[pscustomobject]@{sizeBytes=6GB;minimumLinuxBytes=40GB}
    $found=Get-StagingChoices $release
    Assert ($script:shrinkCalls -eq 0) 'Disk listing must not trigger shrink analysis.'
    Assert ($found.disks.Count -eq 1 -and $found.choices.Count -eq 0 -and $found.blocked.Count -eq 0) 'A full disk was hidden or blocked.'
    $region=$found.disks[0].regions[0]
    Assert ($region.freeBytes -eq 100GB -and $region.resizeState -eq 'unchecked') 'Unused filesystem space must remain distinct from unallocated space.'
    Assert ($found.disks[0].unallocatedBytes -eq 0) 'Filesystem free bytes became unallocated disk space.'
    $script:volumeLetter=''; $script:volumeLabel='RESTORE'
    $labeled=Get-StagingChoices $release
    Assert ($labeled.disks[0].regions[0].label -eq 'RESTORE (Partition 3)') 'A volume label without a drive letter was hidden.'
    $script:volumeLetter='C'; $script:volumeLabel='OS'
    $labeled=Get-StagingChoices $release
    Assert ($labeled.disks[0].regions[0].label -eq 'OS (C:)') 'The drive letter and filesystem label were not shown together.'
    $script:volumeLabel=''
    $query=[pscustomobject]@{diskNumber=0;diskUniqueId='disk';partitionNumber=3;partitionGuid=$part.Guid}
    $found=Get-StagingChoices $release $query
    Assert ($script:shrinkCalls -eq 1 -and $found.choices.Count -eq 1) 'Selected resize analysis did not expose a usable allocation.'
    Assert ($found.choices[0].target.partition_guid -eq $part.Guid -and $found.choices[0].largestFreeAfterStagingBytes -eq 0) 'Temporary-only shrink lost identity or invented Linux space.'
    $script:shrinkLimit=195GB
    $found=Get-StagingChoices $release $query
    Assert ($found.choices.Count -eq 0 -and $found.disks[0].regions[0].resizeState -eq 'checked' -and $found.disks[0].regions[0].maximumReleaseBytes -eq 5GB-2MB) 'Insufficient shrink space must retain the measured limit and partition.'
    $query.partitionGuid='44444444-4444-4444-4444-444444444444'
    Reject { Get-StagingChoices $release $query } 'Resize query accepted a replaced partition.'
    Assert ($script:shrinkCalls -eq 2) 'Changed partition reached shrink analysis.'
}

# One privileged inspection emits the inventory before running the expensive
# Windows query, then publishes each result without opening another helper.
& {
    $script:streamDisk=[pscustomobject]@{Number=0;UniqueId='stream-disk';Size=[long]400GB}
    $partitions=@(
        [pscustomobject]@{DiskNumber=0;PartitionNumber=3;Guid='33333333-3333-3333-3333-333333333333';Offset=[long]1MB;Size=[long]180GB;GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';IsReadOnly=$false},
        [pscustomobject]@{DiskNumber=0;PartitionNumber=4;Guid='44444444-4444-4444-4444-444444444444';Offset=[long]181GB;Size=[long]180GB;GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';IsReadOnly=$false}
    )
    $script:minimumBytes=[long]40GB; $script:shrinkCalls=0; $script:partial=@(); $script:streamFree=[long]5GB
    function Emit($Type,$Stage,$Fields) {
        if ($Fields.ContainsKey('stagedIsoInspection')) { $script:partial+=@{stage=$Stage;choices=@($Fields.stagedIsoInspection.choices).Count;regions=@($Fields.stagedIsoInspection.disks[0].regions | Where-Object { $_.kind -eq 'partition' -and $_.resizeState -eq 'checked' }).Count} }
    }
    function Get-Disk { return $script:streamDisk }
    function Get-StagingDisk($Number,$Identity) { if ($Number -ne 0 -or $Identity -cne 'stream-disk') { throw 'Changed disk' }; return $script:streamDisk }
    function Get-InspectedPartitions { return $partitions }
    function Get-BitLockerSnapshot { return @{known=$true;volumes=@()} }
    function Get-Volume { process { return [pscustomobject]@{Path=('volume-'+$_.PartitionNumber);DriveLetter='C';FileSystemType='NTFS';Size=$_.Size;SizeRemaining=$script:streamFree;HealthStatus='Healthy';OperationalStatus=@('OK')} } }
    function Get-PartitionSupportedSize { $script:shrinkCalls++; return @{SizeMin=[long]90GB} }
    $release=[pscustomobject]@{sizeBytes=6GB;minimumLinuxBytes=40GB}
    $tooFull=Get-StagingInspection $release
    Assert ($script:shrinkCalls -eq 0 -and $script:partial.Count -eq 1) 'Insufficient unused space must skip slow Windows shrink queries.'
    Assert (@($tooFull.disks[0].regions | Where-Object { $_.kind -eq 'partition' -and $_.resizeState -eq 'insufficient' }).Count -eq 2) 'Insufficient partitions must stay visible with an immediate result.'
    $script:streamFree=[long]120GB; $script:partial=@()
    $found=Get-StagingInspection $release
    Assert ($script:shrinkCalls -eq 2) 'Background inspection repeated a shrink calculation.'
    Assert ($script:partial.Count -eq 3 -and $script:partial[0].regions -eq 0 -and $script:partial[0].choices -eq 1) 'Partition list was not published before shrink analysis.'
    Assert ($script:partial[1].regions -eq 1 -and $script:partial[1].choices -eq 2 -and $script:partial[2].regions -eq 2 -and $script:partial[2].choices -eq 3) 'Measured limits did not accumulate.'
    Assert ($found.choices.Count -eq 3 -and @($found.disks[0].regions | Where-Object { $_.kind -eq 'partition' -and $_.resizeState -eq 'checked' }).Count -eq 2) 'Final inspection lost a resize option.'
    $query=[pscustomobject]@{diskNumber=0;diskUniqueId='stream-disk';partitionNumber=3;partitionGuid=$partitions[0].Guid}
    $initial=Get-StagingChoices $release; $measured=Get-StagingChoices $release $query
    $measured.disks[0].regions[0].sizeBytes+=1MB
    Reject { Merge-StagingAnalysis $initial $measured $query } 'Changed partition was merged into an old inventory.'
}

# Late suspension uses only the reviewed volume set; pre-existing suspension is
# not adopted. Every external/mutating boundary is replaced here.
& {
    $events=New-Object 'Collections.Generic.List[string]'
    $v=[pscustomobject]@{volumeId='volume';protectionStatus=1}
    $plan=[pscustomobject]@{diskNumber=0;bitLocker=@($v.PSObject.Copy())}
    function Get-StagingEncryption { return @($v) }
    function Assert-BitLockerUnchanged($Before,$Now) {
        if ($Before[0].protectionStatus -ne $Now[0].protectionStatus) { throw 'State changed' }
        $events.Add('validate')
    }
    function Assert-EncryptionOwner($ProcessId) { Assert ($ProcessId -eq 123) 'Missing authenticated owner'; $events.Add('owner') }
    function Get-EncryptionFacts($Id) { Assert ($Id -eq 'volume') 'Wrong volume'; return @{volumeId=$Id} }
    function Assert-SameEncryptionVolume { $events.Add('identity') }
    function Get-EncryptionLock { return New-Object IO.MemoryStream }
    function Get-FactsHash { return 'reviewed' }
    function Emit { }
    function Start-EncryptionPreparation($Expected,$Mode,$VolumeId) {
        Assert ($Expected -eq 'reviewed' -and $Mode -eq 'suspend' -and $VolumeId -eq 'volume') 'Wrong suspension request'
        $events.Add('suspend'); $v.protectionStatus=0
    }
    $v | Add-Member driveLetter 'C:'
    Start-StagingSuspension $plan 123
    Assert (($events -join ',') -eq 'validate,owner,identity,suspend') 'Suspension ordering changed'
    $events.Clear()
    Reject { Start-StagingSuspension $plan 123 } 'A changed confirmation must not be accepted'
    Assert ($events.Count -eq 0) 'Changed state caused a mutation'
    $plan.bitLocker[0].protectionStatus=0
    Start-StagingSuspension $plan 123
    Assert (($events -join ',') -eq 'validate') 'Existing suspension acquired an app-owned reminder'
}

$script:disk=[pscustomobject]@{Number=7;UniqueId='disk-identity';Size=[long]500GB;SerialNumber='serial';PartitionStyle='GPT';IsOffline=$false;IsReadOnly=$false;LogicalSectorSize=512;PhysicalSectorSize=4096;BusType='NVMe'}
& {
    function Get-Disk { return $script:disk }
    function Get-InspectedPartitions($Disk) { return @() }
    $script:disk.BusType='SAS'
    Assert ($null -ne (Get-StagingDisk 7 'disk-identity')) 'Internal SAS disks, as in Hyper-V virtual machines, must be accepted.'
    $script:disk.BusType='USB'
    Reject { Get-StagingDisk 7 'disk-identity' } 'USB disks must be rejected.'
    $script:disk.BusType='NVMe'
}
$esp=[guid]::NewGuid().ToString(); $data=[guid]::NewGuid().ToString()
$owned=@([pscustomobject]@{role='efi';guid=$esp;gptType='{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}';offsetBytes=[long]200GB;sizeBytes=[long]512MB},[pscustomobject]@{role='source';guid=$data;gptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';offsetBytes=[long](200GB+512MB);sizeBytes=[long]8GB})
$script:parts=@(foreach ($p in $owned) { [pscustomobject]@{DiskNumber=7;PartitionNumber=$(if($p.role -eq 'efi'){4}else{5});Guid=$p.guid;GptType=$p.gptType;Offset=$p.offsetBytes;Size=$p.sizeBytes;IsBoot=$false;IsSystem=$false;AccessPaths=@()} })
$state=[pscustomobject]@{schema=1;operationId=[guid]::NewGuid().ToString();diskNumber=0;diskUniqueId='disk-identity';diskSizeBytes=[long]500GB;serialNumber='serial';partitions=$owned;boot=$null;windowsBoot=@{partitionGuid=[guid]::NewGuid().ToString()};status='copying';message='';linuxOffsetBytes=[long]100GB;linuxBytes=[long]100GB}
function Get-Disk { return $script:disk }
function Get-InspectedPartitions($Disk) { return $script:parts }
function Get-Partition($DiskNumber,$PartitionNumber) { return @($script:parts | Where-Object { $_.PartitionNumber -eq $PartitionNumber })[0] }
$resolved=Get-OwnedStagingPartition $state $owned[0]
Assert ($resolved.DiskNumber -eq 7) 'Cleanup must survive disk number changes.'
$script:parts[0].IsSystem=$true
Assert ($null -ne (Get-OwnedStagingPartition $state $owned[0])) 'Windows classifying our ESP as IsSystem must not hide it.'
$script:parts[0].IsSystem=$false
$savedWindows=$state.windowsBoot.partitionGuid; $state.windowsBoot.partitionGuid=$esp
Reject { Get-OwnedStagingPartition $state $owned[0] } 'The recorded Windows ESP must never be adopted as temporary storage.'
$state.windowsBoot.partitionGuid=$savedWindows
$script:parts[1].Size++
Reject { Get-OwnedStagingPartition $state $owned[1] } 'Resized owned partition was accepted.'
$script:parts[1].Size--
$script:parts[0].IsBoot=$true
Reject { Get-OwnedStagingPartition $state $owned[0] } 'System partition was accepted for cleanup.'
$script:parts[0].IsBoot=$false
$script:parts[1].Guid=[guid]::NewGuid().ToString()
Assert ($null -eq (Get-OwnedStagingPartition $state $owned[1] $true)) 'A replacement partition must never be adopted by its number.'
$script:parts[1].Guid=$data
$realBootCheck=${function:Assert-StagingCleanupBootFiles}
$script:removed=New-Object 'Collections.Generic.List[int]'
function Assert-StagingCleanupBootFiles($State) { }
function Read-StagingLayout($Disk) { return ,([byte[]]@(1,2)) }
function Confirm-StagingDeletion($Before,$Disk,$Owned) { }
function Save-StagingState($State) { }
function Get-StagingDirectory($Id) { return Join-Path $env:TEMP $Id }
function Remove-OwnedStagingPartition($Part,$Owned,$Before) { $script:removed.Add($Part.PartitionNumber); $script:parts=@($script:parts | Where-Object { $_.Guid -ne $Part.Guid }) }
$script:parts[1].Size++
Reject { Invoke-StagingCleanup $state } 'Changed second partition must block the whole cleanup.'
Assert ($script:removed.Count -eq 0) 'Cleanup deleted something before validating both owned partitions.'
$script:parts[1].Size--
[void](Invoke-StagingCleanup $state)
Assert (($script:removed -join ',') -eq '5,4' -and $state.status -eq 'cleaned') 'Interrupted copy cleanup must remove only owned source/EFI partitions.'
[void](Invoke-StagingCleanup $state)
Assert ($script:removed.Count -eq 2) 'Repeated cleanup must be idempotent.'

$temp=Join-Path $env:TEMP ('omarchy-staged-copy-test-'+[guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($temp)
try {
    $source=Join-Path $temp 'source'; $target=Join-Path $temp 'copied'
    [IO.File]::WriteAllText($source,'verified installer bytes')
    Copy-StagedFile $source $target (Sha $source)
    Assert ((Sha $source) -ceq (Sha $target)) 'Copy readback differs.'
    Reject { Copy-StagedFile $source $target (Sha $source) } 'Existing file was overwritten.'
    Reject { Copy-StagedFile $source (Join-Path $temp 'bad-hash') ('0'*64) } 'Incorrect readback digest was accepted.'
    $script:parts=@([pscustomobject]@{DiskNumber=7;PartitionNumber=4;Guid=$esp;GptType=$owned[0].gptType;Offset=$owned[0].offsetBytes;Size=$owned[0].sizeBytes;IsBoot=$false;IsSystem=$true;AccessPaths=@()})
    function Get-StagingVolume($Part) { return [pscustomobject]@{FileSystem='FAT32'} }
    function Mount-StagingPartition($State,$Owned) { return $temp }
    function Dismount-StagingPartition($State,$Owned,$Path) { }
    $state | Add-Member -NotePropertyName loaderSha256 -NotePropertyValue (Sha $source)
    $state | Add-Member -NotePropertyName configText -NotePropertyValue "verified boot menu`n"
    $realText=${function:Write-StagedText}
    foreach ($failure in @('loader-interrupted','config-interrupted','verified-unpublished','published')) {
        & {
            $script:parts=@(foreach ($p in $owned) { [pscustomobject]@{DiskNumber=7;PartitionNumber=$(if($p.role -eq 'efi'){4}else{5});Guid=$p.guid;GptType=$p.gptType;Offset=$p.offsetBytes;Size=$p.sizeBytes;IsBoot=$false;IsSystem=$false;AccessPaths=@()} })
            $mount=Join-Path $temp $failure
            [void][IO.Directory]::CreateDirectory($mount)
            function Mount-StagingPartition($State,$Owned) { return $mount }
            if ($failure -eq 'loader-interrupted') {
                function Copy-StagedFile($Source,$Destination,$ExpectedHash) { [IO.File]::WriteAllText($Destination,'partial'); throw 'Simulated interrupted loader copy.' }
            } elseif ($failure -eq 'config-interrupted') {
                function Write-StagedText($Path,$Text) { [IO.File]::WriteAllText($Path,'partial'); throw 'Simulated interrupted config write.' }
            } elseif ($failure -eq 'verified-unpublished') {
                function Write-StagedText($Path,$Text) { & $realText $Path $Text; throw 'Simulated interruption after verification but before publication.' }
            }
            if ($failure -eq 'published') {
                Write-StagingBootFiles $state $mount $source
                Assert ((Sha (Join-Path $mount 'EFI/BOOT/BOOTX64.EFI')) -ceq $state.loaderSha256) 'Published loader differs.'
                Assert ([IO.File]::ReadAllText((Join-Path $mount 'EFI/Omarchy/grub.cfg')) -ceq $state.configText) 'Published configuration differs.'
                Reject { Write-StagingBootFiles $state $mount $source } 'Existing EFI artifacts were overwritten.'
                [IO.File]::WriteAllText((Join-Path $mount 'EFI/BOOT/BOOTX64.EFI'),'replacement loader')
                Reject { & $realBootCheck $state } 'A replaced final loader was accepted for cleanup.'
                [IO.File]::Copy($source,(Join-Path $mount 'EFI/BOOT/BOOTX64.EFI'),$true)
                [IO.File]::WriteAllText((Join-Path $mount 'EFI/Omarchy/grub.cfg'),'replacement menu')
                Reject { & $realBootCheck $state } 'A replaced final menu was accepted for cleanup.'
                [IO.File]::WriteAllText((Join-Path $mount 'EFI/Omarchy/grub.cfg'),$state.configText)
            } else {
                Reject { Write-StagingBootFiles $state $mount $source } 'Injected write interruption did not fail staging.'
                Assert (-not (Test-Path -LiteralPath (Join-Path $mount 'EFI/BOOT/BOOTX64.EFI'))) 'An unverified write was published.'
            }
            # Exercise the real boot-file guard in the cleanup path, with only
            # disk/firmware mutations mocked by the fixtures above.
            ${function:Assert-StagingCleanupBootFiles}=$realBootCheck
            $state.status='copying'; $script:removed.Clear()
            [void](Invoke-StagingCleanup $state)
            Assert (($script:removed -join ',') -eq '5,4') "Boot-file recovery failed for $failure."
        }
    }
    $script:parts=@([pscustomobject]@{DiskNumber=7;PartitionNumber=4;Guid=$esp;GptType=$owned[0].gptType;Offset=$owned[0].offsetBytes;Size=$owned[0].sizeBytes;IsBoot=$false;IsSystem=$true;AccessPaths=@()})
    [void][IO.Directory]::CreateDirectory((Join-Path $temp 'EFI/limine'))
    [IO.File]::WriteAllText((Join-Path $temp 'EFI/limine/limine_x64.efi'),'installed Linux bootloader')
    Reject { & $realBootCheck $state } 'Cleanup must refuse additional EFI files that may boot installed Linux.'
} finally {
    foreach ($fixture in @('loader-interrupted','config-interrupted','verified-unpublished','published')) {
        $dir=Join-Path $temp $fixture; $scratch='.omarchy-stage-'+$state.operationId
        foreach ($name in @("$scratch/BOOTX64.EFI","$scratch/grub.cfg",'EFI/BOOT/BOOTX64.EFI','EFI/Omarchy/grub.cfg')) {
            $path=Join-Path $dir $name; if(Test-Path -LiteralPath $path){[IO.File]::Delete($path)}
        }
        foreach ($name in @($scratch,'EFI/BOOT','EFI/Omarchy','EFI')) {
            $path=Join-Path $dir $name; if(Test-Path -LiteralPath $path){[IO.Directory]::Delete($path)}
        }
        if(Test-Path -LiteralPath $dir){[IO.Directory]::Delete($dir)}
    }
    # Exact disposable fixture files only; no recursive deletion.
    foreach ($name in @('source','copied','bad-hash')) { $path=Join-Path $temp $name; if(Test-Path -LiteralPath $path){[IO.File]::Delete($path)} }
    $efiFile=Join-Path $temp 'EFI/limine/limine_x64.efi'; if(Test-Path -LiteralPath $efiFile){[IO.File]::Delete($efiFile)}
    foreach($name in @('EFI/limine','EFI')){$path=Join-Path $temp $name;if(Test-Path -LiteralPath $path){[IO.Directory]::Delete($path)}}
    [IO.Directory]::Delete($temp)
}
# Planning never consults Secure Boot, and a reviewed shrink can also leave
# room for Omarchy right after the temporary installer.
if (-not ('Omarchy.DirectX86.NativeDisk' -as [type])) {
    Add-Type -TypeDefinition 'namespace Omarchy.DirectX86 { public static class NativeDisk { public static string Firmware() { return "uefi"; } public static object InspectWindowsBoot() { return null; } public static string LayoutHash(int disk) { return "layout"; } } }'
}
& {
    $planDisk=[pscustomobject]@{Number=0;UniqueId='plan-disk';Size=[long]500GB;SerialNumber='serial'}
    $windowsPart=[pscustomobject]@{DiskNumber=0;PartitionNumber=3;Guid='66666666-6666-6666-6666-666666666666';Offset=[long]1MB;Size=[long](500GB-2MB);GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}'}
    function Get-StagingRelease { return [pscustomobject]@{sizeBytes=[long]6GB;minimumLinuxBytes=[long]32GB} }
    function Confirm-SecureBootUEFI { throw 'Planning must not require Secure Boot to be off.' }
    function Get-StagingDisk { return $planDisk }
    function Get-StagingEncryption { return @() }
    function Get-Partition { return $windowsPart }
    function Get-ShrinkCandidate { return @{eligible=$true;maximumAllocationBytes=[long]100GB;volumeId='volume';driveLetter='C'} }
    function Get-InspectedPartitions { return @($windowsPart) }
    function Assert-Path($Path) { return $Path }
    $staging=[long](512MB+7GB)
    $request=[pscustomobject]@{sourceIsoPath='C:\omarchy.iso';sourceSha256=('a'*64);sourceLength=[long]6GB;diskNumber=0;diskUniqueId='plan-disk';targetKind='shrink';shrinkPartitionNumber=3;shrinkPartitionGuid=$windowsPart.Guid;linuxBytes=[long]40GB}
    $plan=Get-StagingPlan $request
    Assert ($plan.linuxBytes -eq 40GB -and $plan.shrink.allocationBytes -eq $staging+40GB) 'Space for Omarchy was not added to the shrink.'
    Assert ($plan.partitions[0].offsetBytes -eq $plan.shrink.freedOffsetBytes -and $plan.shrink.afterSizeBytes -eq $plan.shrink.freedOffsetBytes-1MB) 'The installer must sit right after the shrunk partition.'
    Assert ($plan.largestFreeAfterStagingBytes -eq 40GB -and $plan.shrink.driveLetter -eq 'C') 'Omarchy space or the drive letter is missing from the plan.'
    $request.linuxBytes=[long]0
    $replace=Get-StagingPlan $request
    Assert ($replace.linuxBytes -eq 0 -and $replace.shrink.allocationBytes -eq $staging) 'Replacing Windows must shrink only for the temporary installer.'
    $request.linuxBytes=[long]31GB
    Reject { Get-StagingPlan $request } 'Space below the installer minimum was accepted.'
    $request.linuxBytes=[long]99GB
    Reject { Get-StagingPlan $request } 'More space than Windows can release was accepted.'
    $free=[pscustomobject]@{sourceIsoPath='C:\omarchy.iso';sourceSha256=('a'*64);sourceLength=[long]6GB;diskNumber=0;diskUniqueId='plan-disk';targetKind='free';startOffsetBytes=[long]1MB;linuxBytes=[long]40GB}
    Reject { Get-StagingPlan $free } 'Omarchy space was reserved without a reviewed shrink.'
}

# Copying records the source hash while reporting progress, and cancellation
# from the progress callback stops it. The boot entry neither rereads the
# whole image nor waits on a one-entry menu.
& {
    $dir=Join-Path $env:TEMP ('omarchy-staged-hash-test-'+[guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($dir)
    try {
        $src=Join-Path $dir 'source'; [IO.File]::WriteAllBytes($src,[byte[]](1..200))
        $counted=@{bytes=[long]0}
        $hash=Copy-StagedFile $src (Join-Path $dir 'copy') '' { param($Count) $counted.bytes+=$Count }
        Assert ($hash -ceq (Sha $src) -and $counted.bytes -eq 200) 'Copy did not return the source hash or report every byte.'
        Reject { Copy-StagedFile $src (Join-Path $dir 'cancelled') '' { param($Count) throw 'Preparation was cancelled.' } } 'A cancelled copy completed.'
        $config=Get-StagingConfig ([pscustomobject]@{partitions=@(@{},@{guid=[guid]::NewGuid().ToString()});release=[pscustomobject]@{kernelPath='arch/boot/x86_64/vmlinuz-linux';initrdPath='arch/boot/x86_64/initramfs-linux.img'}})
        Assert ($config -notmatch 'checksum=' -and $config -match 'set timeout=0') 'The staged boot entry rereads the image or waits on a one-entry menu.'
        # Staging sums the inventory with Measure-Object, which Windows PowerShell only supports on objects.
        $inventory=@(Get-IsoInventory ($dir+'\'))
        $expected=@(Get-ChildItem -LiteralPath $dir -File)
        Assert ((($inventory | Measure-Object sizeBytes -Sum).Sum -eq ($expected | Measure-Object Length -Sum).Sum) -and $inventory.Count -eq $expected.Count) 'The ISO inventory cannot be measured.'
    } finally {
        foreach ($name in @('source','copy','cancelled')) { $path=Join-Path $dir $name; if (Test-Path -LiteralPath $path) { [IO.File]::Delete($path) } }
        [IO.Directory]::Delete($dir)
    }
}
Write-Output 'Passed staging release/ownership/encryption/copy and mocked cleanup tests; no host changes.'
