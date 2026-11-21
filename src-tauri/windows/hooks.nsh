!macro NSIS_HOOK_POSTINSTALL
  ; Tauri resources are installed inside $INSTDIR\resources.
  ; The Windows DLL loader requires libmpv beside our executable.
  IfFileExists "$INSTDIR\resources\libmpv-2.dll" +2 0
    Abort "Missing bundled libmpv runtime"
  SetOutPath "$INSTDIR"
  ClearErrors
  CopyFiles /SILENT "$INSTDIR\resources\libmpv-2.dll" "$INSTDIR"
  IfErrors 0 +2
    Abort "Failed to copy libmpv into application directory"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$INSTDIR\libmpv-2.dll"
!macroend
