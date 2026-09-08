# Imports only a release-packaged, digest-bound local archive. No network,
# arbitrary image name, Dockerfile, build arguments or host-feature installation.
function Get-RuntimeDistribution($ArchiveEvidence=$null) {
    $descriptor=Join-Path $script:builderRoot 'runtime-distribution.json'
    if (-not (Test-Path -LiteralPath $descriptor)) { return $null }
    $value=Read-Json $descriptor; $runtime=Get-Runtime
    Assert-Fields $value @('schemaVersion','imageId','archive','sizeBytes','sha256') @('schemaVersion','imageId','archive','sizeBytes','sha256')
    if ($value.schemaVersion -ne 1 -or $value.imageId -cne $runtime.imageId -or $value.archive -cne 'runtime.tar' -or $value.sizeBytes -le 0 -or $value.sizeBytes -gt 256MB -or $value.sha256 -cnotmatch '^[0-9a-f]{64}$') {
        Fail 'runtime_distribution_invalid' 'The packaged construction runtime descriptor does not match its pinned image.'
    }
    if ($null -ne $ArchiveEvidence) {
        # Read-only probe: compare the helper's compiled manifest record with
        # the staged descriptor. Import never uses this metadata-only path.
        Assert-Fields $ArchiveEvidence @('length','sha256') @('length','sha256')
        if ($ArchiveEvidence.length -ne $value.sizeBytes -or $ArchiveEvidence.sha256 -cne $value.sha256) {
            Fail 'runtime_distribution_invalid' 'The packaged construction runtime descriptor does not match the compiled archive record.'
        }
    } else {
        $archive=Assert-Path (Join-Path $script:builderRoot 'runtime.tar')
        if ((Get-Item -LiteralPath $archive).Length -ne $value.sizeBytes) { Fail 'runtime_distribution_invalid' 'The packaged construction runtime archive is incomplete.' }
    }
    return $value
}
function Invoke-RuntimePreparation($Request) {
    Assert-Fields $Request @() @()
    Assert-Admin
    $docker=Get-Docker; $runtime=Get-Runtime
    $engine=& $docker info --format '{{.OSType}}' 2>$null
    if ($LASTEXITCODE -ne 0 -or ($engine -join '').Trim() -ne 'linux') { Fail 'linux_engine_required' 'Start Docker Desktop with its Linux engine, then retry Prepare runtime.' }
    $existing=& $docker image inspect $runtime.imageId --format '{{.Id}}' 2>$null
    if ($LASTEXITCODE -eq 0 -and ($existing -join '').Trim() -ceq $runtime.imageId) {
        Emit 'result' 'runtime-ready' @{result=@{runtimeReady=$true;alreadyPresent=$true}}; return
    }
    $distribution=Get-RuntimeDistribution
    if ($null -eq $distribution) { Fail 'runtime_not_packaged' 'This development build has no packaged runtime archive. Use a release package containing the verified construction runtime.' }
    $archive=Assert-Path (Join-Path $script:builderRoot 'runtime.tar')
    if ((Sha $archive) -cne $distribution.sha256) { Fail 'runtime_archive_changed' 'The construction runtime archive failed verification.' }
    $script:cancelAvailable=$false
    Emit 'progress' 'runtime-import' @{message='Importing the verified local construction runtime. No host features are changed.'}
    $import=& $docker image load --input $archive 2>&1
    if ($LASTEXITCODE -ne 0) { Fail 'runtime_import_failed' 'Docker could not import the runtime. Check Docker storage and its Linux engine, then retry.' }
    $image=& $docker image inspect $runtime.imageId --format '{{.Id}}' 2>$null
    if ($LASTEXITCODE -ne 0 -or ($image -join '').Trim() -cne $runtime.imageId) { Fail 'runtime_import_unverified' 'Docker did not report the exact pinned image after import.' }
    Emit 'result' 'runtime-ready' @{result=@{runtimeReady=$true;alreadyPresent=$false;imageId=$runtime.imageId}}
}
