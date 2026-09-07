function Invoke-DirectPlan($Request) {
    $allowed=@('operationId','diskNumber','diskUniqueId','targetKind','allocationBytes','startOffsetBytes','shrinkPartitionNumber','shrinkPartitionGuid','deletePartitionNumber','deletePartitionGuid','deleteOffsetBytes','deleteSizeBytes','deleteConfirmation','protectedPaths','manifestPath','manifestSha256','encryption','bootMenu')
    $required=@('operationId','diskNumber','diskUniqueId','targetKind','allocationBytes','manifestPath','manifestSha256','encryption','protectedPaths','bootMenu')
    if ($Request.targetKind -eq 'free') { $required += 'startOffsetBytes' }
    elseif ($Request.targetKind -eq 'shrink') { $required += @('shrinkPartitionNumber','shrinkPartitionGuid') }
    elseif ($Request.targetKind -eq 'delete') { $required += @('deletePartitionNumber','deletePartitionGuid','deleteOffsetBytes','deleteSizeBytes','deleteConfirmation') }
    Assert-Fields $Request $allowed $required
    Assert-Admin
    if ($Request.encryption -cne 'luks2') { Fail 'encryption_unsupported' 'This installation requires the per-install LUKS2 recipe.' }
    $manifest=Read-Manifest $Request.manifestPath $Request.manifestSha256
    Assert-BootMenuEqual $Request.bootMenu $manifest.bootMenu
    $windowsBoot=Get-VerifiedWindowsBoot
    if ($manifest.operationId -ne $script:operationId) { Fail 'operation_mismatch' 'Image belongs to another operation.' }
    $directory=Split-Path -Parent $Request.manifestPath; Assert-ProtectedDirectory $directory
    $probe=Get-Probe $Request.protectedPaths; $selection=Select-Allocation $probe $Request; $disk=$selection.disk
    $partitions=@(); $start=[long]$selection.startOffsetBytes
    foreach ($role in @('esp','root')) {
        $image=@($manifest.outputs | Where-Object { $_.role -eq $role })[0]
        $size=if ($role -eq 'esp') { $script:espBytes } else { [long]$Request.allocationBytes-$script:espBytes }
        $partitions += [ordered]@{role=$role;partitionGuid=$image.partitionGuid;offsetBytes=$start;sizeBytes=$size;imageSizeBytes=[long]$image.sizeBytes;sha256=$image.sha256;file=$image.file}
        $start += $size
    }
    $affected=@(Get-AffectedBitLocker $probe.bitLocker $disk.diskNumber)
    $suspend=@($affected | Where-Object { $_.isOsVolume -and $_.conversionStatus -eq 1 -and $_.protectionStatus -eq 1 } | ForEach-Object { [ordered]@{volumeId=$_.volumeId;driveLetter=$_.driveLetter} })
    $plan=[ordered]@{schemaVersion=2;kind='omarchy-windows-alongside-plan';operationId=$script:operationId;model='gpt-6-astra';manifestSha256=$Request.manifestSha256;diskNumber=$disk.diskNumber;diskUniqueId=$disk.diskUniqueId;serialNumber=$disk.serialNumber;diskSizeBytes=$disk.sizeBytes;logicalSectorBytes=$disk.logicalSectorBytes;layoutSha256=[Omarchy.DirectX86.NativeDisk]::LayoutHash($disk.diskNumber);bootOrderSha256=[Omarchy.DirectX86.NativeDisk]::FirmwarePreflight();targetKind=$Request.targetKind;allocationBytes=[long]$Request.allocationBytes;shrink=$selection.shrink;preservedPartitions=$disk.partitions;partitions=$partitions;encryption='luks2';protectionState='owner-setup-required';bitLocker=[ordered]@{volumes=$affected;suspendVolumes=$suspend;policy='suspend-active-os-protection-restore-verified';rebootLimit=1;recovery='protected-system-startup-and-minute-watchdog'};bootPath='\EFI\limine\limine_x64.efi';bootPolicy='menu-first-preserve-existing-entries';reboot=$false;createdAt=[DateTime]::UtcNow.ToString('o')}
    $plan.delete=$selection.delete; $plan.protectedPaths=@($Request.protectedPaths)
    $plan.bootMenu=$manifest.bootMenu; $plan.windowsBoot=$windowsBoot; $plan.bootPolicy='menu-first-preserve-existing-entries'
    if ($plan.bootOrderSha256 -ne $windowsBoot.bootOrderSha256) { Fail 'firmware_changed' 'The firmware order changed during planning.' }
    if ($null -ne $selection.delete) { $plan.preservedPartitions=@($disk.partitions | Where-Object { $_.guid -cne $selection.delete.partitionGuid }) }
    $path=Join-Path $directory 'plan.json'; Write-Json $path $plan
    Emit 'result' 'planned' @{planPath=$path;planSha256=(Sha $path);result=$plan}
}
function Invoke-DirectDeploy($Request) {
    Assert-Fields $Request @('operationId','planPath','planSha256','manifestPath','manifestSha256') @('planPath','planSha256','manifestPath','manifestSha256')
    Assert-Admin
    $manifest=Read-Manifest $Request.manifestPath $Request.manifestSha256
    $planPath=Assert-Path $Request.planPath; $directory=Split-Path -Parent $planPath; Assert-ProtectedDirectory $directory
    if ($directory -cne (Split-Path -Parent $Request.manifestPath) -or [IO.Path]::GetFileName($planPath) -cne 'plan.json' -or $Request.planSha256 -notmatch '^[0-9a-f]{64}$' -or (Sha $planPath) -ne $Request.planSha256) { Fail 'plan_mismatch' 'Confirmed plan changed or is outside the protected operation.' }
    $plan=Read-Json $planPath; $script:operationId=$manifest.operationId
    if ($plan.schemaVersion -ne 2 -or $plan.kind -ne 'omarchy-windows-alongside-plan' -or $plan.operationId -ne $script:operationId -or $plan.manifestSha256 -ne $Request.manifestSha256 -or $plan.encryption -ne 'luks2' -or $plan.protectionState -ne 'owner-setup-required' -or $plan.bootPath -cne '\EFI\limine\limine_x64.efi' -or $plan.bootPolicy -ne 'menu-first-preserve-existing-entries' -or $plan.reboot -ne $false -or @($plan.partitions).Count -ne 2 -or $plan.targetKind -notin @('free','shrink','delete') -or $plan.bitLocker.policy -ne 'suspend-active-os-protection-restore-verified' -or $plan.bitLocker.rebootLimit -ne 1) { Fail 'plan_unsupported' 'Approved plan violates the encrypted alongside deployment policy.' }
    Assert-AllocationBytes $plan.allocationBytes
    Assert-BootMenuEqual $plan.bootMenu $manifest.bootMenu
    Assert-WindowsBootUnchanged $plan.windowsBoot
    if (Test-Path -LiteralPath (Join-Path $directory 'deployment-started.json')) { Fail 'operation_used' 'This operation already started deployment; inspect its journal before recovery.' }
    $mutex=New-ProtectedMutex 'Global\OmarchyDirectX86Deployment'; $acquired=$false
    $recoveryContext=$null; $restoration=$null; $failure=$null; $restoreFailure=$null; $receipt=$null
    try {
        try { $acquired=$mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $acquired=$true }
        if (-not $acquired) { Fail 'deployment_busy' 'Another direct deployment is active.' }
        $probe=Get-Probe $plan.protectedPaths
        $selectionRequest=[pscustomobject]@{targetKind=$plan.targetKind;allocationBytes=$plan.allocationBytes;diskNumber=$plan.diskNumber;diskUniqueId=$plan.diskUniqueId;startOffsetBytes=$plan.partitions[0].offsetBytes;shrinkPartitionNumber=$(if ($null -ne $plan.shrink) { $plan.shrink.partitionNumber } else { $null });shrinkPartitionGuid=$(if ($null -ne $plan.shrink) { $plan.shrink.partitionGuid } else { $null })}
        if ($plan.targetKind -eq 'delete') {
            $selectionRequest | Add-Member -NotePropertyMembers @{deletePartitionNumber=$plan.delete.partitionNumber;deletePartitionGuid=$plan.delete.partitionGuid;deleteOffsetBytes=$plan.delete.offsetBytes;deleteSizeBytes=$plan.delete.sizeBytes;deleteConfirmation=$plan.delete.confirmation}
        }
        $selection=Select-Allocation $probe $selectionRequest; $disk=$selection.disk
        if ($selection.startOffsetBytes -ne $plan.partitions[0].offsetBytes -or $disk.sizeBytes -ne $plan.diskSizeBytes -or $disk.serialNumber -cne $plan.serialNumber -or $disk.logicalSectorBytes -ne $plan.logicalSectorBytes -or [Omarchy.DirectX86.NativeDisk]::LayoutHash($disk.diskNumber) -ne $plan.layoutSha256) { Fail 'target_changed' 'Disk geometry or the confirmed allocation changed.' }
        if ($plan.targetKind -eq 'shrink') {
            foreach ($field in @('partitionNumber','partitionGuid','volumeId','offsetBytes','beforeSizeBytes','afterSizeBytes','shrinkBytes','freedOffsetBytes','allocationBytes')) { if ([string]$selection.shrink.$field -cne [string]$plan.shrink.$field) { Fail 'shrink_plan_changed' "The confirmed NTFS shrink changed: $field" } }
        } elseif ($null -ne $plan.shrink) { Fail 'plan_unsupported' 'A free-space plan cannot contain a resize action.' }
        if ($plan.targetKind -eq 'delete') {
            foreach ($field in @('partitionNumber','partitionGuid','gptType','offsetBytes','sizeBytes','volumeId','fileSystem','label','confirmation','allocationBytes')) { if ([string]$selection.delete.$field -cne [string]$plan.delete.$field) { Fail 'delete_plan_changed' ('The confirmed deletion changed: '+$field) } }
        } elseif ($null -ne $plan.delete) { Fail 'plan_unsupported' 'This plan cannot contain a deletion action.' }
        $affected=@(Get-AffectedBitLocker $probe.bitLocker $disk.diskNumber)
        Assert-BitLockerUnchanged $plan.bitLocker.volumes $affected
        if ([Omarchy.DirectX86.NativeDisk]::FirmwarePreflight() -ne $plan.bootOrderSha256) { Fail 'firmware_changed' 'UEFI boot configuration changed after confirmation.' }
        Assert-WindowsBootUnchanged $plan.windowsBoot
        for ($i=0; $i -lt 2; $i++) {
            $role=@('esp','root')[$i]; $p=$plan.partitions[$i]; $o=@($manifest.outputs | Where-Object { $_.role -eq $role })[0]
            $targetSize=if ($role -eq 'esp') { $script:espBytes } else { [long]$plan.allocationBytes-$script:espBytes }
            if ($p.role -ne $role -or $p.partitionGuid -ne $o.partitionGuid -or $p.file -ne $o.file -or $p.sizeBytes -ne $targetSize -or $p.imageSizeBytes -ne $o.sizeBytes -or $p.sha256 -ne $o.sha256 -or ($i -eq 1 -and $p.offsetBytes -ne $plan.partitions[0].offsetBytes+$script:espBytes)) { Fail 'plan_mismatch' 'Plan extent or payload differs from the local encrypted image.' }
            $label=if ($role -eq 'esp') { 'boot files' } else { 'Omarchy system' }
            Emit 'progress' 'authenticating' @{message=('Checking the prepared '+$label+'…');role=$role}
            Assert-ArtifactHash $o $directory
        }
        if (Test-Path -LiteralPath (Join-Path $directory 'cancel.requested')) { Fail 'build_cancelled' 'Installation was cancelled before Windows changes.' }
        Assert-BitLockerUnchanged $plan.bitLocker.volumes @(Get-AffectedBitLocker (Get-BitLockerSnapshot) $disk.diskNumber)
        Assert-PhysicalDiskIdentity $plan
        $script:cancelAvailable=$false
        Emit 'progress' 'preparing-recovery' @{message='Preparing Windows recovery…'}
        $recoveryContext=New-BitLockerRecovery $plan
        Write-Json (Join-Path $directory 'deployment-started.json') @{operationId=$script:operationId;planSha256=$Request.planSha256;startedAt=[DateTime]::UtcNow.ToString('o');status='windows-changes-starting';targetKind=$plan.targetKind;recoveryState=$(if ($null -ne $recoveryContext) { $recoveryContext.statePath } else { $null })}
        $script:mutationStarted=$true
        if ($null -ne $recoveryContext) { Emit 'progress' 'suspending-bitlocker' @{message='Temporarily suspending BitLocker protection…'} }
        Suspend-PlannedBitLocker $recoveryContext
        Assert-PhysicalDiskIdentity $plan
        $layoutHash=$plan.layoutSha256
        if ($plan.targetKind -eq 'shrink') {
            Emit 'progress' 'shrinking-ntfs' @{message='Making space on the selected Windows partition…';beforeSizeBytes=$plan.shrink.beforeSizeBytes;afterSizeBytes=$plan.shrink.afterSizeBytes}
            $layoutHash=Invoke-ConfirmedShrink $plan $directory
            foreach ($p in @(Get-InspectedPartitions (Get-Disk -Number $disk.diskNumber -ErrorAction Stop))) {
                if ([long]$plan.partitions[0].offsetBytes -lt [long]$p.Offset+[long]$p.Size -and [long]$p.Offset -lt [long]$plan.partitions[0].offsetBytes+[long]$plan.allocationBytes) { Fail 'shrink_extent_occupied' 'The expected freed allocation is still occupied after the Windows resize.' }
            }
        }
        if ($plan.targetKind -eq 'delete') {
            Emit 'progress' 'deleting-partition' @{message=('Deleting the explicitly confirmed '+$plan.delete.confirmation+' and all its data.');partitionNumber=$plan.delete.partitionNumber;sizeBytes=$plan.delete.sizeBytes}
            $layoutHash=Invoke-ConfirmedDeletion $plan $directory
            foreach ($p in @(Get-InspectedPartitions (Get-Disk -Number $disk.diskNumber -ErrorAction Stop))) {
                if ([long]$plan.partitions[0].offsetBytes -lt [long]$p.Offset+[long]$p.Size -and [long]$p.Offset -lt [long]$plan.partitions[0].offsetBytes+[long]$plan.allocationBytes) { Fail 'delete_extent_occupied' 'The expected freed allocation is still occupied after deletion.' }
            }
        }
        Emit 'progress' 'allocating' @{message='Creating the Omarchy partitions…'}
        $numbers=[Omarchy.DirectX86.NativeDisk]::Allocate($disk.diskNumber,$layoutHash,$plan.partitions[0].offsetBytes,$plan.partitions[1].sizeBytes,[guid]$plan.partitions[0].partitionGuid,[guid]$plan.partitions[1].partitionGuid)
        Write-Json (Join-Path $directory 'partitions-created.json') @{operationId=$script:operationId;partitionNumbers=$numbers;partitions=$plan.partitions}
        for ($i=0; $i -lt 2; $i++) {
            $p=$plan.partitions[$i]; $o=@($manifest.outputs | Where-Object { $_.role -eq $p.role })[0]; $script:writingRole=$p.role
            $label=if ($p.role -eq 'esp') { 'boot files' } else { 'Omarchy system' }
            Emit 'progress' 'authenticating' @{message=('Opening the verified '+$label+'…');role=$p.role}
            $callback=[Action[string,long,long]] {
                param($phase,$done,$total)
                $label=if ($script:writingRole -eq 'esp') { 'boot files' } else { 'Omarchy system' }
                $message=switch ($phase) { 'writing' { 'Writing the '+$label+'…' } 'verifying' { 'Verifying the '+$label+'…' } 'flushing' { 'Flushing pending writes…' } default { 'Installing Omarchy…' } }
                Emit 'progress' $phase @{message=$message;role=$script:writingRole;completedBytes=$done;totalBytes=$total}
            }
            $plaintext=Open-Artifact $o $directory
            try { [Omarchy.DirectX86.NativeDisk]::WritePartition($disk.diskNumber,$numbers[$i],[guid]$p.partitionGuid,$p.offsetBytes,$p.sizeBytes,$p.imageSizeBytes,$plaintext,$p.sha256,$p.role,$callback) }
            finally { $plaintext.Dispose() }
            Write-Json (Join-Path $directory ($p.role+'-verified.json')) @{sha256=$p.sha256;writtenBytes=$p.imageSizeBytes;partitionBytes=$p.sizeBytes;partitionGuid=$p.partitionGuid;verifiedAt=[DateTime]::UtcNow.ToString('o')}
        }
        $windowAcquired=$false
        try {
            if ($null -ne $recoveryContext) {
                try { $windowAcquired=$recoveryContext.mutex.WaitOne(30000) } catch [Threading.AbandonedMutexException] { $windowAcquired=$true }
                if (-not $windowAcquired) { Fail 'recovery_busy' 'Windows protection recovery is active.' }
                $window=Read-Json $recoveryContext.statePath
                if ($window.completed -or [DateTime]::UtcNow -ge [DateTime]::Parse($window.deadlineUtc).ToUniversalTime()) { Fail 'suspension_expired' 'The bounded suspension window expired before firmware registration; Windows protection is restored next.' }
            }
            Assert-WindowsBootUnchanged $plan.windowsBoot
            Write-Json (Join-Path $directory 'boot-order-before.json') @{bootOrderBase64=$plan.windowsBoot.bootOrderBase64;bootOrderSha256=$plan.bootOrderSha256;windowsBoot=$plan.windowsBoot;bootMenu=$plan.bootMenu}
            Emit 'progress' 'registering-boot' @{message='Setting up the Omarchy and Windows boot menu…'}
            $bootEntry=[Omarchy.DirectX86.NativeDisk]::RegisterBoot($numbers[0],$plan.partitions[0].offsetBytes,$script:espBytes,[guid]$plan.partitions[0].partitionGuid,$plan.bootOrderSha256,$plan.windowsBoot.entrySha256)
        } finally { if ($windowAcquired) { $recoveryContext.mutex.ReleaseMutex() } }
        $receipt=[ordered]@{schemaVersion=2;operationId=$script:operationId;status='deployed';planSha256=$Request.planSha256;manifestSha256=$Request.manifestSha256;diskUniqueId=$disk.diskUniqueId;targetKind=$plan.targetKind;allocationBytes=$plan.allocationBytes;shrink=$plan.shrink;partitions=$plan.partitions;bootEntry=$bootEntry;bootPolicy='menu-first-preserve-existing-entries';readbackVerified=$true;encryption='luks2';protectionState='owner-setup-required';firstBoot='Choose Omarchy in the firmware boot menu. First boot grows LUKS/Btrfs, configures hardware and completes personal encryption and owner setup.';physicalBootVerified=$false;rebooted=$false}
    } catch { $failure=$_ }
    finally {
        try {
            if ($null -ne $recoveryContext) { Emit 'progress' 'restoring-bitlocker' @{message='Restoring and checking BitLocker protection…'} }
            $restoration=Restore-PlannedBitLocker $recoveryContext
        } catch { $restoreFailure=$_ }
        if ($null -ne $recoveryContext) { $recoveryContext.mutex.Dispose() }
        if ($acquired) { $mutex.ReleaseMutex() }
        $mutex.Dispose()
    }
    if ($null -ne $restoreFailure) { Fail 'bitlocker_restore_failed' ($restoreFailure.Exception.Message+' The protected SYSTEM recovery task remains active; Windows data was never decrypted.') }
    if ($null -ne $failure) { throw $failure }
    $remainingVolumes=@($plan.bitLocker.volumes)
    if ($plan.targetKind -eq 'delete') { $remainingVolumes=@($remainingVolumes | Where-Object { $_.partitionGuid -ine $plan.delete.partitionGuid -or $_.diskNumber -ne $plan.diskNumber }) }
    Assert-BitLockerUnchanged $remainingVolumes @(Get-AffectedBitLocker (Get-BitLockerSnapshot) $plan.diskNumber)
    $receipt.delete=$plan.delete
    $receipt.bootMenu=$plan.bootMenu; $receipt.windowsBoot=$plan.windowsBoot; $receipt.bootPolicy=$plan.bootPolicy
    $receipt.firstBoot='At normal startup the menu offers Omarchy and Windows, with the chosen default and countdown. Selecting Windows briefly restarts through its existing firmware entry. The first Omarchy boot completes hardware, encryption and owner setup.'
    $receipt.bitLockerRestoration=$restoration; $receipt.completedAt=[DateTime]::UtcNow.ToString('o')
    $path=Join-Path $directory 'deployment-receipt.json'; Write-Json $path $receipt
    Emit 'result' 'deployed' @{receiptPath=$path;result=$receipt}
}
