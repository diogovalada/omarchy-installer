[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Executable)
$ErrorActionPreference='Stop'
$Executable=(Resolve-Path -LiteralPath $Executable).Path
$packageDirectory=Split-Path -Parent $Executable
$record=Get-Content -LiteralPath (Join-Path $packageDirectory 'portable-record.json') -Raw | ConvertFrom-Json
if ($record.cache.id -notmatch '^[a-f0-9]{20}$' -or $record.cache.id -ne $record.cache.manifestSha256.Substring(0,20)) { throw 'Invalid package cache identity.' }
$cacheParent=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'OmarchySetup/p'))
$cacheRoot=[IO.Path]::GetFullPath((Join-Path $cacheParent $record.cache.id))
$savedRoot=[IO.Path]::GetFullPath((Join-Path $cacheParent ($record.cache.id+'.concurrency-'+[Guid]::NewGuid().ToString('N').Substring(0,8))))
# Both resolved targets must remain in this app's explicitly named cache directory.
if (-not $cacheRoot.StartsWith($cacheParent+[IO.Path]::DirectorySeparatorChar) -or -not $savedRoot.StartsWith($cacheParent+[IO.Path]::DirectorySeparatorChar)) { throw 'Invalid fixture cache paths.' }
$appPath=Join-Path $cacheRoot 'Omarchy Installer.exe'
if (@(Get-Process -Name 'Omarchy Installer' -ErrorAction SilentlyContinue | Where-Object Path -eq $appPath).Count) { throw 'This cache is in use; test leaves existing app instances untouched.' }
$savedCache=$null
if (Test-Path -LiteralPath $cacheRoot) {
    if ((Get-Item -LiteralPath $cacheRoot).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Cache fixture must not be a link.' }
    Move-Item -LiteralPath $cacheRoot -Destination $savedRoot
    $savedCache=$savedRoot
}
# These are intentional visible app startup tests, not background services.
$first=Start-Process -FilePath $Executable -PassThru
$second=Start-Process -FilePath $Executable -PassThru
$launcherIds=@($first.Id,$second.Id)
$timer=[Diagnostics.Stopwatch]::StartNew()
$applications=@()
while ($timer.Elapsed.TotalSeconds -lt 120) {
    $applications=@(Get-Process -Name 'Omarchy Installer' -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $appPath -and $_.MainWindowHandle -ne [IntPtr]::Zero } |
        Where-Object { (Get-CimInstance Win32_Process -Filter "ProcessId=$($_.Id)").ParentProcessId -in $launcherIds })
    if ($applications.Count -eq 2) { break }
    $first.Refresh(); $second.Refresh()
    if ($first.HasExited -or $second.HasExited) { throw 'A concurrent launcher exited before both app windows appeared.' }
    Start-Sleep -Milliseconds 200
}
if ($applications.Count -ne 2) { throw 'Concurrent startup failed; test instances left for inspection.' }
$windowSeconds=$timer.Elapsed.TotalSeconds
foreach ($application in $applications) {
    if (-not $application.CloseMainWindow()) { throw 'Could not close a test application normally.' }
}
if (-not $first.WaitForExit(30000) -or -not $second.WaitForExit(30000)) { throw 'Concurrent launchers did not exit normally.' }
$helper=Join-Path $PSScriptRoot 'portable-cache-helper/target/release/omarchy-portable-cache.exe'
& $helper verify (Join-Path $packageDirectory 'cache-manifest.json') $record.cache.manifestSha256 $cacheRoot 0
$verified=$LASTEXITCODE -eq 0
$result=[ordered]@{
    executable=$Executable
    twoWindowsSeconds=$windowSeconds
    applicationProcessIds=@($applications | ForEach-Object Id)
    launcherExitCodes=@($first.ExitCode,$second.ExitCode)
    cacheDirectory=$cacheRoot
    savedFixtureCache=$savedCache
    cacheVerified=$verified
    stagingAbsent=(-not (Test-Path -LiteralPath ($cacheRoot+'.part')))
    observedAtUtc=[DateTime]::UtcNow.ToString('o')
}
$json=$result | ConvertTo-Json -Depth 4
[IO.File]::WriteAllText((Join-Path $packageDirectory 'concurrent-startup-verification.json'),$json,(New-Object Text.UTF8Encoding($false)))
$json
if ($first.ExitCode -ne 0 -or $second.ExitCode -ne 0 -or -not $verified -or -not $result.stagingAbsent) { throw 'Concurrent cache startup verification failed.' }
