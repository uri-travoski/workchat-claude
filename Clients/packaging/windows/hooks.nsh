; NSIS installer hooks for WorkChat (referenced by bundle.windows.nsis.installerHooks).
; Registers the workchat:// URL scheme. SHCTX is HKCU for a per-user install and HKLM for a per-machine one.

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr SHCTX "Software\Classes\workchat" "" "URL:WorkChat"
  WriteRegStr SHCTX "Software\Classes\workchat" "URL Protocol" ""
  WriteRegStr SHCTX "Software\Classes\workchat\DefaultIcon" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
  WriteRegStr SHCTX "Software\Classes\workchat\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
!macroend

!macro NSIS_HOOK_PREUNINSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegKey SHCTX "Software\Classes\workchat"
!macroend
