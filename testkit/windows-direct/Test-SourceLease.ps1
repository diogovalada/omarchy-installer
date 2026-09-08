# Disposable regular files only. No administrator access, disks or firmware.
param([switch]$Container)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
Add-Type -Path (Join-Path $root 'providers/direct-x86/NativeSource.cs')
$directory = Join-Path ([IO.Path]::GetTempPath()) ('omarchy-source-test-' + [guid]::NewGuid().ToString())
[void][IO.Directory]::CreateDirectory($directory)
$path = Join-Path $directory 'image.iso'
$other = Join-Path $directory 'other.iso'
[IO.File]::WriteAllBytes($path, [byte[]](1,2,3))
[IO.File]::WriteAllBytes($other, [byte[]](1,2,3))
$lease = $null
$second = $null
try {
    $lease = New-Object Omarchy.DirectX86.NativeSource($path,3)
    $lease.Match($lease.VolumeSerial,$lease.FileIndex)
    $second = New-Object Omarchy.DirectX86.NativeSource($other,3)
    $rejected = $false
    try { $second.Match($lease.VolumeSerial,$lease.FileIndex) } catch { $rejected = $true }
    if (-not $rejected) { throw 'Different files shared a source identity.' }
    $rejected = $false
    try { $writer = [IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::Write,[IO.FileShare]::ReadWrite); $writer.Dispose() } catch [IO.IOException] { $rejected = $true }
    if (-not $rejected) { throw 'The verified ISO was writable.' }
    $rejected = $false
    try { [IO.Directory]::Move($directory,($directory+'-moved')) } catch [IO.IOException] { $rejected = $true }
    if (-not $rejected) { throw 'The held source parent was movable.' }
    $lease.Dispose(); $lease = $null
    [IO.File]::WriteAllBytes($path,[byte[]](3,2,1))
    if ($Container) {
        $lease = New-Object Omarchy.DirectX86.NativeSource($path,3)
        $signature = Join-Path $directory 'image.iso.sig'
        $output = Join-Path $directory 'output'
        [IO.File]::WriteAllText($signature,'fixture signature')
        [void][IO.Directory]::CreateDirectory($output)
        try {
            $runtime = Get-Content -LiteralPath (Join-Path $root 'providers/image-builder-x86/runtime-lock.json') -Raw | ConvertFrom-Json
            $proof = Join-Path $root 'providers/image-builder-x86'
            $testScript = Join-Path $PSScriptRoot 'source_lease_container.py'
            $frame = @{protocolVersion=3;stagingKey=[Convert]::ToBase64String((New-Object byte[] 32));verifiedSource=@{kind='windows-held-readonly-bind-v1';sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant();length=3;signatureSha256=(Get-FileHash -LiteralPath $signature -Algorithm SHA256).Hash.ToLowerInvariant()}} | ConvertTo-Json -Depth 4 -Compress
            $frame | & 'C:\Program Files\Docker\Docker\resources\bin\docker.exe' run -i --rm --network none --read-only --cap-drop ALL --security-opt no-new-privileges --pids-limit 64 --memory 512m --tmpfs /tmp:rw,nosuid,nodev,size=64m --user 1000:1000 --mount "type=bind,source=$proof,target=/proof,readonly" --mount "type=bind,source=$path,target=/input/omarchy-test.iso,readonly" --mount "type=bind,source=$signature,target=/input/omarchy-test.iso.sig,readonly" --mount "type=bind,source=$output,target=/output" --mount "type=bind,source=$testScript,target=/test.py,readonly" --entrypoint python3 $runtime.imageId -B /test.py
            if ($LASTEXITCODE -ne 0) { throw 'Container source handoff failed.' }
        } finally {
            [IO.File]::Delete((Join-Path $output 'signature-verification.txt'))
            [IO.Directory]::Delete($output)
            [IO.File]::Delete($signature)
        }
    }
    foreach ($script in Get-ChildItem -LiteralPath (Join-Path $root 'providers/direct-x86') -Filter '*.ps1') {
        $tokens = $null; $parseErrors = $null
        [void][Management.Automation.Language.Parser]::ParseFile($script.FullName,[ref]$tokens,[ref]$parseErrors)
        if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
    }
    'PASS: source identity, deny-write lifetime, ancestor lock and release'
} finally {
    if ($null -ne $lease) { $lease.Dispose() }
    if ($null -ne $second) { $second.Dispose() }
    # Exact files created above, followed by the now-empty test directory.
    [IO.File]::Delete($path)
    [IO.File]::Delete($other)
    [IO.Directory]::Delete($directory)
}
