# Private memory-only key input and strict encrypted artifact manifest reader.
function Read-StagingInput {
    $chars=New-Object Text.StringBuilder
    while ($true) {
        $n=[Console]::In.Read()
        if ($n -lt 0) { Fail 'staging_key_missing' 'The authenticated helper did not send the private staging-key frame.' }
        if ($n -eq 10) { break }
        if ($chars.Length -ge 512) { Fail 'staging_key_invalid' 'Private input frame exceeds its fixed bound.' }
        [void]$chars.Append([char]$n)
    }
    try { $frame=$chars.ToString().TrimEnd([char]13) | ConvertFrom-Json }
    catch { Fail 'staging_key_invalid' 'Private input frame is not valid JSON.' }
    $names=@($frame.PSObject.Properties | ForEach-Object { $_.Name })
    if ($names.Count -ne 2 -or $names -notcontains 'protocolVersion' -or $names -notcontains 'stagingKey') { Fail 'staging_key_invalid' 'Private input frame has unexpected fields.' }
    if ($frame.protocolVersion -ne 2 -or $frame.stagingKey -notmatch '^[A-Za-z0-9+/]{43}=$') { Fail 'staging_key_invalid' 'Private input frame has an unsupported shape.' }
    $script:stagingKey=[Convert]::FromBase64String($frame.stagingKey)
    if ($script:stagingKey.Length -ne 32) { Fail 'staging_key_invalid' 'Staging key must have exactly 32 bytes.' }
    # This string is forwarded only to the private Docker stdin pipe. It is
    # never written to JSON artifacts, argv, environment variables or logs.
    $script:stagingFrame=(@{protocolVersion=2;stagingKey=$frame.stagingKey} | ConvertTo-Json -Compress)
    $frame=$null; [void]$chars.Clear()
}
function Open-Artifact($Output,[string]$Directory) {
    $e=$Output.envelope
    return [Omarchy.DirectX86.EncryptedImage]::Open((Join-Path $Directory $Output.file),[long]$Output.sizeBytes,[long]$e.storedSizeBytes,[string]$e.storedSha256,[string]$e.hmacSha256,$script:stagingKey)
}
function Assert-ArtifactHash($Output,[string]$Directory) {
    $e=$Output.envelope
    [Omarchy.DirectX86.EncryptedImage]::ValidateHash((Join-Path $Directory $Output.file),[long]$Output.sizeBytes,[long]$e.storedSizeBytes,[string]$e.storedSha256,[string]$e.hmacSha256,[string]$Output.sha256,$script:stagingKey)
}
function Read-Manifest([string]$Path,[string]$ExpectedHash) {
    $Path=Assert-Path $Path
    if ([IO.Path]::GetFileName($Path) -cne 'manifest.json' -or $ExpectedHash -notmatch '^[0-9a-f]{64}$' -or (Sha $Path) -ne $ExpectedHash) { Fail 'manifest_mismatch' 'The construction manifest differs from this authenticated operation.' }
    $m=Read-Json $Path; $release=Read-Json (Join-Path $script:builderRoot 'release-lock.json'); $runtime=Get-Runtime
    if ($m.schemaVersion -ne 2 -or $m.kind -ne 'omarchy-local-direct-image' -or $m.encryption -ne 'luks2' -or $m.logicalSectorBytes -ne 512 -or $m.minimumUnallocatedBytes -ne $script:minimumBytes -or $m.bootPath -cne '\EFI\limine\limine_x64.efi') { Fail 'manifest_unsupported' 'Only the pinned LUKS2 construction manifest and boot layout are accepted.' }
    if ($m.source.sha256 -ne $release.sha256 -or $m.source.sizeBytes -ne $release.sizeBytes -or $m.source.signingKeyFingerprint -ne $release.signingKeyFingerprint -or $m.source.version -ne $release.version -or $m.runtimeLock.imageId -ne $runtime.imageId) { Fail 'manifest_provenance' 'Image source/runtime does not match packaged pins.' }
    $required=@('product_builder.py','product-guest.sh','product-firstboot.sh','boot_menu.py','builder.py','configuration.py','release-lock.json','runtime-lock.json','Dockerfile','runtime-packages.lock','omarchy-signing-key.gpg','product-validate.py','product-source-lock.json','product_nbd.py','artifact_crypto.py')
    Assert-Fields $m.recipeFiles $required $required
    foreach ($name in $required) { if ($m.recipeFiles.$name -ne (Sha (Join-Path $script:builderRoot $name))) { Fail 'manifest_provenance' "The image was constructed with different recipe bytes: $name" } }
    $id=$m.identity
    Assert-BootMenuEqual $m.bootMenu $id.bootMenu
    if ($id.bootMenuUpdateHook -ne $true -or $id.windowsBootProtocol -cne 'efi_boot_entry') { Fail 'manifest_boot_menu' 'The built image lacks the persistent, firmware-based Windows startup menu.' }
    if (-not $m.qualification.constructionCompleted -or $id.operationId -ne $m.operationId -or $id.ownerProvisioning -ne 'pending' -or $id.encryption -ne 'luks2' -or $id.buildCredentials -ne $false -or $id.portableInitramfs -ne $true -or $id.physicalHardwareSetup -ne 'upstream-on-first-boot' -or $id.offlineRepositoryIncluded -ne $true -or $id.machineIdentity -ne 'generate-on-first-boot' -or $id.bootstrapKey -ne 'upstream-per-install' -or $id.protectionState -ne 'owner-setup-required' -or $id.firstBootGrowth -ne 'luks-mapping-and-btrfs' -or $id.ownerRekey -ne 'passphrase-slots-only') { Fail 'manifest_identity' 'Image does not meet deferred ownership, encryption, portability and bootstrap invariants.' }
    [void][guid]::Parse($m.operationId); $luksUuid=[guid]::Parse($id.rootLuksUuid)
    if ($luksUuid -eq [guid]::Empty -or @($m.outputs).Count -ne 2) { Fail 'manifest_outputs' 'Exactly two unique credential-free partition exports are required.' }
    $directory=Split-Path -Parent $Path
    foreach ($role in @('esp','root')) {
        $matches=@($m.outputs | Where-Object { $_.role -ceq $role })
        if ($matches.Count -ne 1) { Fail 'manifest_outputs' 'Duplicate or missing partition role.' }
        $o=$matches[0]; $size=if ($role -eq 'esp') { $script:espBytes } else { $script:rootBytes }; $fs=if ($role -eq 'esp') { 'fat32' } else { 'crypto_LUKS' }
        if ($o.file -cne ($role+'.img.enc') -or $o.sizeBytes -ne $size -or $o.filesystem -cne $fs -or $o.sha256 -notmatch '^[0-9a-f]{64}$') { Fail 'manifest_outputs' 'Unexpected encrypted partition filename, format, size or digest.' }
        $guid=[guid]::Parse($o.partitionGuid)
        if ($guid -eq [guid]::Empty -or [string]$guid -ne $id.($role+'PartitionGuid')) { Fail 'manifest_outputs' 'Partition identity does not match this construction.' }
        $e=$o.envelope
        if ($e.format -cne 'openssl-aes-256-cbc-pbkdf2-sha256' -or $e.iterations -ne 100000 -or $e.storedSizeBytes -ne $size+32 -or $e.storedSha256 -notmatch '^[0-9a-f]{64}$' -or $e.hmacSha256 -notmatch '^[0-9a-f]{64}$') { Fail 'artifact_envelope' 'Unsupported or unauthenticated encrypted artifact envelope.' }
        $image=Assert-Path (Join-Path $directory $o.file)
        if ((Get-Item -LiteralPath $image).Length -ne $e.storedSizeBytes) { Fail 'artifact_changed' 'Encrypted artifact length changed.' }
    }
    if ($script:Action -in @('plan','deploy')) {
        $root=@($m.outputs | Where-Object { $_.role -eq 'root' })[0]
        $plaintext=Open-Artifact $root $directory
        try {
            if ([Omarchy.DirectX86.EncryptedImage]::InspectLuksHeader($plaintext) -ine $id.rootLuksUuid) { Fail 'luks_header_mismatch' 'Actual LUKS2 header UUID differs from the authenticated build manifest.' }
        } finally { $plaintext.Dispose() }
    }
    return $m
}
