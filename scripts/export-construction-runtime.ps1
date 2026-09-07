# Maintainer packaging step; exports an existing pinned runtime, never pulls it.
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../providers/image-builder-x86'))
$runtime=Get-Content -LiteralPath (Join-Path $root 'runtime-lock.json') -Raw | ConvertFrom-Json
if ($runtime.imageId -cnotmatch '^sha256:[0-9a-f]{64}$') { throw 'Invalid runtime image pin.' }
$archive=Join-Path $root 'runtime.tar'
$descriptor=Join-Path $root 'runtime-distribution.json'
if (Test-Path -LiteralPath $archive) {
    if (-not (Test-Path -LiteralPath $descriptor)) { throw 'An unrecorded runtime archive already exists; inspect it before replacing.' }
    $record=Get-Content -LiteralPath $descriptor -Raw | ConvertFrom-Json
    if ($record.imageId -cne $runtime.imageId -or $record.sizeBytes -ne (Get-Item -LiteralPath $archive).Length -or $record.sha256 -cne (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Existing runtime archive differs from its pin. Inspect it before replacing.' }
    Write-Output 'Verified the existing packaged runtime archive.'
    return
}
$docker=Join-Path $env:ProgramFiles 'Docker/Docker/resources/bin/docker.exe'
$identity=& $docker image inspect $runtime.imageId --format '{{.Id}}'
if ($LASTEXITCODE -ne 0 -or ($identity -join '').Trim() -cne $runtime.imageId) { throw 'Build and qualify the pinned runtime before packaging.' }
$partial=Join-Path $root ('runtime-'+[guid]::NewGuid().ToString('N')+'.partial')
try {
    & $docker image save --output $partial $runtime.imageId
    if ($LASTEXITCODE -ne 0) { throw 'Runtime export failed.' }
    $length=(Get-Item -LiteralPath $partial).Length
    if ($length -le 0 -or $length -gt 2GB) { throw 'Runtime archive exceeds its packaging bound.' }
    $hash=(Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::Move($partial,$archive)
    $value=[ordered]@{schemaVersion=1;imageId=$runtime.imageId;archive='runtime.tar';sizeBytes=$length;sha256=$hash}
    [IO.File]::WriteAllText($descriptor,($value | ConvertTo-Json)+"`n",(New-Object Text.UTF8Encoding($false)))
    Write-Output "Packaged $length bytes of pinned construction runtime."
} finally {
    # One verified workspace file, never a directory or a shell-built delete.
    if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial -Force }
}
