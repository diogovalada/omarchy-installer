[CmdletBinding()]
param(
    [switch]$UnsignedPreview,
    [switch]$DebugBuild,
    [switch]$ReuseVerifiedBuild,
    [string]$VerifiedPortableRecord
)
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ($env:OS -ne 'Windows_NT') { throw 'Windows packaging requires Windows.' }
if (-not $UnsignedPreview) { throw 'Portable packaging currently produces unsigned previews. Explicitly pass -UnsignedPreview.' }
if ($VerifiedPortableRecord -and -not $ReuseVerifiedBuild) { throw 'A prior portable record is only used with -ReuseVerifiedBuild.' }
$buildProfile=if ($DebugBuild) { 'debug' } else { 'release' }
if (-not $ReuseVerifiedBuild) {
    & (Join-Path $PSScriptRoot 'export-construction-runtime.ps1')
    $arguments=@('--dir',(Join-Path $root 'apps/desktop'),'exec','tauri','build','--no-bundle')
    if ($DebugBuild) { $arguments += '--debug' }
    & pnpm @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Portable application build failed.' }
}
$nsisCompiler=Join-Path $env:LOCALAPPDATA 'tauri/NSIS/makensis.exe'
if (-not (Test-Path -LiteralPath $nsisCompiler)) { throw 'The packaging machine requires the verified NSIS compiler supplied by the Tauri toolchain.' }
& cargo build --manifest-path (Join-Path $PSScriptRoot 'portable-cache-helper/Cargo.toml') --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Portable cache verifier build failed.' }
$cacheHelper=Join-Path $PSScriptRoot 'portable-cache-helper/target/release/omarchy-portable-cache.exe'
$prepared=& node (Join-Path $PSScriptRoot 'stage-windows-portable.mjs') $buildProfile $(if($ReuseVerifiedBuild){'reuse'}else{'built'}) $VerifiedPortableRecord
if ($LASTEXITCODE -ne 0) { throw 'Portable staging failed.' }
$stage=($prepared -join "`n") | ConvertFrom-Json
$stagedNode=Join-Path (Join-Path $stage.payloadDirectory 'providers') $stage.nodeRelative
& $stagedNode (Join-Path $PSScriptRoot 'verify-staged-media.cjs') (Split-Path -Parent $stagedNode)
if ($LASTEXITCODE -ne 0) { throw 'Staged media native-module/file-write verification failed.' }
$nsisArguments=@('/V2',('/DPAYLOAD_DIR='+$stage.payloadDirectory),('/DOUTPUT_FILE='+$stage.executablePath),('/DCACHE_HELPER='+$cacheHelper),('/DCACHE_MANIFEST='+$stage.cacheManifestPath),('/DCACHE_HASH='+$stage.cacheHash),('/DCACHE_ID='+$stage.cacheId),('/DAPP_ICON='+(Join-Path $root 'apps/desktop/src-tauri/icons/icon.ico')),(Join-Path $PSScriptRoot 'windows-portable-launcher.nsi'))
& $nsisCompiler @nsisArguments
if ($LASTEXITCODE -ne 0) { throw 'Portable executable creation failed.' }
# This fixed mode extracts and hashes its own payload, without opening the app
# or invoking host inspection, network access, firmware or an elevated helper.
$verificationTimer=[Diagnostics.Stopwatch]::StartNew()
$verification=Start-Process -FilePath $stage.executablePath -ArgumentList '--verify-bundle' -WindowStyle Hidden -Wait -PassThru
$verificationTimer.Stop()
if ($verification.ExitCode -ne 0) { throw 'Portable executable extraction/hash verification failed.' }
$record=Get-Content -LiteralPath $stage.recordPath -Raw | ConvertFrom-Json
$record.cache | Add-Member -NotePropertyName verifierSha256 -NotePropertyValue (Get-FileHash -LiteralPath $cacheHelper -Algorithm SHA256).Hash.ToLowerInvariant()
$record.cache | Add-Member -NotePropertyName verifierCargoLockSha256 -NotePropertyValue (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'portable-cache-helper/Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
$record.cache | Add-Member -NotePropertyName verifierSourceSha256 -NotePropertyValue (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'portable-cache-helper/src/main.rs') -Algorithm SHA256).Hash.ToLowerInvariant()
$launcher=[ordered]@{path=$stage.executablePath;sizeBytes=(Get-Item -LiteralPath $stage.executablePath).Length;sha256=(Get-FileHash -LiteralPath $stage.executablePath -Algorithm SHA256).Hash.ToLowerInvariant();signatureStatus=[string](Get-AuthenticodeSignature -LiteralPath $stage.executablePath).Status;extractionAndHashesVerified=$true;extractionAndHashSeconds=$verificationTimer.Elapsed.TotalSeconds;stagedMediaFileWriteVerified=$true;normalApplicationLaunchTested=$false}
$record | Add-Member -NotePropertyName launcher -NotePropertyValue $launcher
[IO.File]::WriteAllText($stage.recordPath,($record | ConvertTo-Json -Depth 12)+"`n",(New-Object Text.UTF8Encoding($false)))
Write-Output ("Portable executable: "+$stage.executablePath)
