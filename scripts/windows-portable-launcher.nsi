; Portable launcher only: no installation directory, registry registration,
; shortcuts, uninstaller or elevation. Exact payloads share a verified user cache.
Unicode true
Name "Omarchy Installer"
OutFile "${OUTPUT_FILE}"
Icon "${APP_ICON}"
RequestExecutionLevel user
SilentInstall silent
AutoCloseWindow true
CRCCheck force
; Independent blocks let a cache hit load only the small trusted verifier.
SetCompressor zlib
VIProductVersion "0.1.0.0"
VIAddVersionKey "ProductName" "Omarchy Installer Portable"
VIAddVersionKey "FileDescription" "Portable Omarchy Installer community preview"
VIAddVersionKey "FileVersion" "0.1.0"
VIAddVersionKey "LegalCopyright" "Community preview; upstream notices included"
!include FileFunc.nsh
Var VerifyOnly
Var AppProcess
Var AppPid
Var AppWindow
Var WindowChecks
Var AppDir
Var LauncherPid
Var CacheMutex
Var MutexHeld
Var BannerWindow

Function .onInit
  StrCpy $VerifyOnly "0"
  StrCpy $MutexHeld "0"
  StrCpy $CacheMutex "0"
  ${GetParameters} $0
  StrCmp $0 "" accepted
  StrCmp $0 "--verify-bundle" verify
  SetErrorLevel 64
  Quit
verify:
  StrCpy $VerifyOnly "1"
accepted:
  ; Declare the plugin before the payload so a solid archive can show this
  ; immediately, without first decompressing the application to load the plugin.
  StrCmp $VerifyOnly "1" quiet_init
  Banner::show /set 1030 "Opening Omarchy Installer..." "Omarchy Installer"
  Banner::getWindow
  Pop $BannerWindow
quiet_init:
FunctionEnd

Section
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  SetOverwrite off
  ClearErrors
  ; This verifier and its manifest always come from the opened executable.
  File /oname=cache-check.exe "${CACHE_HELPER}"
  File /oname=cache-manifest.json "${CACHE_MANIFEST}"
  IfErrors failed
  System::Call 'kernel32::GetCurrentProcessId() i.s'
  Pop $LauncherPid
  StrCmp $VerifyOnly "1" verify_extract
  StrCpy $AppDir "$LOCALAPPDATA\OmarchySetup\p\${CACHE_ID}"
  ; Serialize extraction/publication for this user and exact payload only.
  ReadEnvStr $8 USERDOMAIN
  ReadEnvStr $9 USERNAME
  ; Global also covers two desktop sessions of the same account sharing this cache.
  System::Call 'kernel32::CreateMutexW(p 0, i 0, w "Global\OmarchySetup.${CACHE_ID}.$8.$9") p.s'
  Pop $CacheMutex
  StrCmp $CacheMutex "0" failed
  StrCpy $WindowChecks 0
wait_cache:
  System::Call 'kernel32::WaitForSingleObject(p $CacheMutex, i 100) i.r0'
  StrCmp $0 "0" cache_locked
  StrCmp $0 "128" cache_locked
  StrCmp $0 "258" 0 failed
  IntOp $WindowChecks $WindowChecks + 1
  IntCmp $WindowChecks 1200 failed
  Goto wait_cache
cache_locked:
  StrCpy $MutexHeld "1"
  GetDlgItem $1 $BannerWindow 1030
  SendMessage $1 0x000C 0 "STR:Checking saved application files..."
  nsExec::ExecToStack '"$PLUGINSDIR\cache-check.exe" check "$PLUGINSDIR\cache-manifest.json" "${CACHE_HASH}" "$AppDir" $LauncherPid'
  Pop $0
  Pop $1
  StrCmp $0 "0" launch
  StrCmp $0 "10" 0 failed
  GetDlgItem $1 $BannerWindow 1030
  SendMessage $1 0x000C 0 "STR:Extracting application files for this version..."
  nsExec::ExecToStack '"$PLUGINSDIR\cache-check.exe" prepare "$PLUGINSDIR\cache-manifest.json" "${CACHE_HASH}" "$AppDir" $LauncherPid'
  Pop $0
  Pop $1
  StrCmp $0 "0" 0 failed
  SetOutPath "$AppDir.part"
  Goto extract
verify_extract:
  StrCpy $AppDir "$PLUGINSDIR\payload"
  SetOutPath "$AppDir"
extract:
  ClearErrors
  File /r "${PAYLOAD_DIR}\*.*"
  IfErrors failed
  ; Release the CWD before the verifier atomically publishes the staging folder.
  SetOutPath "$TEMP"
  StrCmp $VerifyOnly "1" verify
  GetDlgItem $1 $BannerWindow 1030
  SendMessage $1 0x000C 0 "STR:Verifying application files..."
  nsExec::ExecToStack '"$PLUGINSDIR\cache-check.exe" publish "$PLUGINSDIR\cache-manifest.json" "${CACHE_HASH}" "$AppDir" $LauncherPid'
  Pop $0
  Pop $1
  StrCmp $0 "0" launch failed
verify:
  ; Hidden read-only smoke test: no GUI, disks, network or elevated helper.
  nsExec::ExecToStack '"$PLUGINSDIR\cache-check.exe" verify "$PLUGINSDIR\cache-manifest.json" "${CACHE_HASH}" "$AppDir" 0'
  Pop $0
  Pop $1
  StrCmp $0 "0" verified failed
verified:
  SetErrorLevel 0
  Goto done
launch:
  SetOutPath "$AppDir"
  ; Receipt exports belong beside the portable launcher, not its extracted cache.
  ; This is used only by the unelevated desktop for user-owned output files.
  System::Call 'kernel32::SetEnvironmentVariableW(w "OMARCHY_PORTABLE_EXE", w "$EXEPATH")'
  ; Keep the extraction window alive until the child's real window is ready.
  ; Destroying it before launching drops the foreground activation handoff.
  ; NSIS is a 32-bit process, including when it launches this x64 application.
  ; STARTUPINFOW (68 bytes) and PROCESS_INFORMATION (16 bytes).
  System::Call '*(i 68, p 0, p 0, p 0, i 0, i 0, i 0, i 0, i 0, i 0, i 0, i 1, &i2 1, &i2 0, p 0, p 0, p 0, p 0) p.r1'
  System::Call '*(p 0, p 0, i 0, i 0) p.r2'
  StrCmp $1 0 allocation_failed
  StrCmp $2 0 allocation_failed
  StrCpy $7 '"$AppDir\Omarchy Installer.exe"'
  System::Call 'kernel32::CreateProcessW(w "$AppDir\Omarchy Installer.exe", w r7, p 0, p 0, i 0, i 0, p 0, w "$AppDir", p r1, p r2) i.r0'
  System::Call '*$2(p.r3, p.r4, i.r5, i.r6)'
  System::Free $1
  System::Free $2
  StrCmp $0 0 failed
  StrCpy $AppProcess $3
  StrCpy $AppPid $5
  System::Call 'kernel32::CloseHandle(p r4)'
  System::Call 'kernel32::ReleaseMutex(p $CacheMutex)'
  StrCpy $MutexHeld "0"
  System::Call 'user32::AllowSetForegroundWindow(i $AppPid)'
  StrCpy $WindowChecks 0
wait_window:
  System::Call 'kernel32::WaitForSingleObject(p $AppProcess, i 0) i.r0'
  StrCmp $0 0 app_exited
  ; Only activate a visible, unowned, titled window from the exact child PID.
  ; Another running Omarchy instance must never receive this handoff.
  System::Call 'user32::GetTopWindow(p 0) p.r8'
next_window:
  StrCmp $8 0 retry_window
  System::Call 'user32::GetWindowThreadProcessId(p r8, *i.r9)'
  StrCmp $9 $AppPid 0 skip_window
  System::Call 'user32::IsWindowVisible(p r8) i.r0'
  StrCmp $0 0 skip_window
  System::Call 'user32::GetWindow(p r8, i 4) p.r0'
  StrCmp $0 0 0 skip_window
  System::Call 'user32::GetWindowTextLengthW(p r8) i.r0'
  StrCmp $0 0 skip_window
  StrCpy $AppWindow $8
  System::Call 'user32::IsIconic(p $AppWindow) i.r0'
  StrCmp $0 0 activate_window
  System::Call 'user32::ShowWindowAsync(p $AppWindow, i 9)'
activate_window:
  ; One normal activation, never TOPMOST or repeated focus stealing.
  Banner::destroy
  System::Call 'user32::SetForegroundWindow(p $AppWindow)'
  Goto wait_app
skip_window:
  System::Call 'user32::GetWindow(p r8, i 2) p.r8'
  Goto next_window
retry_window:
  IntOp $WindowChecks $WindowChecks + 1
  IntCmp $WindowChecks 1200 hide_banner
  Sleep 100
  Goto wait_window
hide_banner:
  Banner::destroy
wait_app:
  ; Retain verified file handles until app exit. The reusable cache stays on disk.
  System::Call 'kernel32::WaitForSingleObject(p $AppProcess, i 100) i.r0'
  StrCmp $0 258 wait_app
app_exited:
  Banner::destroy
  StrCpy $0 2
  System::Call 'kernel32::GetExitCodeProcess(p $AppProcess, *i.r0)'
  System::Call 'kernel32::CloseHandle(p $AppProcess)'
  SetErrorLevel $0
  Goto done
allocation_failed:
  System::Free $1
  System::Free $2
  Goto failed
failed:
  StrCmp $VerifyOnly "1" quiet
  Banner::destroy
  MessageBox MB_OK|MB_ICONSTOP "Omarchy Installer could not prepare or verify its application cache. Close other Omarchy Installer windows, check available disk space, and try again."
quiet:
  SetErrorLevel 2
done:
  StrCmp $MutexHeld "1" 0 mutex_closed
  System::Call 'kernel32::ReleaseMutex(p $CacheMutex)'
mutex_closed:
  StrCmp $CacheMutex "0" release_cwd
  System::Call 'kernel32::CloseHandle(p $CacheMutex)'
release_cwd:
  ; Release the launcher's current-directory handle before NSIS removes its
  ; private payload. Windows cannot delete a directory that is still the CWD.
  SetOutPath "$TEMP"
  ; NSIS removes only its own small verifier/plugin directory on exit.
  ; The application prevents a normal exit while installation work is active.
SectionEnd
