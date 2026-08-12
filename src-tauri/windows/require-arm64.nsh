!include "LogicLib.nsh"
!include "x64.nsh"

!macro NSIS_HOOK_PREINSTALL
  ${IfNot} ${IsNativeARM64}
    MessageBox MB_ICONSTOP|MB_OK "This MineTrace installer is for Snapdragon and other Windows on ARM devices.$\r$\n$\r$\nFor an Intel or AMD Windows PC, download the x64 installer instead."
    Abort
  ${EndIf}
!macroend
