# Pure policies, disposable file copies, and mocked cleanup. No host disk calls.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$script:testingBuild=$false
# Staging paths sit under ProgramData, which has no 8.3 short names; this test
# uses TEMP instead. GitHub's Windows runners set TEMP to a short path
# (C:\Users\RUNNER~1\...) while PowerShell lists files under the long one, so
# use the long form here, as staging paths always are.
$env:TEMP=(Get-Item -LiteralPath $env:TEMP).FullName
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
# Windows disk primitives, stubbed: inspection and planning read the layout fingerprint.
if (-not ('Omarchy.DirectX86.NativeDisk' -as [type])) {
    Add-Type -TypeDefinition 'namespace Omarchy.DirectX86 { public static class NativeDisk { public static System.Collections.Generic.Queue<string> Layouts = new System.Collections.Generic.Queue<string>(); public static string Firmware() { return "uefi"; } public static object InspectWindowsBoot() { return null; } public static string LayoutHash(int disk) { return Layouts.Count > 0 ? Layouts.Dequeue() : "layout"; } } }'
}
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
        # Replacing Windows, so the app's summary names it for the installer.
        $windowsLetter=([IO.Path]::GetPathRoot([Environment]::GetFolderPath('Windows'))).Substring(0,1)
        $record=@{schema=1;operationId=$valid;status='copying';diskUniqueId='fixture';diskNumber=0;linuxOffsetBytes=1MB;linuxBytes=0;message='Interrupted';partitions=@(@{},@{sizeBytes=8GB});targetKind='shrink';shrink=@{driveLetter=$windowsLetter;partitionNumber=3}}
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
        Assert ($index.operations[0].replaces -ceq 'Windows') 'The app summary lost the partition to delete in the installer.'
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
    $part=[pscustomobject]@{DiskNumber=0;PartitionNumber=5;Guid='55555555-5555-5555-5555-555555555555';Offset=[long]1MB;Size=[long]22GB;GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';IsBoot=$false}
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
    $part=[pscustomobject]@{DiskNumber=0;PartitionNumber=3;Guid='33333333-3333-3333-3333-333333333333';Offset=[long]1MB;Size=[long](200GB-2MB);GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';IsReadOnly=$false;IsBoot=$false}
    $script:shrinkCalls=0; $script:shrinkLimit=[long]120GB
    $script:minimumBytes=[long]40GB
    function Emit { }
    function Get-Disk { return $disk }
    function Get-StagingDisk($Number,$Identity) { if ($Number -ne 0 -or $Identity -cne 'disk') { throw 'Changed disk' }; return $disk }
    function Get-InspectedPartitions { return @($part) }
    function Get-BitLockerSnapshot { return @{known=$true;volumes=@()} }
    $script:volumeLetter='C'; $script:volumeLabel=''; $script:volumeFree=[long]100GB
    function Get-Volume { return [pscustomobject]@{Path='volume';DriveLetter=$script:volumeLetter;FileSystemLabel=$script:volumeLabel;FileSystemType='NTFS';Size=$part.Size;SizeRemaining=$script:volumeFree;HealthStatus='Healthy';OperationalStatus=@('OK')} }
    function Get-PartitionSupportedSize { $script:shrinkCalls++; return @{SizeMin=$script:shrinkLimit} }
    $release=[pscustomobject]@{sizeBytes=6GB;minimumLinuxBytes=40GB}
    $found=Get-StagingChoices $release
    Assert ($script:shrinkCalls -eq 0) 'Disk listing must not trigger shrink analysis.'
    Assert ($found.disks.Count -eq 1 -and $found.choices.Count -eq 0 -and $found.blocked.Count -eq 0) 'A full disk was hidden or blocked.'
    $region=$found.disks[0].regions[0]
    Assert ($region.freeBytes -eq 100GB -and $region.resizeState -eq 'unchecked') 'Unused filesystem space must remain distinct from unallocated space.'
    Assert ($found.disks[0].unallocatedBytes -eq 0) 'Filesystem free bytes became unallocated disk space.'
    Assert ($found.disks[0].layoutSha256 -eq 'layout') 'Inspection did not record the layout fingerprint the plan checks.'
    [Omarchy.DirectX86.NativeDisk]::Layouts.Enqueue('before'); [Omarchy.DirectX86.NativeDisk]::Layouts.Enqueue('after')
    Assert ((Get-StagingChoices $release).disks[0].layoutSha256 -eq '') 'A layout that changed during inspection vouched for the regions read before it.'
    Assert ($region.isWindows -eq $false) 'A data partition was marked as the Windows partition.'
    $part.IsBoot=$true
    Assert ((Get-StagingChoices $release).disks[0].regions[0].isWindows -eq $true) 'The Windows boot partition was not marked.'
    $part.IsBoot=$false
    $script:volumeLetter=''; $script:volumeLabel='RESTORE'
    $labeled=Get-StagingChoices $release
    Assert ($labeled.disks[0].regions[0].label -eq 'RESTORE (Partition 3)') 'A volume label without a drive letter was hidden.'
    $script:volumeLetter='C'; $script:volumeLabel='OS'
    $labeled=Get-StagingChoices $release
    Assert ($labeled.disks[0].regions[0].label -eq 'OS (C:)') 'The drive letter and filesystem label were not shown together.'
    $script:volumeLabel=''
    $script:volumeFree=[long]20GB
    $short=(Get-StagingChoices $release).disks[0].regions[0]
    Assert ($short.resizeState -eq 'insufficient' -and $short.resizeReason -like 'Not enough space*Free up space*') 'A partition short of free space did not ask to free space.'
    $size=$part.Size; $part.Size=[long]22GB; $script:volumeFree=[long]8GB
    $short=(Get-StagingChoices $release).disks[0].regions[0]
    Assert ($short.resizeState -eq 'insufficient' -and $short.resizeReason -eq 'This partition is too small for the temporary installer.') 'A partition too small for the installer asked to free space.'
    $part.Size=$size; $script:volumeFree=[long]100GB
    $query=[pscustomobject]@{diskNumber=0;diskUniqueId='disk';partitionNumber=3;partitionGuid=$part.Guid}
    $found=Get-StagingChoices $release $query
    Assert ($script:shrinkCalls -eq 1 -and $found.choices.Count -eq 1) 'Selected resize analysis did not expose a usable allocation.'
    Assert ($found.choices[0].target.partition_guid -eq $part.Guid -and $found.choices[0].largestFreeAfterStagingBytes -eq 0) 'Temporary-only shrink lost identity or invented Linux space.'
    $script:shrinkLimit=195GB
    $found=Get-StagingChoices $release $query
    Assert ($found.choices.Count -eq 0 -and $found.disks[0].regions[0].resizeState -eq 'checked' -and $found.disks[0].regions[0].maximumReleaseBytes -eq 5GB-2MB) 'Insufficient shrink space must retain the measured limit and partition.'
    Assert ($found.disks[0].regions[0].resizeReason -like 'Windows cannot move some files*') 'A Windows shrink limit was blamed on the free-space reserve.'
    # Stop pressed while C: is measured: the next disk is still read, so C:'s
    # result is returned rather than lost.
    $temp=Join-Path $env:TEMP ('omarchy-stop-keep-'+[guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($temp); $cancel=Join-Path $temp 'staged-cancel'
    $other=[pscustomobject]@{Number=1;UniqueId='other';Size=[long]100GB}
    function Get-Disk { return @($disk,$other) }
    function Get-StagingDisk($Number,$Identity) { if ($Number -eq 1) { return $other }; return $disk }
    function Get-InspectedPartitions($Disk) { if ($Disk.Number -eq 0) { return @($part) }; return @() }
    function Get-PartitionSupportedSize { $script:shrinkCalls++; [IO.File]::WriteAllText($cancel,''); return @{SizeMin=$script:shrinkLimit} }
    try {
        $kept=Get-StagingChoices $release $query $cancel
        Assert ($kept.disks.Count -eq 2 -and $kept.disks[0].regions[0].resizeState -eq 'checked') 'Stop discarded the measurement that had just finished.'
    } finally { Remove-Item -LiteralPath $temp -Recurse -Force }
    $query.partitionGuid='44444444-4444-4444-4444-444444444444'
    function Get-Disk { return $disk }
    Reject { Get-StagingChoices $release $query } 'Resize query accepted a replaced partition.'
    Assert ($script:shrinkCalls -eq 3) 'Changed partition reached shrink analysis.'
}

# One privileged inspection emits the inventory before running the expensive
# Windows query, then publishes each result without opening another helper.
& {
    $script:streamDisk=[pscustomobject]@{Number=0;UniqueId='stream-disk';Size=[long]400GB}
    $partitions=@(
        [pscustomobject]@{DiskNumber=0;PartitionNumber=3;Guid='33333333-3333-3333-3333-333333333333';Offset=[long]1MB;Size=[long]180GB;GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';IsReadOnly=$false;IsBoot=$true},
        [pscustomobject]@{DiskNumber=0;PartitionNumber=4;Guid='44444444-4444-4444-4444-444444444444';Offset=[long]181GB;Size=[long]180GB;GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';IsReadOnly=$false;IsBoot=$false}
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
Assert ($state.message -ceq 'The temporary installer was removed. Its space stays unallocated; Windows and any installed Omarchy were not changed.') 'A record without BitLocker data must not mention BitLocker.'
$noted=Get-StagingBitLockerNote ([pscustomobject]@{bitLocker=@([pscustomobject]@{driveLetter='C:';protectionStatus=1},[pscustomobject]@{driveLetter='D:';protectionStatus=0},[pscustomobject]@{driveLetter='';protectionStatus=1})})
Assert ($noted -ceq ' If BitLocker on C:, a volume is still paused, resume it in Windows.') "BitLocker note must name only volumes protected when preparing: $noted"
Assert ((Get-StagingBitLockerNote ([pscustomobject]@{bitLocker=@([pscustomobject]@{driveLetter='C:';protectionStatus=0})})) -ceq '') 'Volumes already unprotected when preparing must not be mentioned.'
Assert ((Get-StagingBitLockerNote ([pscustomobject]@{bitLocker=$null})) -ceq '') 'An empty BitLocker record must not be mentioned.'
[void](Invoke-StagingCleanup $state)
Assert ($script:removed.Count -eq 2) 'Repeated cleanup must be idempotent.'

# Removal gives the space back to the partition it was shrunk from, up to the
# next partition (installed Omarchy) or the disk's usable end.
& {
    $usableEnd=[long]500GB-1MB
    $layout=New-Object byte[] 48
    [BitConverter]::GetBytes([long]1MB).CopyTo($layout,24); [BitConverter]::GetBytes($usableEnd-1MB).CopyTo($layout,32)
    function Read-StagingLayout($Disk) { return ,$layout }
    $windowsGuid=[guid]::NewGuid().ToString()
    $script:resized=New-Object 'Collections.Generic.List[string]'; $script:failResize=$false
    $fresh={
        $script:parts=@([pscustomobject]@{DiskNumber=7;PartitionNumber=3;Guid=$windowsGuid;GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';Offset=[long]1MB;Size=[long]200GB-1MB;DriveLetter='C';IsBoot=$true;IsSystem=$false;AccessPaths=@()})+@(foreach ($p in $owned) { [pscustomobject]@{DiskNumber=7;PartitionNumber=$(if($p.role -eq 'efi'){4}else{5});Guid=$p.guid;GptType=$p.gptType;Offset=$p.offsetBytes;Size=$p.sizeBytes;IsBoot=$false;IsSystem=$false;AccessPaths=@()} })
        $script:removed.Clear(); $script:resized.Clear()
    }
    function Resize-Partition($DiskNumber,$PartitionNumber,$Size) {
        if ($script:failResize) { throw 'Not enough available capacity.' }
        $script:resized.Add("$PartitionNumber=$Size"); @($script:parts | Where-Object { $_.PartitionNumber -eq $PartitionNumber })[0].Size=$Size
    }
    $shrunk=[pscustomobject]@{schema=1;operationId=[guid]::NewGuid().ToString();diskNumber=7;diskUniqueId='disk-identity';diskSizeBytes=[long]500GB;serialNumber='serial';partitions=$owned;boot=$null;windowsBoot=@{partitionGuid=[guid]::NewGuid().ToString()};status='boot-scheduled';message='';linuxOffsetBytes=[long](200GB+512MB+8GB);linuxBytes=[long]64GB;shrink=[pscustomobject]@{partitionGuid=$windowsGuid;offsetBytes=[long]1MB;driveLetter='C'}}
    & $fresh
    # Omarchy was installed after the installer: C: grows up to it.
    $script:parts+=@([pscustomobject]@{DiskNumber=7;PartitionNumber=6;Guid=[guid]::NewGuid().ToString();GptType='{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}';Offset=[long](200GB+512MB+8GB);Size=[long]2GB;IsBoot=$false;IsSystem=$false;AccessPaths=@()})
    $grow=Get-StagingReclaim $shrunk
    Assert ($grow.name -eq 'C:' -and $grow.partitionNumber -eq 3 -and $grow.beforeBytes -eq 200GB-1MB -and $grow.afterBytes -eq 200GB+512MB+8GB-1MB) 'C: must grow exactly up to installed Omarchy.'
    $shrunk.status='boot-scheduled'
    $result=Invoke-StagingCleanup $shrunk $true
    Assert (($script:removed -join ',') -eq '5,4' -and ($script:resized -join ',') -eq ('3='+(200GB+512MB+8GB-1MB))) 'Removal must delete both partitions, then grow C: over their space.'
    Assert ($shrunk.status -eq 'cleaned' -and $shrunk.message -clike 'The temporary installer was removed. C: grew from * to * GiB over its space; any installed Omarchy was not changed.') "Removal must say how far C: grew: $($shrunk.message)"
    Assert ($null -eq (Get-StagingReclaim $shrunk)) 'A grown C: has nothing left to reclaim.'
    # Nothing installed: C: grows over the installer and Omarchy's space, up to the disk's usable end.
    & $fresh; $shrunk.status='staged'
    Assert ((Get-StagingReclaim $shrunk).afterBytes -eq $usableEnd-1MB) 'Without Omarchy, C: must grow to the disk usable end.'
    # A failed resize keeps the removal and says the space stays unallocated.
    $script:failResize=$true
    [void](Invoke-StagingCleanup $shrunk $true)
    $script:failResize=$false
    Assert ($shrunk.status -eq 'cleaned' -and ($script:removed -join ',') -eq '5,4' -and $script:resized.Count -eq 0 -and $shrunk.message -clike 'The temporary installer was removed. Its space stays unallocated: C: could not be extended (Not enough available capacity). Extend it in Disk Management.') "A failed resize must keep the removal: $($shrunk.message)"
    # Prepare again keeps the space free.
    & $fresh; $shrunk.status='staged'
    [void](Invoke-StagingCleanup $shrunk)
    Assert ($script:resized.Count -eq 0 -and $shrunk.message -clike '*Its space stays unallocated; Windows*') 'Prepare again must not grow C:.'
    # Only the partition it came from, only from where it starts, and only
    # with nothing else in between.
    & $fresh
    Assert ($null -eq (Get-StagingReclaim ([pscustomobject]@{shrink=$null;partitions=$owned}))) 'Space taken from free space must stay unallocated.'
    $script:parts[0].Offset=[long]2MB
    Assert ($null -eq (Get-StagingReclaim $shrunk)) 'A moved partition must not grow.'
    $script:parts[0].Offset=[long]1MB; $script:parts[0].Size=[long]199GB
    $script:parts+=@([pscustomobject]@{DiskNumber=7;PartitionNumber=8;Guid=[guid]::NewGuid().ToString();GptType='{de94bba4-06d1-4d40-a16a-bfd50179d6ac}';Offset=[long]199GB+1MB;Size=[long]512MB;IsBoot=$false;IsSystem=$false;AccessPaths=@()})
    Assert ($null -eq (Get-StagingReclaim $shrunk)) 'A partition between C: and the installer must stop the growth.'
}

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
& {
    $planDisk=[pscustomobject]@{Number=0;UniqueId='plan-disk';Size=[long]500GB;SerialNumber='serial'}
    $windowsPart=[pscustomobject]@{DiskNumber=0;PartitionNumber=3;Guid='66666666-6666-6666-6666-666666666666';Offset=[long]1MB;Size=[long](500GB-2MB);GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';DriveLetter='C'}
    # Planning says what it waits for while Windows measures.
    function Emit($Type,$Stage,$Fields) { if ($Fields.message) { $script:planningMessages+=$Fields.message } }
    $script:planningMessages=@()
    function Get-StagingRelease { return [pscustomobject]@{sizeBytes=[long]6GB;minimumLinuxBytes=[long]32GB} }
    function Confirm-SecureBootUEFI { throw 'Planning must not require Secure Boot to be off.' }
    function Get-StagingDisk { return $planDisk }
    function Get-StagingEncryption { return @() }
    function Get-Partition { return $windowsPart }
    function Get-ShrinkCandidate { return @{eligible=$true;maximumAllocationBytes=[long]100GB;volumeId='volume';driveLetter='C'} }
    function Get-InspectedPartitions { return @($windowsPart) }
    function Assert-Path($Path) { return $Path }
    $staging=[long](512MB+6GB+256MB)
    $request=[pscustomobject]@{sourceIsoPath='C:\omarchy.iso';sourceSha256=('a'*64);sourceLength=[long]6GB;diskNumber=0;diskUniqueId='plan-disk';targetKind='shrink';shrinkPartitionNumber=3;shrinkPartitionGuid=$windowsPart.Guid;linuxBytes=[long]40GB}
    $plan=Get-StagingPlan $request
    Assert ($plan.linuxBytes -eq 40GB -and $plan.shrink.allocationBytes -eq $staging+40GB) 'Space for Omarchy was not added to the shrink.'
    Assert ($plan.partitions[0].offsetBytes -eq $plan.shrink.freedOffsetBytes -and $plan.shrink.afterSizeBytes -eq $plan.shrink.freedOffsetBytes-1MB) 'The installer must sit right after the shrunk partition.'
    Assert ($plan.largestFreeAfterStagingBytes -eq 40GB -and $plan.shrink.driveLetter -eq 'C') 'Omarchy space or the drive letter is missing from the plan.'
    Assert ($script:planningMessages -contains 'Checking how much C: can shrink. This can take a few minutes.') 'Planning did not say it was measuring C:.'
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
        Assert ($hash -ceq (Sha $src) -and $counted.bytes -eq 400) 'Copy did not return the source hash or report every byte of both passes.'
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
# shutdown.exe exits with 203 without restarting when OsIndications does not
# exist yet; the firmware restart retries that once and fails on anything else.
& {
    function Get-OwnedStagingPartition { return [pscustomobject]@{DiskNumber=0} }
    function Assert-StagingEncryption { }
    function Confirm-SecureBootUEFI { return $true }
    # The app keeps its reopen entry once `restarting` is reported, so it
    # comes before shutdown.exe, and never when nothing restarts.
    function Emit($Type,$Stage,$Fields) { $script:events+=,@($Stage,$script:shutdownCalls) }
    function Start-Process { $script:shutdownCalls++; return [pscustomobject]@{ExitCode=$script:exitCodes[$script:shutdownCalls-1]} }
    $state=[pscustomobject]@{operationId='firmware';status='staged';partitions=@(@{})}
    foreach ($case in @(@{codes=@(203,0);calls=2;ok=$true},@{codes=@(0);calls=1;ok=$true},@{codes=@(203,203);calls=2;ok=$false},@{codes=@(5);calls=1;ok=$false})) {
        $script:exitCodes=$case.codes; $script:shutdownCalls=0; $script:events=@(); $ok=$true
        try { [void](Invoke-StagingFirmware $state) } catch { $ok=$false }
        Assert ($ok -eq $case.ok -and $script:shutdownCalls -eq $case.calls) "Firmware restart with exit codes $($case.codes -join ',') was handled incorrectly."
        Assert ($script:events.Count -eq 1 -and $script:events[0][0] -eq 'restarting' -and $script:events[0][1] -eq 0) 'The restarting stage was not reported before shutdown.exe.'
    }
    function Confirm-SecureBootUEFI { return $false }
    $script:events=@(); $script:shutdownCalls=0
    $off=Invoke-StagingFirmware $state
    Assert ($off.secureBoot -eq $false -and $script:events.Count -eq 0 -and $script:shutdownCalls -eq 0) 'Secure Boot already off reported a restart.'
}
# The summary names the partition to delete in the installer only for a
# shrink that left no space for Omarchy.
& {
    $windowsLetter=([IO.Path]::GetPathRoot([Environment]::GetFolderPath('Windows'))).Substring(0,1)
    $record={ param($kind,$linux,$letter) [pscustomobject]@{operationId='o';status='staged';diskUniqueId='d';diskNumber=0;linuxOffsetBytes=0;linuxBytes=[long]$linux;message='';partitions=@(@{},@{sizeBytes=[long]8GB});targetKind=$kind;shrink=$(if ($kind -eq 'shrink') { [pscustomobject]@{driveLetter=$letter;partitionNumber=3} } else { $null })} }
    Assert ($null -eq (Get-StagingSummary (& $record 'free' 0 '')).replaces) 'A free-space installer asked to delete a partition.'
    Assert ($null -eq (Get-StagingSummary (& $record 'shrink' 64GB $windowsLetter)).replaces) 'Keeping Windows asked to delete it.'
    Assert ((Get-StagingSummary (& $record 'shrink' 0 $windowsLetter)).replaces -ceq 'Windows') 'Replacing Windows did not name it.'
    $other=if ($windowsLetter -ieq 'D') { 'E' } else { 'D' }
    Assert ((Get-StagingSummary (& $record 'shrink' 0 $other)).replaces -ceq ($other+':')) 'Replacing a data partition did not name its drive.'
    Assert ((Get-StagingSummary (& $record 'shrink' 0 '')).replaces -ceq 'Partition 3') 'A partition without a letter was not named.'
    $old=& $record 'free' 0 ''; $old.PSObject.Properties.Remove('targetKind')
    Assert ($null -eq (Get-StagingSummary $old).replaces) 'An older record without a target kind asked to delete a partition.'
}
# Preparing lists its steps from the reviewed plan.
& {
    $ids={ param($plan) (@(Get-StagingSteps $plan) | ForEach-Object { $_.id }) -join ',' }
    $free=[pscustomobject]@{bitLocker=@();shrink=$null}
    Assert ((& $ids $free) -ceq 'verify,partitions,copy,startup') 'Free space listed the wrong steps.'
    $shrink=[pscustomobject]@{bitLocker=@([pscustomobject]@{driveLetter='C:';protectionStatus=1},[pscustomobject]@{driveLetter='D:';protectionStatus=0});shrink=[pscustomobject]@{driveLetter='C';partitionNumber=3}}
    $steps=@(Get-StagingSteps $shrink)
    Assert ((& $ids $shrink) -ceq 'verify,preflight,bitlocker,shrink,partitions,copy,startup') 'A shrink with BitLocker listed the wrong steps.'
    Assert ($steps[1].label -ceq 'Check that C: can still shrink' -and $steps[2].label -ceq 'Pause BitLocker on C:' -and $steps[3].label -ceq 'Shrink C:') 'Steps did not name the partitions.'
    $older=[pscustomobject]@{shrink=[pscustomobject]@{driveLetter='';partitionNumber=4}}
    Assert ((@(Get-StagingSteps $older))[2].label -ceq 'Shrink Partition 4') 'A record without BitLocker data or a letter was mislabelled.'
}
# Steps record when each started and ended, for the test runs' timings.
& {
    function Emit($Type,$Stage,$Fields) { $script:emitted+=,$Fields }
    $script:emitted=@()
    $plan=[pscustomobject]@{operationId='timed';status='planned'}
    Enter-StagingStep $plan 'verify' 'verifying-source' 'Verifying'
    Enter-StagingStep $plan 'copy' 'copying-installer' 'Copying'
    Enter-StagingStep $plan '' '' ''
    Assert (@($plan.timings).Count -eq 2 -and $plan.timings[0].end -and $plan.timings[1].end -and $plan.timings[1].step -eq 'copy') 'Step timings were not recorded, or finishing added an empty one.'
    Assert ($script:emitted.Count -eq 2 -and $script:emitted[1].step -eq 'copy') 'Entering a step did not tell the app, or finishing did.'
    # Once preparing, each step change is saved, so an interrupted run keeps its timings.
    function Save-StagingState($State) { $script:saved++ }
    $script:saved=0; $plan.status='preparing'
    Enter-StagingStep $plan 'startup' 'registering-boot' 'Adding'
    Assert ($script:saved -eq 1) 'A step change was not saved.'
}
# The disk loop itself stops before reading the next disk.
& {
    $temp=Join-Path $env:TEMP ('omarchy-stop-loop-'+[guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($temp); $cancel=Join-Path $temp 'staged-cancel'; [IO.File]::WriteAllText($cancel,'')
    function Emit { }
    function Confirm-SecureBootUEFI { return $true }
    function Get-BitLockerSnapshot { return @{known=$false;volumes=@()} }
    function Get-Disk { return @([pscustomobject]@{Number=0;UniqueId='a';Size=[long]500GB},[pscustomobject]@{Number=1;UniqueId='b';Size=[long]500GB}) }
    function Get-StagingDisk { $script:read++; throw 'Disks must not be read after Stop.' }
    try {
        $script:read=0; $stopped=$false
        try { [void](Get-StagingChoices ([pscustomobject]@{sizeBytes=[long]6GB;minimumLinuxBytes=[long]32GB}) $null $cancel) } catch { $stopped=$_.Exception.Message -ceq 'The disk check was stopped.' }
        Assert ($stopped -and $script:read -eq 0) 'The disk loop read a disk after Stop.'
        # A stop inside a disk's own reading (before measuring) isn't recorded as an unusable disk.
        [IO.File]::Delete($cancel)
        function Get-StagingDisk { throw 'The disk check was stopped.' }
        $stopped=$false
        try { [void](Get-StagingChoices ([pscustomobject]@{sizeBytes=[long]6GB;minimumLinuxBytes=[long]32GB}) $null $cancel) } catch { $stopped=$_.Exception.Message -ceq 'The disk check was stopped.' }
        Assert $stopped 'A stop while reading a disk became a blocked disk.'
    } finally { Remove-Item -LiteralPath $temp -Recurse -Force }
}
# The chunked hash matches Get-FileHash, and copying counts the readback too.
& {
    $temp=Join-Path $env:TEMP ('omarchy-hash-'+[guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($temp)
    try {
        $source=Join-Path $temp 'source'; $bytes=New-Object byte[] (5MB+123); (New-Object Random 7).NextBytes($bytes); [IO.File]::WriteAllBytes($source,$bytes)
        $counted=@{bytes=[long]0}; $count={ param([long]$n) $counted.bytes+=$n }
        Assert ((Get-StagingFileHash $source $count) -ceq (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant() -and $counted.bytes -eq $bytes.Length) 'The chunked hash was wrong or missed bytes.'
        $counted.bytes=0
        [void](Copy-StagedFile $source (Join-Path $temp 'copy') '' $count)
        Assert ($counted.bytes -eq 2*$bytes.Length) 'Copy progress did not count the readback.'
    } finally { Remove-Item -LiteralPath $temp -Recurse -Force }
}
# Stop takes effect before the next partition is measured.
& {
    $temp=Join-Path $env:TEMP ('omarchy-stop-'+[guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($temp); $cancel=Join-Path $temp 'staged-cancel'
    function Emit { }
    $first=@{diskNumber=0;diskUniqueId='d';diskSizeBytes=[long]500GB;regions=@(@{kind='partition';partitionNumber=3;partitionGuid='a';resizeState='unchecked';label='C:'},@{kind='partition';partitionNumber=4;partitionGuid='b';resizeState='unchecked';label='D:'})}
    function Get-StagingChoices($Release,$Resize) {
        if ($null -eq $Resize) { return @{disks=@($first);choices=@();blocked=@()} }
        $script:measured+=$Resize.partitionNumber
        # The user stops while the first measurement runs.
        [IO.File]::WriteAllText($cancel,'')
        return @{disks=@($first);choices=@();blocked=@()}
    }
    function Merge-StagingAnalysis($Inventory,$Measured,$Query) { return $Inventory }
    try {
        $script:measured=@(); $stopped=$false
        try { [void](Get-StagingInspection @{} $null $cancel) } catch { $stopped=$_.Exception.Message -ceq 'The disk check was stopped.' }
        Assert ($stopped -and $script:measured.Count -eq 1) 'Stop did not end the check before the next partition.'
        $script:measured=@(); $stopped=$false
        try { [void](Get-StagingInspection @{} $null $cancel) } catch { $stopped=$true }
        Assert ($stopped -and $script:measured.Count -eq 0) 'A stopped check still measured.'
    } finally { Remove-Item -LiteralPath $temp -Recurse -Force }
}
# The final check measures before BitLocker or the disk change; a stale limit
# removes the record and fails plainly.
& {
    function Enter-StagingStep { }
    $part=[pscustomobject]@{Guid='g';PartitionNumber=3}
    function Get-Partition { return $part }
    function Remove-StagingRecord($State) { $script:removed++ }
    $plan=[pscustomobject]@{diskNumber=0;status='preparing';operationId='o';shrink=[pscustomobject]@{partitionNumber=3;partitionGuid='g';volumeId='v';driveLetter='C';afterSizeBytes=[long]80GB}}
    function Get-ShrinkCandidate($Partition,$Known) { return @{eligible=$true;volumeId='v';minimumSizeBytes=[long]70GB;windowsMinimumSizeBytes=[long]60GB} }
    $script:removed=0
    Assert ((Get-StagingShrinkPreflight $plan) -eq 60GB -and $script:removed -eq 0) 'A shrink that still fits did not return its fresh minimum.'
    function Get-ShrinkCandidate($Partition,$Known) { return @{eligible=$true;volumeId='v';minimumSizeBytes=[long]90GB;windowsMinimumSizeBytes=[long]85GB} }
    $message=''
    try { [void](Get-StagingShrinkPreflight $plan) } catch { $message=$_.Exception.Message }
    Assert ($message -ceq 'Windows can no longer shrink C: that far. Nothing was changed. Check again.' -and $script:removed -eq 1) 'A stale limit did not fail plainly or left the record.'
    $part.Guid='other'; $script:removed=0
    Reject { Get-StagingShrinkPreflight $plan } 'A replaced partition passed the final check.'
}
# Only a record from before any change is removed, with its summary entry.
& {
    $temp=Join-Path $env:TEMP ('omarchy-remove-record-'+[guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($temp); [IO.File]::WriteAllText((Join-Path $temp 'state.json'),'{}')
    function Get-StagingDirectory($Id) { return $temp }
    function Assert-ProtectedDirectory($Path) { }
    function Write-StagingSummary { $script:summaries++ }
    try {
        $script:summaries=0
        Reject { Remove-StagingRecord ([pscustomobject]@{operationId='o';status='allocation-starting'}) } 'A record from after the disk changed was removed.'
        Assert ((Test-Path -LiteralPath $temp) -and $script:summaries -eq 0) 'A refused removal still changed something.'
        Remove-StagingRecord ([pscustomobject]@{operationId='o';status='preparing'})
        Assert (-not (Test-Path -LiteralPath $temp) -and $script:summaries -eq 1) 'An untouched record or its summary entry was left behind.'
    } finally { if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force } }
}
# The staged write measures before pausing BitLocker (source order), and
# shrinks with that measurement.
& {
    $source=Get-Content -LiteralPath (Join-Path $root 'providers/staged-iso/Staging.ps1') -Raw
    $write=$source.Substring($source.IndexOf('function Invoke-StagingWrite'))
    Assert ($write.IndexOf('Get-StagingShrinkPreflight') -lt $write.IndexOf('Start-StagingSuspension') -and $write.IndexOf('Invoke-ConfirmedShrink $Plan $directory $minimum') -gt 0) 'The final check no longer runs before BitLocker is paused.'
}
# Windows' own disk and partition are measured first.
& {
    function Emit { }
    $data=@{diskNumber=0;diskUniqueId='data';diskSizeBytes=[long]1TB;regions=@(@{kind='partition';partitionNumber=1;partitionGuid='d';resizeState='unchecked';label='D:';isWindows=$false})}
    $system=@{diskNumber=1;diskUniqueId='system';diskSizeBytes=[long]500GB;regions=@(@{kind='partition';partitionNumber=2;partitionGuid='e';resizeState='unchecked';label='E:';isWindows=$false},@{kind='partition';partitionNumber=3;partitionGuid='c';resizeState='unchecked';label='C:';isWindows=$true})}
    function Get-StagingChoices($Release,$Resize) {
        if ($null -ne $Resize) { $script:order+=$Resize.partitionGuid }
        return @{disks=@($data,$system);choices=@();blocked=@()}
    }
    function Merge-StagingAnalysis($Inventory,$Measured,$Query) { return $Inventory }
    $script:order=@()
    [void](Get-StagingInspection @{} $null '')
    Assert (($script:order -join ',') -ceq 'c,e,d') "Windows' partition was not measured first."
}
# Planning reuses the helper's own measurement only for the partition as it is now.
& {
    $planDisk=[pscustomobject]@{Number=0;UniqueId='plan-disk';Size=[long]500GB;SerialNumber='serial'}
    $windowsPart=[pscustomobject]@{DiskNumber=0;PartitionNumber=3;Guid='77777777-7777-7777-7777-777777777777';Offset=[long]1MB;Size=[long](500GB-2MB);GptType='{ebd0a0a2-b9e5-4433-87c0-68b6b72699c7}';DriveLetter='C'}
    function Get-StagingRelease { return [pscustomobject]@{sizeBytes=[long]6GB;minimumLinuxBytes=[long]32GB} }
    function Get-StagingDisk { return $planDisk }
    function Get-StagingEncryption { return @() }
    function Get-Partition { return $windowsPart }
    function Get-ShrinkCandidate($Partition,[long]$Known) { $script:known=$Known; return @{eligible=$true;maximumAllocationBytes=[long]100GB;volumeId='volume';driveLetter='C'} }
    function Get-InspectedPartitions { return @($windowsPart) }
    function Assert-Path($Path) { return $Path }
    function Emit($Type,$Stage,$Fields) { if ($Stage -eq 'planning') { $script:measuring=$true } }
    $request=[pscustomobject]@{sourceIsoPath='C:\omarchy.iso';sourceSha256=('a'*64);sourceLength=[long]6GB;diskNumber=0;diskUniqueId='plan-disk';targetKind='shrink';shrinkPartitionNumber=3;shrinkPartitionGuid=$windowsPart.Guid;linuxBytes=[long]0;knownMinimum=[pscustomobject]@{bytes=[long]300GB;offsetBytes=[long]1MB;sizeBytes=[long](500GB-2MB)}}
    $script:known=-1; $script:measuring=$false
    [void](Get-StagingPlan $request)
    Assert ($script:known -eq 300GB -and -not $script:measuring) 'Planning measured again despite a fresh measurement of this partition.'
    $request.knownMinimum.sizeBytes=[long]400GB
    $script:known=-1; $script:measuring=$false
    [void](Get-StagingPlan $request)
    Assert ($script:known -eq 0 -and $script:measuring) 'Planning reused a measurement of a partition that has since changed.'
}
Write-Output 'Passed staging release/ownership/encryption/copy and mocked cleanup tests; no host changes.'
