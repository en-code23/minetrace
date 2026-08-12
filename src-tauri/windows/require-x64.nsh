!include "LogicLib.nsh"
!include "x64.nsh"

!macro NSIS_HOOK_PREINSTALL
  ${IfNot} ${IsNativeAMD64}
    MessageBox MB_ICONSTOP|MB_OK "This MineTrace installer is for Intel and AMD 64-bit Windows PCs.$\r$\n$\r$\nFor a Snapdragon or other Windows on ARM device, download the ARM64 installer instead."
    Abort
  ${EndIf}
!macroend
