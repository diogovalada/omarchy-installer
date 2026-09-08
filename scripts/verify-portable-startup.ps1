[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$Executable,
    [ValidateSet('auto','cold','warm','repair')][string]$ExpectedCacheMode='auto'
)
$ErrorActionPreference='Stop'
$Executable=(Resolve-Path -LiteralPath $Executable).Path
$package=Get-Content -LiteralPath (Join-Path (Split-Path -Parent $Executable) 'portable-record.json') -Raw | ConvertFrom-Json
$cacheDirectory=if ($package.cache) { Join-Path $env:LOCALAPPDATA ('OmarchySetup/p/'+$package.cache.id) } else { $null }
$cacheBefore=if ($cacheDirectory -and (Test-Path -LiteralPath $cacheDirectory)) { (Get-Item -LiteralPath $cacheDirectory).CreationTimeUtc.Ticks } else { $null }
$cachedExeBefore=if ($cacheDirectory -and (Test-Path -LiteralPath (Join-Path $cacheDirectory 'Omarchy Installer.exe'))) { (Get-Item -LiteralPath (Join-Path $cacheDirectory 'Omarchy Installer.exe')).CreationTimeUtc.Ticks } else { $null }
if ($ExpectedCacheMode -eq 'cold' -and $null -ne $cacheBefore) { throw 'Cold-start verification requires an absent cache.' }
if ($ExpectedCacheMode -in @('warm','repair') -and $null -eq $cacheBefore) { throw 'This verification requires an existing cache.' }
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class PortableStartupWindow {
  [StructLayout(LayoutKind.Sequential)] private struct LastInput { public uint Size; public uint Time; }
  [DllImport("user32.dll")] private static extern bool GetLastInputInfo(ref LastInput info);
  public static uint LastInputTime() {
    var info = new LastInput { Size = 8 };
    if (!GetLastInputInfo(ref info)) throw new System.ComponentModel.Win32Exception();
    return info.Time;
  }
  [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr window, int id);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr window, int index);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
}
'@
$existing=@(Get-Process -Name 'Omarchy Installer' -ErrorAction SilentlyContinue | ForEach-Object Id)
$inputAtStart=[PortableStartupWindow]::LastInputTime()
$timer=[Diagnostics.Stopwatch]::StartNew()
# This is an intentional visible UI test, not a background helper/service.
$launcher=Start-Process -FilePath $Executable -PassThru
$bannerSeconds=$null
$bannerText=$null
$bootstrapDirectory=$null
$application=$null
$windowSeconds=$null
while ($timer.Elapsed.TotalSeconds -lt 120) {
    $launcher.Refresh()
    if ($launcher.HasExited) { throw "Launcher exited before the application window appeared: $($launcher.ExitCode)" }
    if ($null -eq $bannerSeconds -and $launcher.MainWindowHandle -ne [IntPtr]::Zero) {
        $text=New-Object Text.StringBuilder 1024
        $control=[PortableStartupWindow]::GetDlgItem($launcher.MainWindowHandle,1030)
        [void][PortableStartupWindow]::GetWindowText($control,$text,$text.Capacity)
        if ($text.ToString() -match '^(Opening Omarchy Installer|Checking saved application files|Extracting application files|Verifying application files)') {
            $bannerSeconds=$timer.Elapsed.TotalSeconds
            $bannerText=$text.ToString()
        }
    }
    if ($cacheDirectory -and -not $bootstrapDirectory) {
        $verifier=Get-CimInstance Win32_Process -Filter "ParentProcessId=$($launcher.Id) AND Name='cache-check.exe'" | Select-Object -First 1
        if ($verifier.ExecutablePath) { $bootstrapDirectory=Split-Path -Parent $verifier.ExecutablePath }
    }
    $application=Get-Process -Name 'Omarchy Installer' -ErrorAction SilentlyContinue |
        Where-Object { $_.Id -notin $existing -and $_.MainWindowHandle -ne [IntPtr]::Zero -and $_.MainWindowTitle -like 'Omarchy Installer*' } |
        Where-Object { (Get-CimInstance Win32_Process -Filter "ProcessId=$($_.Id)").ParentProcessId -eq $launcher.Id } |
        Select-Object -First 1
    if ($application) { $windowSeconds=$timer.Elapsed.TotalSeconds; break }
    Start-Sleep -Milliseconds 100
}
if (-not $application) { throw 'Application window did not appear within 120 seconds; test instance left for inspection.' }
$payloadDirectory=Split-Path -Parent $application.Path
$applicationId=$application.Id
$foregroundSeconds=$null
$activationTimer=[Diagnostics.Stopwatch]::StartNew()
while ($activationTimer.Elapsed.TotalSeconds -lt 10) {
    $application.Refresh()
    if ([PortableStartupWindow]::GetForegroundWindow() -eq $application.MainWindowHandle -and -not [PortableStartupWindow]::IsIconic($application.MainWindowHandle)) {
        $foregroundSeconds=$timer.Elapsed.TotalSeconds
        break
    }
    Start-Sleep -Milliseconds 100
}
# Catch a transient activation immediately undone by the extraction window closing.
Start-Sleep -Milliseconds 750
$foregroundRetained=[PortableStartupWindow]::GetForegroundWindow() -eq $application.MainWindowHandle
$foregroundProcessId=[uint32]0
[void][PortableStartupWindow]::GetWindowThreadProcessId([PortableStartupWindow]::GetForegroundWindow(),[ref]$foregroundProcessId)
$minimized=[PortableStartupWindow]::IsIconic($application.MainWindowHandle)
$alwaysOnTop=([PortableStartupWindow]::GetWindowLong($application.MainWindowHandle,-20) -band 8) -ne 0
$inputDuringLaunch=[PortableStartupWindow]::LastInputTime() -ne $inputAtStart
$foregroundCheck=if ($null -ne $foregroundSeconds -and $foregroundRetained -and -not $minimized -and -not $alwaysOnTop) { 'passed' } elseif ($inputDuringLaunch) { 'inconclusive-input-during-launch' } else { 'failed' }
$cachedFilesProtected=$null
if ($cacheDirectory) {
    if ($payloadDirectory -ne $cacheDirectory) { throw 'Application started from an unexpected cache directory.' }
    try {
        $writeProbe=[IO.File]::Open((Join-Path $cacheDirectory 'LICENSE.txt'),[IO.FileMode]::Open,[IO.FileAccess]::Write,[IO.FileShare]::ReadWrite)
        $writeProbe.Dispose()
        $cachedFilesProtected=$false
    } catch [IO.IOException] { $cachedFilesProtected=$true }
}
if (-not $application.CloseMainWindow()) { throw 'Could not request normal closure of the test application.' }
if (-not $launcher.WaitForExit(30000)) { throw 'Launcher did not exit after the application closed.' }
$cacheRetained=$cacheDirectory -and (Test-Path -LiteralPath $cacheDirectory)
$cacheReused=$cacheRetained -and $null -ne $cacheBefore -and $cacheBefore -eq (Get-Item -LiteralPath $cacheDirectory).CreationTimeUtc.Ticks -and $cachedExeBefore -eq (Get-Item -LiteralPath (Join-Path $cacheDirectory 'Omarchy Installer.exe')).CreationTimeUtc.Ticks
$record=[ordered]@{
    executable=$Executable
    extractionIndicatorSeconds=$bannerSeconds
    extractionIndicatorText=$bannerText
    applicationWindowSeconds=$windowSeconds
    applicationProcessId=$applicationId
    applicationForegroundSeconds=$foregroundSeconds
    foregroundRetained=$foregroundRetained
    finalForegroundProcessId=$foregroundProcessId
    inputDuringLaunch=$inputDuringLaunch
    foregroundCheck=$foregroundCheck
    minimized=$minimized
    alwaysOnTop=$alwaysOnTop
    payloadDirectory=$payloadDirectory
    expectedCacheMode=$ExpectedCacheMode
    cacheWasPresent=($null -ne $cacheBefore)
    cacheRetained=[bool]$cacheRetained
    cacheReused=[bool]$cacheReused
    cachedFilesProtected=$cachedFilesProtected
    temporaryVerifierDirectory=$bootstrapDirectory
    temporaryVerifierRemoved=$(if ($bootstrapDirectory) { -not (Test-Path -LiteralPath $bootstrapDirectory) } else { $null })
    launcherExitCode=$launcher.ExitCode
    temporaryPayloadRemoved=$(if (-not $cacheDirectory) { -not (Test-Path -LiteralPath $payloadDirectory) } else { $null })
    observedAtUtc=[DateTime]::UtcNow.ToString('o')
}
$json=$record | ConvertTo-Json
$reportName=if ($ExpectedCacheMode -eq 'auto') { 'startup-verification.json' } else { 'startup-verification-'+$ExpectedCacheMode+'.json' }
$report=Join-Path (Split-Path -Parent $Executable) $reportName
[IO.File]::WriteAllText($report,$json+"`n",(New-Object Text.UTF8Encoding($false)))
$json
if ($launcher.ExitCode -ne 0) { throw 'Normal application exit failed.' }
if ($cacheDirectory) {
    if (-not $cacheRetained -or -not $cachedFilesProtected) { throw 'Cache retention or verified-file protection failed.' }
    if ($bootstrapDirectory -and -not $record.temporaryVerifierRemoved) { throw 'Temporary verifier cleanup failed.' }
    if ($ExpectedCacheMode -eq 'warm' -and -not $cacheReused) { throw 'Warm start unexpectedly re-extracted the payload.' }
    if ($ExpectedCacheMode -eq 'repair' -and $cacheReused) { throw 'Damaged cache was not replaced.' }
} elseif (-not $record.temporaryPayloadRemoved) { throw 'Temporary payload cleanup failed.' }
if ($null -eq $bannerSeconds) { throw 'The extraction indicator was not observed.' }
# Physical input during this visible test can revoke foreground permission.
# Record that as inconclusive, not as proof of either success or a launcher bug.
if ($foregroundCheck -eq 'failed') { throw 'The application did not retain normal foreground focus after extraction.' }
