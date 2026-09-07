# Fixed user-requested firmware restart. Does not disable Secure Boot or TPM,
# alter boot entries, accept an executable, or touch partition tables.
function Get-FirmwarePreparation {
    Assert-Admin
    if ([Omarchy.DirectX86.NativeDisk]::Firmware() -ne 'uefi' -or -not (Confirm-SecureBootUEFI -ErrorAction Stop)) {
        Fail 'firmware_preparation_unneeded' 'Secure Boot is already off, or this is not a supported UEFI computer. Refresh the installation requirements.'
    }
    if (-not [Omarchy.DirectX86.NativeDisk]::CanRestartToFirmware()) { Fail 'firmware_restart_unsupported' 'This firmware does not support restarting into its settings from Windows. Use the manufacturer startup key and prepare BitLocker with your administrator.' }
    $snapshot=Get-BitLockerSnapshot
    $osVolumes=@($snapshot.volumes | Where-Object { $_.isOsVolume })
    if (-not $snapshot.known -or $osVolumes.Count -ne 1) { Fail 'firmware_encryption_unknown' 'Exactly one identified Windows OS volume is required for firmware preparation.' }
    $v=$osVolumes[0]
    if ($v.conversionStatus -notin @(0,1) -or $v.lockStatus -ne 0 -or $v.protectionStatus -notin @(0,1) -or -not $v.partitionGuid -or -not $v.diskUniqueId) {
        Fail 'firmware_encryption_unknown' 'Windows must be fully encrypted or unencrypted, unlocked, and on an identified disk before firmware preparation.'
    }
    if ($v.conversionStatus -eq 1 -and ($v.keyProtectorIds.Count -eq 0 -or @($v.keyProtectors).Count -ne @($v.keyProtectorIds).Count -or @($v.keyProtectors | Where-Object { $_.type -notin @(1,2,3,4,5,6,8) }).Count)) {
        Fail 'firmware_protectors_unsupported' 'The Windows key protectors cannot be safely identified for this firmware preparation.'
    }
    # PCR changes are the purpose of this separate transition. Deployment's
    # stricter profile policy is evaluated after the next Windows startup.
    return [ordered]@{schemaVersion=1;secureBoot='enabled';bootTimeUtc=(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o');volumes=$osVolumes;reboot='uefi-settings';restore='next-windows-start-or-five-minute-cancel-deadline'}
}
function Invoke-FirmwarePreparation($Request) {
    Assert-Fields $Request @('operationId','expectedSha256') @('operationId','expectedSha256')
    $script:operationId=([guid]$Request.operationId).ToString()
    $facts=Get-FirmwarePreparation
    $digest=[Omarchy.DirectX86.NativeDisk]::Hash((New-Object Text.UTF8Encoding($false)).GetBytes(($facts | ConvertTo-Json -Depth 20 -Compress)))
    if ($Request.expectedSha256 -cne $digest) { Fail 'firmware_state_changed' 'Windows firmware or encryption state changed after the restart was reviewed. Inspect again.' }
    $mutex=New-ProtectedMutex 'Global\OmarchyDirectX86Deployment'; $acquired=$false; $context=$null; $restarting=$false
    try {
        try { $acquired=$mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $acquired=$true }
        if (-not $acquired) { Fail 'deployment_busy' 'Another installation or firmware preparation is active.' }
        $script:cancelAvailable=$false
        $context=New-BitLockerRecovery ([pscustomobject]@{bitLocker=[pscustomobject]@{volumes=$facts.volumes}})
        Suspend-PlannedBitLocker $context
        if ($null -ne $context) {
            $held=$false
            try {
                try { $held=$context.mutex.WaitOne(30000) } catch [Threading.AbandonedMutexException] { $held=$true }
                if (-not $held) { Fail 'recovery_busy' 'Windows protection recovery is active.' }
                $record=Read-Json $context.statePath
                $record.phase='firmware-restart'; $record.deadlineUtc=[DateTime]::UtcNow.AddMinutes(5).ToString('o')
                $record | Add-Member -NotePropertyName firmwareBootTimeUtc -NotePropertyValue $facts.bootTimeUtc
                Write-DurableJson $context.statePath $record
            } finally { if ($held) { $context.mutex.ReleaseMutex() } }
        }
        # No /f: do not force-close applications with unsaved work. If Windows
        # cancels/blocks this restart, the short watchdog deadline restores it.
        $shutdown=Join-Path ([Environment]::GetFolderPath('Windows')) 'System32/shutdown.exe'
        & $shutdown /r /fw /t 0
        if ($LASTEXITCODE -ne 0) { Fail 'firmware_restart_failed' 'Windows could not schedule the firmware restart. Protection is being restored.' }
        $restarting=$true
        Emit 'result' 'firmware-restart' @{result=@{restartRequested=$true;message='In firmware settings, disable Secure Boot and leave TPM enabled. Save, return to Windows, reopen Omarchy Setup and check with administrator access.'}}
    } finally {
        if (-not $restarting -and $null -ne $context) { [void](Restore-PlannedBitLocker $context) }
        if ($null -ne $context) { $context.mutex.Dispose() }
        if ($acquired) { $mutex.ReleaseMutex() }
        $mutex.Dispose()
    }
}
