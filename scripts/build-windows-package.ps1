[CmdletBinding()]
param(
    [string]$CertificateThumbprint,
    [string]$TimestampUrl,
    [switch]$UnsignedPreview,
    [switch]$DebugBuild
)
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if ($env:OS -ne 'Windows_NT') { throw 'Windows packaging requires Windows.' }
if (-not $UnsignedPreview -and ($CertificateThumbprint -cnotmatch '^[0-9a-fA-F]{40}$' -or $TimestampUrl -notmatch '^https://')) {
    throw 'Provide a code-signing certificate thumbprint and HTTPS timestamp service, or explicitly select -UnsignedPreview.'
}
if ($UnsignedPreview -and ($CertificateThumbprint -or $TimestampUrl)) { throw 'Choose signed packaging or an unsigned preview, not both.' }
& (Join-Path $PSScriptRoot 'export-construction-runtime.ps1')
$config=Get-Content -LiteralPath (Join-Path $root 'apps/desktop/src-tauri/tauri.windows-package.conf.json') -Raw | ConvertFrom-Json
if ($DebugBuild) { $config.bundle.windows.nsis | Add-Member -NotePropertyName compression -NotePropertyValue 'zlib' }
if (-not $UnsignedPreview) {
    $cert=Get-Item -LiteralPath ('Cert:\CurrentUser\My\'+$CertificateThumbprint) -ErrorAction Stop
    if (-not $cert.HasPrivateKey -or $cert.NotAfter -le (Get-Date) -or @($cert.EnhancedKeyUsageList | Where-Object { $_.ObjectId.Value -eq '1.3.6.1.5.5.7.3.3' }).Count -eq 0) { throw 'A valid code-signing certificate with its accessible private key is required.' }
    $config.bundle.windows | Add-Member -NotePropertyMembers @{certificateThumbprint=$CertificateThumbprint;digestAlgorithm='sha256';timestampUrl=$TimestampUrl;tsp=$true}
}
$out=Join-Path $root 'artifacts/windows-package'
New-Item -ItemType Directory -Force -Path $out | Out-Null
$configPath=Join-Path $out 'tauri-package-config.json'
[IO.File]::WriteAllText($configPath,($config | ConvertTo-Json -Depth 16)+"`n",(New-Object Text.UTF8Encoding($false)))
$arguments=@('--dir',(Join-Path $root 'apps/desktop'),'exec','tauri','build','--config',$configPath)
if ($DebugBuild) { $arguments += '--debug' }
& pnpm @arguments
if ($LASTEXITCODE -ne 0) { throw 'Windows package build failed.' }
$buildProfile=if ($DebugBuild) { 'debug' } else { 'release' }
$bundle=Join-Path $root "apps/desktop/src-tauri/target/$buildProfile/bundle/nsis"
$packages=@(Get-ChildItem -LiteralPath $bundle -Filter '*.exe' -File)
if ($packages.Count -ne 1) { throw 'Expected exactly one NSIS package; inspect the bundle output.' }
$exe=Join-Path $root "apps/desktop/src-tauri/target/$buildProfile/omarchy-setup-desktop.exe"
$files=@()
foreach ($path in @($exe,$packages[0].FullName)) {
    $signature=Get-AuthenticodeSignature -LiteralPath $path
    if (-not $UnsignedPreview -and ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ine $CertificateThumbprint)) { throw 'The application or installer signature did not verify against the requested signer.' }
    $files += [ordered]@{path=$path;sizeBytes=(Get-Item -LiteralPath $path).Length;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant();signatureStatus=[string]$signature.Status}
}
$record=[ordered]@{schemaVersion=1;model='gpt-6-astra';createdAt=[DateTime]::UtcNow.ToString('o');profile=$buildProfile;signed=(-not $UnsignedPreview);communityPreview=$true;installationQualified=$false;files=$files;runtime=(Get-Content -LiteralPath (Join-Path $root 'providers/image-builder-x86/runtime-distribution.json') -Raw | ConvertFrom-Json)}
[IO.File]::WriteAllText((Join-Path $out 'package-record.json'),($record | ConvertTo-Json -Depth 16)+"`n",(New-Object Text.UTF8Encoding($false)))
Write-Output ("Created community preview package: "+$packages[0].FullName)
