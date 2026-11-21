!macro NSIS_HOOK_POSTINSTALL
  ; Depending on the bundler's resource destination mapping, libmpv may
  ; already be installed beside the EXE or under resources. If a new
  ; resource copy exists, always replace the app-private runtime (upgrade).
  IfFileExists "$INSTDIR\resources\libmpv-2.dll" hgr_copy_mpv 0
  IfFileExists "$INSTDIR\libmpv-2.dll" hgr_mpv_done hgr_mpv_missing

  hgr_copy_mpv:
    SetOutPath "$INSTDIR"
    ClearErrors
    CopyFiles /SILENT "$INSTDIR\resources\libmpv-2.dll" "$INSTDIR"
    IfErrors hgr_mpv_missing hgr_mpv_done

  hgr_mpv_missing:
    Abort "The bundled libmpv-2.dll was not installed beside HongGuoRust"

  hgr_mpv_done:
    DetailPrint "Verified libmpv runtime placement in application directory"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Remove only the app-local DLL, never a shared or system runtime.
  Delete "$INSTDIR\libmpv-2.dll"
!macroend
