[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$proofRoot = [IO.Path]::GetFullPath($PSScriptRoot)
$cacheRoot = Join-Path $proofRoot '.cache'
New-Item -ItemType Directory -Force -Path $cacheRoot | Out-Null
foreach ($path in @($proofRoot, $cacheRoot)) {
    $item = Get-Item -LiteralPath $path -Force
    while ($null -ne $item) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Runtime build paths cannot contain reparse points.' }
        $item = $item.Parent
    }
}
if ((Get-Item -LiteralPath $cacheRoot).PSDrive.Free -lt 2GB) { throw 'Runtime build requires at least 2 GiB free disk space.' }
$pins = Get-Content -LiteralPath (Join-Path $proofRoot 'runtime-packages.lock')
if (-not $pins -or @($pins | Where-Object { $_ -notmatch '^[a-z0-9][a-z0-9+.:-]*=[0-9a-zA-Z+.:~_-]+$' }).Count) { throw 'Invalid package lock.' }
$idPath = Join-Path $cacheRoot 'runtime-image-id.txt'
docker build --tag omarchy-local-image-proof:4.0.2 --iidfile $idPath $proofRoot
if ($LASTEXITCODE -ne 0) { throw 'Runtime build failed; existing runtime lock was not replaced.' }
$imageId = (Get-Content -LiteralPath $idPath -Raw).Trim()
if ($imageId -notmatch '^sha256:[0-9a-f]{64}$') { throw 'Build did not produce an immutable image ID.' }
$inventory = docker run --rm --network none --read-only --cap-drop ALL --security-opt no-new-privileges --memory 128m --entrypoint cat $imageId /runtime-packages.tsv
if ($LASTEXITCODE -ne 0) { throw 'Could not inspect runtime package inventory.' }
$inventory | Set-Content -Encoding utf8NoBOM -LiteralPath (Join-Path $cacheRoot 'runtime-packages.tsv')
$lock = [ordered]@{
    schemaVersion=1; imageId=$imageId
    observedAt=(Get-Date).ToUniversalTime().ToString('o')
    baseImage='ubuntu@sha256:4fbb8e6a8395de5a7550b33509421a2bafbc0aab6c06ba2cef9ebffbc7092d90'
    packageIntegrity='APT signed repository metadata and package hashes; all added/changed package versions pinned'
    dockerfileSha256=(Get-FileHash -LiteralPath (Join-Path $proofRoot 'Dockerfile')).Hash.ToLowerInvariant()
    packageLockSha256=(Get-FileHash -LiteralPath (Join-Path $proofRoot 'runtime-packages.lock')).Hash.ToLowerInvariant()
    model='gpt-6-astra'
}
$lock | ConvertTo-Json | Set-Content -Encoding utf8NoBOM -LiteralPath (Join-Path $proofRoot 'runtime-lock.json')
Write-Output "Pinned local runtime $imageId. Run smoke and inspect to qualify this build."
