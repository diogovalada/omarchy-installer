function Assert-BootMenu($Menu) {
    if ($null -eq $Menu) { Fail 'boot_menu_missing' 'Review the Windows startup menu settings.' }
    Assert-Fields $Menu @('defaultOs','timeoutSeconds') @('defaultOs','timeoutSeconds')
    if ($Menu.defaultOs -cnotin @('omarchy','windows') -or ($Menu.timeoutSeconds -isnot [int] -and $Menu.timeoutSeconds -isnot [long]) -or $Menu.timeoutSeconds -notin @(5,10,15,30)) {
        Fail 'boot_menu_invalid' 'Choose Omarchy or Windows and a countdown of 5, 10, 15 or 30 seconds.'
    }
}
function Assert-BootMenuEqual($Expected,$Actual) {
    Assert-BootMenu $Expected; Assert-BootMenu $Actual
    if ($Expected.defaultOs -cne $Actual.defaultOs -or $Expected.timeoutSeconds -ne $Actual.timeoutSeconds) { Fail 'boot_menu_changed' 'The built image does not contain the approved startup menu settings.' }
}
function Get-VerifiedWindowsBoot {
    $target=[Omarchy.DirectX86.NativeDisk]::InspectWindowsBoot()
    $partitions=@(Get-Partition -ErrorAction Stop | Where-Object { $_.Guid -and ([guid]$_.Guid -eq [guid]$target.partitionGuid) })
    if ($partitions.Count -ne 1 -or -not $partitions[0].IsSystem -or [string]$partitions[0].GptType -ine '{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}' -or [long]$partitions[0].Offset -ne $target.offsetBytes -or [long]$partitions[0].Size -ne $target.sizeBytes -or $partitions[0].PartitionNumber -ne $target.partitionNumber) {
        Fail 'windows_boot_ambiguous' 'The Windows firmware entry must identify the current system EFI partition. This boot layout is not supported.'
    }
    $snapshot=Get-BitLockerSnapshot
    if (-not $snapshot.known) { Fail 'bitlocker_unknown' 'Windows encryption must be inspected before validating its boot route.' }
    $tpm=@($snapshot.volumes | Where-Object { $_.isOsVolume -and $_.conversionStatus -eq 1 } | ForEach-Object { $_.keyProtectors } | Where-Object { $_.type -in @(1,4,5,6) })
    $target.measuredBootSha256=if ($tpm.Count) { [Omarchy.DirectX86.NativeBootEvidence]::Inspect($target.partitionGuid,$target.partitionNumber,$target.offsetBytes,$target.sizeBytes) } else { 'not-required-without-os-tpm-protector' }
    return $target
}
function Assert-WindowsBootUnchanged($Expected) {
    $current=Get-VerifiedWindowsBoot
    foreach ($field in @('entryName','description','entrySha256','partitionGuid','partitionNumber','offsetBytes','sizeBytes','bootOrderSha256','bootOrderBase64','currentBootEntry','measuredBootSha256')) {
        if ([string]$current.$field -cne [string]$Expected.$field) { Fail 'windows_boot_changed' ('Windows startup configuration changed: '+$field) }
    }
}
