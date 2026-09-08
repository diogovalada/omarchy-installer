[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidateSet('probe','build','plan','deploy','firmware-info','prepare-firmware','prepare-runtime')][string]$Action,
    [Parameter(Mandatory=$true)][string]$RequestPath
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$providerRoot = [IO.Path]::GetFullPath($PSScriptRoot)
$builderRoot = [IO.Path]::GetFullPath((Join-Path $providerRoot '../image-builder-x86'))
$operationId = $null
$minimumBytes = [long]42947575808
$espBytes = [long]2147483648
$rootBytes = [long]40800092160
$stage = $Action
$mutationStarted = $false
$cancelAvailable = $true
$stagingKey = $null
$stagingFrame = $null
$sourceLease = $null
$runtimeFrame = $null

function Emit([string]$Type, [string]$Stage, [hashtable]$Fields) {
    $event = [ordered]@{ protocolVersion=1; type=$Type; operationId=$script:operationId; stage=$Stage; cancelAvailable=$script:cancelAvailable }
    foreach ($key in $Fields.Keys) { $event[$key] = $Fields[$key] }
    [Console]::Out.WriteLine(($event | ConvertTo-Json -Depth 32 -Compress))
}
function Fail([string]$Code, [string]$Message) {
    $errorObject = New-Object InvalidOperationException($Message)
    $errorObject.Data['code'] = $Code
    throw $errorObject
}
function Assert-Fields($Value, [string[]]$Allowed, [string[]]$Required) {
    $names = @($Value.PSObject.Properties | ForEach-Object { $_.Name })
    foreach ($key in $names) {
        if ($Allowed -notcontains $key) { Fail 'invalid_request' "Unknown request field: $key" }
    }
    foreach ($key in $Required) {
        if ($names -notcontains $key -or $null -eq $Value.$key) { Fail 'invalid_request' "Required request field: $key" }
    }
}
function Assert-Path([string]$Path, [bool]$Directory=$false) {
    if (-not [IO.Path]::IsPathRooted($Path) -or $Path.StartsWith('\\') -or $Path.Contains(',')) { Fail 'unsafe_path' 'A local absolute path without commas is required.' }
    $full = [IO.Path]::GetFullPath($Path)
    if ($full.Substring(2).Contains(':')) { Fail 'unsafe_path' 'Alternate data streams are not permitted.' }
    $item = Get-Item -LiteralPath $full -Force
    if (($item -is [IO.DirectoryInfo]) -ne $Directory) { Fail 'unsafe_path' 'Unexpected file or directory type.' }
    $cursor = $item
    while ($null -ne $cursor) {
        if (($cursor.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { Fail 'unsafe_path' 'Reparse points are not permitted in provider input/output paths.' }
        $cursor = if ($cursor -is [IO.DirectoryInfo]) { $cursor.Parent } else { $cursor.Directory }
    }
    return $full
}
function Sha([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Read-Json([string]$Path) {
    $Path = Assert-Path $Path
    if ((Get-Item -LiteralPath $Path).Length -gt 1048576) { Fail 'invalid_request' 'JSON input exceeds its 1 MiB bound.' }
    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}
function Write-Json([string]$Path, $Value) {
    if (Test-Path -LiteralPath $Path) { Fail 'output_exists' 'Operation output already exists; use a fresh operation.' }
    [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 32) + "`n", (New-Object Text.UTF8Encoding($false)))
}
function Assert-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { Fail 'elevation_required' 'The authenticated application helper must invoke this action elevated.' }
}
function Assert-ProtectedDirectory([string]$Directory) {
    $Directory = Assert-Path $Directory $true
    $acl = Get-Acl -LiteralPath $Directory
    if (-not $acl.AreAccessRulesProtected) { Fail 'unprotected_operation' 'Operation directory must disable inherited permissions.' }
    $owner = $acl.GetOwner([Security.Principal.SecurityIdentifier]).Value
    if ($owner -notin @('S-1-5-18','S-1-5-32-544')) { Fail 'unprotected_operation' 'Operation directory must be owned by Administrators or SYSTEM.' }
    # Read access is permitted; mutation permissions are reserved to the helper.
    $writeMask = [long]0x000D0156 # write data/append/EA/attributes/delete/WRITE_DAC/WRITE_OWNER
    foreach ($rule in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
        if ($rule.AccessControlType -eq 'Allow' -and (([long]$rule.FileSystemRights -band $writeMask) -ne 0) -and $rule.IdentityReference.Value -notin @('S-1-5-18','S-1-5-32-544')) {
            Fail 'unprotected_operation' 'An unprivileged identity can modify the operation directory.'
        }
    }
}
function Get-Docker {
    $docker = Join-Path $env:ProgramFiles 'Docker/Docker/resources/bin/docker.exe'
    if (-not (Test-Path -LiteralPath $docker)) { Fail 'docker_missing' 'Install Docker Desktop with a running Linux engine before building; no host software is installed automatically.' }
    return Assert-Path $docker
}
function Get-Runtime {
    $lock = Read-Json (Join-Path $builderRoot 'runtime-lock.json')
    if ($lock.imageId -notmatch '^sha256:[0-9a-f]{64}$') { Fail 'runtime_unpinned' 'Runtime must use a complete immutable Docker image ID.' }
    if ((Sha (Join-Path $builderRoot 'Dockerfile')) -ne $lock.dockerfileSha256 -or (Sha (Join-Path $builderRoot 'runtime-packages.lock')) -ne $lock.packageLockSha256) {
        Fail 'runtime_inputs_changed' 'Runtime build inputs do not match runtime-lock.json; explicitly rebuild and pin the runtime.'
    }
    return $lock
}
try {
    if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitProcess -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { Fail 'unsupported_host' 'This provider requires native Windows x64.' }
    [void](Assert-Path $providerRoot $true)
    [void](Assert-Path $builderRoot $true)
    Add-Type -Path @((Join-Path $providerRoot 'NativeDisk.cs'),(Join-Path $providerRoot 'EncryptedImage.cs'),(Join-Path $providerRoot 'NativeBootEvidence.cs'),(Join-Path $providerRoot 'NativeSource.cs'))
    foreach ($module in @('BitLocker.ps1','StoragePlan.ps1','DeletionPlan.ps1','BootPlan.ps1','EncryptedInputs.ps1','Deployment.ps1','FirmwarePreparation.ps1','RuntimePreparation.ps1')) {
        . (Assert-Path (Join-Path $providerRoot $module))
    }
    $request = Read-Json $RequestPath
    if (@($request.PSObject.Properties | ForEach-Object { $_.Name }) -contains 'operationId') { $operationId = [guid]::Parse($request.operationId).ToString() }
    if ($Action -in @('build','plan','deploy')) { Read-StagingInput }
    switch ($Action) {
        'firmware-info' {
            Assert-Fields $request @() @()
            $facts=Get-FirmwarePreparation
            $hash=[Omarchy.DirectX86.NativeDisk]::Hash((New-Object Text.UTF8Encoding($false)).GetBytes(($facts | ConvertTo-Json -Depth 20 -Compress)))
            Emit 'result' 'firmware-info' @{result=@{facts=$facts;sha256=$hash}}
        }
        'prepare-firmware' { Invoke-FirmwarePreparation $request }
        'prepare-runtime' { Invoke-RuntimePreparation $request }
        'probe' {
            Assert-Fields $request @('operationId','protectedPaths','runtimeArchive') @()
            $protectedPaths=if (@($request.PSObject.Properties.Name) -contains 'protectedPaths') { @($request.protectedPaths) } else { @() }
            $runtimeArchive=if (@($request.PSObject.Properties.Name) -contains 'runtimeArchive') { $request.runtimeArchive } else { $null }
            Emit 'result' 'probed' @{result=(Get-Probe $protectedPaths $runtimeArchive)}
        }
        'build' {
            Assert-Fields $request @('operationId','sourceIsoPath','sourceSignaturePath','outputDirectory','bootMenu','sourceVerification') @('operationId','sourceIsoPath','sourceSignaturePath','outputDirectory','bootMenu')
            Assert-Admin
            Assert-BootMenu $request.bootMenu
            [void](Get-VerifiedWindowsBoot)
            $iso = Assert-Path $request.sourceIsoPath
            $sig = Assert-Path $request.sourceSignaturePath
            $out = Assert-Path $request.outputDirectory $true
            Assert-ProtectedDirectory $out
            if (@(Get-ChildItem -LiteralPath $out -Force).Count -ne 0) { Fail 'output_exists' 'The protected build output directory must be empty.' }
            $release = Read-Json (Join-Path $builderRoot 'release-lock.json')
            $inputName = 'omarchy-'+$release.version+'.iso'
            if ([IO.Path]::GetFileName($sig) -cne ($inputName+'.sig')) { Fail 'source_name' 'The protected signature must use the pinned release filename.' }
            if ((Get-Item -LiteralPath $iso).Length -ne $release.sizeBytes) { Fail 'source_changed' 'Official ISO size does not match the pinned release.' }
            $runtime = Get-Runtime; $docker = Get-Docker
            if ((Get-Item -LiteralPath $out).PSDrive.Free -lt 85GB) { Fail 'insufficient_storage' '85 GiB free operation storage is required for construction.' }
            if ((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory -lt 10GB/1024) { Fail 'insufficient_memory' '10 GiB currently free memory is required for local construction.' }
            $engine = & $docker info --format '{{.OSType}}' 2>$null
            if ($LASTEXITCODE -ne 0 -or ($engine -join '').Trim() -ne 'linux') { Fail 'linux_engine_required' 'A running Linux Docker engine is required.' }
            $image = & $docker image inspect $runtime.imageId --format '{{.Id}}' 2>$null
            if ($LASTEXITCODE -ne 0 -or ($image -join '').Trim() -ne $runtime.imageId) { Fail 'runtime_missing' 'Build and pin the required local runtime first; the provider never pulls an unqualified image.' }
            $runtimeFrame = $stagingFrame
            if (@($request.PSObject.Properties.Name) -contains 'sourceVerification') {
                Assert-ProtectedDirectory (Split-Path -Parent $RequestPath)
                $verified = $request.sourceVerification
                $fields = @('kind','parentPid','parentStarted','volumeSerial','fileIndex','length','sha256','signatureSha256')
                Assert-Fields $verified $fields $fields
                if ($verified.kind -cne 'windows-verified-download-v1' -or $verified.length -ne $release.sizeBytes -or $verified.sha256 -cne $release.sha256 -or $verified.signatureSha256 -cne (Sha $sig)) {
                    Fail 'source_lease_invalid' 'The verified source lease does not match the packaged release and signature.'
                }
                $sourceLease = New-Object Omarchy.DirectX86.NativeSource($iso,[long]$release.sizeBytes)
                $sourceLease.Match([uint32]::Parse($verified.volumeSerial),[uint64]::Parse($verified.fileIndex))
                # Check the actual parent only after acquiring our own guard.
                # It cannot release its guard before this process has inherited
                # responsibility for keeping the source and path immutable.
                $self = Get-CimInstance Win32_Process -Filter "ProcessId=$PID"
                $parent = Get-Process -Id ([int]$verified.parentPid) -ErrorAction Stop
                if ($self.ParentProcessId -ne $verified.parentPid -or $parent.HasExited -or $parent.StartTime.ToFileTimeUtc().ToString([Globalization.CultureInfo]::InvariantCulture) -cne $verified.parentStarted) { Fail 'source_lease_invalid' 'The verified source helper is no longer the live parent.' }
                $parent.Dispose()
                $frame = $stagingFrame | ConvertFrom-Json
                $runtimeFrame = (@{protocolVersion=3;stagingKey=$frame.stagingKey;verifiedSource=@{kind='windows-held-readonly-bind-v1';sha256=$release.sha256;length=$release.sizeBytes;signatureSha256=$verified.signatureSha256}} | ConvertTo-Json -Depth 4 -Compress)
                $frame = $null
                Emit 'progress' 'source-ready' @{message='Using the locked, verified ISO for isolated construction.'}
            } else {
                Emit 'progress' 'verifying' @{message='Checking the official ISO before isolated construction.'}
                if ((Sha $iso) -ne $release.sha256) { Fail 'source_changed' 'Official ISO digest changed.' }
            }
            $container = 'omarchy-direct-'+$operationId
            # The helper holds the ISO and its ancestors against writes/renames.
            # Bind only the two input files, from their separate host locations.
            $arguments = @('run','-i','--rm','--name',$container,'--init','--network','none','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges','--pids-limit','128','--cpus','4','--memory','9216m','--memory-swap','9216m','--user','1000:1000','--tmpfs','/tmp:rw,nosuid,nodev,size=256m','--mount',"type=bind,source=$builderRoot,target=/proof,readonly",'--mount',"type=bind,source=$iso,target=/input/$inputName,readonly",'--mount',"type=bind,source=$sig,target=/input/$inputName.sig,readonly",'--mount',"type=bind,source=$out,target=/output",'--entrypoint','python3',$runtime.imageId,'/proof/product_builder.py','--operation-id',$operationId)
            $buildResult = $null
            $arguments += @('--boot-default',[string]$request.bootMenu.defaultOs,'--boot-timeout',[string]$request.bootMenu.timeoutSeconds)
            try {
                $runtimeFrame | & $docker @arguments 2> (Join-Path (Split-Path -Parent $out) ($operationId+'-runtime.log')) | ForEach-Object {
                    $line = $_
                    try { $event = $line | ConvertFrom-Json } catch { Fail 'runtime_protocol' 'Build runtime emitted invalid structured output.' }
                    if ($event.protocolVersion -ne 1 -or $event.operationId -ne $operationId) { Fail 'runtime_protocol' 'Build runtime emitted an invalid operation event.' }
                    if ($event.type -eq 'result') { $buildResult = $event }
                    elseif ($event.type -eq 'progress') { [Console]::Out.WriteLine($line) }
                    elseif ($event.type -eq 'error') { Fail $event.code $event.message }
                    else { Fail 'runtime_protocol' 'Unknown build event type.' }
                }
                if ($LASTEXITCODE -ne 0 -or $null -eq $buildResult) { Fail 'construction_failed' 'Local construction failed or exited without a completed manifest.' }
            } finally {
                # Exact operation container only. Does not affect any other VM/container.
                $remaining = & $docker ps --quiet --filter "name=^/$container$" 2>$null
                if ($LASTEXITCODE -ne 0) { Fail 'construction_stop_unknown' 'Docker could not confirm construction stopped. Temporary images are retained.' }
                if ($remaining) {
                    & $docker stop --time 10 $container 2>$null | Out-Null
                    if ($LASTEXITCODE -ne 0) { Fail 'construction_stop_unknown' 'The construction container could not be stopped. Temporary images are retained.' }
                }
                Write-Json (Join-Path (Split-Path -Parent $out) 'image-build-stopped.json') @{operationId=$operationId;container=$container;stopped=$true}
            }
            $manifestPath = Join-Path $out 'manifest.json'
            [void](Read-Manifest $manifestPath $buildResult.manifestSha256)
            Emit 'result' 'built' @{manifestPath=$manifestPath;manifestSha256=$buildResult.manifestSha256;result=$buildResult.result}
        }
        'plan' { Invoke-DirectPlan $request }
        'deploy' { Invoke-DirectDeploy $request }
    }
} catch {
    $code = 'provider_failed'
    if ($_.Exception.Data.Contains('code')) { $code = [string]$_.Exception.Data['code'] }
    Emit 'error' $stage @{code=$code;message=$_.Exception.Message;mutationStarted=$mutationStarted;recovery= $(if ($mutationStarted) { 'Keep the protected operation and BitLocker recovery records. Windows may have been resized exactly as confirmed; new Omarchy partitions or boot setup may be incomplete. Do not blindly retry or delete partitions.' } else { 'No destination mutation was started.' })}
    exit 1
} finally {
    if ($null -ne $sourceLease) { $sourceLease.Dispose() }
    if ($null -ne $stagingKey) { [Array]::Clear($stagingKey,0,$stagingKey.Length) }
    $stagingKey=$null; $stagingFrame=$null; $runtimeFrame=$null
}
