; KIVO's additions to Tauri's NSIS installer (DISTRIBUTION §1, DIST-05). Tauri's template already
; adds the Start-menu shortcut, the uninstaller and in-place upgrades; these hooks handle the two
; processes it doesn't know about (the runtime and the speech worker) and the startup entry.
;
; Startup: "Open KIVO when Windows starts" is off by default and asked during onboarding
; (UX §7). Deployments can turn it on with `KIVO-setup.exe /S /STARTUP`; the runtime adopts the
; entry as the setting on its first start. The value matches what the runtime writes itself.

!include "FileFunc.nsh"

!define KIVO_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define KIVO_RUN_VALUE "KIVO"

; The runtime supervises the app and the worker; stopping its tree frees every file for the copy.
!macro KIVO_STOP_PROCESSES
  nsExec::Exec 'taskkill /T /F /IM kivo-runtime.exe'
  Pop $0
  nsExec::Exec 'taskkill /F /IM kivo-infer.exe'
  Pop $0
  Sleep 300
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro KIVO_STOP_PROCESSES
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} $R0 "/STARTUP" $R1
  ${IfNot} ${Errors}
    WriteRegStr HKCU "${KIVO_RUN_KEY}" "${KIVO_RUN_VALUE}" '"$INSTDIR\kivo-runtime.exe" --autostart'
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro KIVO_STOP_PROCESSES
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; An upgrade reinstalls over the top and keeps the user's choice; a real uninstall removes it.
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "${KIVO_RUN_KEY}" "${KIVO_RUN_VALUE}"
  ${EndIf}
!macroend
