Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
!include "WinVer.nsh"
!include "FileFunc.nsh"

!ifndef VERSION
  !error "VERSION is required"
!endif
!ifndef PAYLOAD
  !error "PAYLOAD is required"
!endif
!ifndef OUTPUT
  !error "OUTPUT is required"
!endif

Name "DrawDock"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\DrawDock"
RequestExecutionLevel user
SetCompressor /SOLID lzma
VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "DrawDock"
VIAddVersionKey /LANG=1033 "CompanyName" "Mike Cerreto"
VIAddVersionKey /LANG=1033 "FileDescription" "DrawDock Windows Installer"
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "Copyright (c) Mike Cerreto"
!define MUI_ICON "..\..\apps\dd-host\assets\drawdock.ico"
!define MUI_UNICON "..\..\apps\dd-host\assets\drawdock.ico"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_ICONSTOP "DrawDock requires Windows 10 or later."
    Abort
  ${EndIf}
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "DrawDock requires 64-bit Windows."
    Abort
  ${EndIf}
  SetShellVarContext current
  SetRegView 64
  StrCpy $INSTDIR "$LOCALAPPDATA\Programs\DrawDock"
FunctionEnd

Function un.onInit
  SetShellVarContext current
  SetRegView 64
  StrCpy $INSTDIR "$LOCALAPPDATA\Programs\DrawDock"
FunctionEnd

Section "DrawDock" SEC_MAIN
  SetOutPath "$INSTDIR"
  File "${PAYLOAD}\dd-host.exe"
  File "${PAYLOAD}\drawdock-launcher.exe"
  File "${PAYLOAD}\LICENSE"
  File "${PAYLOAD}\THIRD-PARTY-NOTICES.txt"
  SetOutPath "$INSTDIR\runtime"
  File /r "${PAYLOAD}\runtime\*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\DrawDock"
  CreateShortcut "$SMPROGRAMS\DrawDock\DrawDock.lnk" "$INSTDIR\drawdock-launcher.exe"
  CreateShortcut "$SMPROGRAMS\DrawDock\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\DrawDock" "DisplayName" "DrawDock"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\DrawDock" "Publisher" "Mike Cerreto"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\DrawDock" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\DrawDock" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\DrawDock" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\DrawDock" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\DrawDock" "DisplayIcon" "$INSTDIR\drawdock-launcher.exe"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\DrawDock" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\DrawDock" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\dd-host.exe"
  Delete "$INSTDIR\drawdock-launcher.exe"
  RMDir /r "$INSTDIR\runtime"
  IfFileExists "$INSTDIR\dd-host.exe" locked
  IfFileExists "$INSTDIR\drawdock-launcher.exe" locked
  IfFileExists "$INSTDIR\runtime\*.*" locked
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD-PARTY-NOTICES.txt"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\DrawDock\DrawDock.lnk"
  Delete "$SMPROGRAMS\DrawDock\Uninstall.lnk"
  RMDir "$SMPROGRAMS\DrawDock"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\DrawDock"
  Goto done
locked:
  MessageBox MB_ICONEXCLAMATION "Some DrawDock files are still in use. Quit DrawDock and any bundled ADB server, then run uninstall again. Pairing data has not been removed." /SD IDOK
  SetErrorLevel 1
  Abort
done:
SectionEnd
