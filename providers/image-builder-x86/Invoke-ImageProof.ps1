[CmdletBinding()]
param(
    [ValidateSet('preflight', 'smoke', 'inspect', 'install')]
    [string]$Action = 'preflight',
    [ValidateSet(4096, 6144, 8192)]
    [int]$MemoryMiB = 6144,
    [ValidateRange(60, 2400)]
    [int]$TimeoutSeconds = 1800
)
$ErrorActionPreference = 'Stop'
$proofRoot = [IO.Path]::GetFullPath($PSScriptRoot)
function Assert-NoLink([string]$Path) {
    $current = Get-Item -LiteralPath $Path -Force
    while ($null -ne $current) {
        if (($current.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Reparse points are not permitted in proof paths: $($current.FullName)"
        }
        $current = $current.Parent
    }
}
Assert-NoLink $proofRoot
$runRoot = Join-Path $proofRoot '.runs'
$inputRoot = Join-Path $proofRoot '.cache'
foreach ($path in @($runRoot, $inputRoot)) {
    if (-not (Test-Path -LiteralPath $path)) { New-Item -ItemType Directory -Path $path | Out-Null }
    Assert-NoLink $path
}
$runtime = Get-Content -LiteralPath (Join-Path $proofRoot 'runtime-lock.json') -Raw | ConvertFrom-Json
if ($runtime.imageId -notmatch '^sha256:[0-9a-f]{64}$') { throw 'Runtime must use a full immutable image ID.' }
$dockerfileHash = (Get-FileHash -LiteralPath (Join-Path $proofRoot 'Dockerfile') -Algorithm SHA256).Hash.ToLowerInvariant()
if ($dockerfileHash -ne $runtime.dockerfileSha256) { throw 'Runtime lock does not match Dockerfile; rebuild and record the runtime.' }
$packageLockHash = (Get-FileHash -LiteralPath (Join-Path $proofRoot 'runtime-packages.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
if ($packageLockHash -ne $runtime.packageLockSha256) { throw 'Runtime package lock changed; rebuild and record the runtime.' }
$engine = docker info --format '{{.OSType}}'
if ($LASTEXITCODE -ne 0 -or $engine -ne 'linux') { throw 'A running Linux Docker engine is required.' }
docker image inspect $runtime.imageId --format '{{.Id}}' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Pinned proof runtime is not present. See README for local build instructions.' }
$freeBytes = (Get-Item -LiteralPath $runRoot).PSDrive.Free
$freeMemoryMiB = [math]::Floor((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1024)
if ($Action -in @('inspect', 'install') -and $freeBytes -lt 50GB) {
    throw 'Proof needs at least 50 GiB free for the bounded virtual disk, ISO inspection, and reserve.'
}
if ($Action -eq 'install' -and $freeMemoryMiB -lt ($MemoryMiB + 4096)) {
    throw "Only $freeMemoryMiB MiB memory is currently free; $($MemoryMiB + 4096) MiB is required for this guest, measured TCG overhead, and host reserve."
}
$containerMemory = if ($Action -eq 'install') { "$($MemoryMiB + 3072)m" } else { '1g' }
$containerName = 'omarchy-image-proof-' + [guid]::NewGuid().ToString('N')
# Read-only source/cache, one owned output mount, no device mappings, privilege,
# host networking, published ports, or Docker socket. Software emulation only.
$arguments = @(
    'run', '--rm', '--name', $containerName, '--init', '--network', 'none',
    '--read-only', '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges',
    '--pids-limit', '128', '--cpus', '4', '--memory', $containerMemory,
    '--memory-swap', $containerMemory, '--user', '1000:1000',
    '--tmpfs', '/tmp:rw,nosuid,nodev,size=256m',
    '--mount', "type=bind,source=$proofRoot,target=/proof,readonly",
    '--mount', "type=bind,source=$inputRoot,target=/input,readonly",
    '--mount', "type=bind,source=$runRoot,target=/runs",
    $runtime.imageId, $Action, '--memory', $MemoryMiB, '--timeout', $TimeoutSeconds
)
Write-Output "Local VM proof: $Action; free disk $([math]::Round($freeBytes / 1GB, 1)) GiB; free RAM $freeMemoryMiB MiB."
try {
    & docker @arguments
    if ($LASTEXITCODE -ne 0) { throw "Image proof failed with exit code $LASTEXITCODE. Inspect .runs/*/receipt.json." }
} finally {
    # Exact operation-owned container only; never stop another task's containers.
    $remaining = docker ps --quiet --filter "name=^/$containerName$"
    if ($remaining) { docker stop --time 10 $containerName | Out-Null }
}
