; Snappy's setup program, built by tools/release/windows.ps1 with NSIS 3:
;
;   makensis /DVERSION=<version> /DARCH=<x64|arm64> /DBINDIR=<folder with the programs>
;            /DROOT=<repository> /DOUTFILE=<setup.exe> packaging\windows\snappy.nsi
;
; A per-user install, with no administrator rights, into %LOCALAPPDATA%\Programs\Snappy,
; where the README's manual install and the MSI put Snappy too.

Unicode true
ManifestDPIAware true
RequestExecutionLevel user
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"

!define PRODUCT "Snappy"
!define PUBLISHER "Syed Zaidi"
!define HOMEPAGE "https://github.com/farhadzaidi/snappy"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Snappy"
; Snappy's "Open at login": the value Snappy under this key.
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
; The class of the running daemon's hidden window.
!define DAEMON_WINDOW_CLASS "Snappy.Daemon"

Name "${PRODUCT}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Snappy"
BrandingText "${PRODUCT} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCT}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${PRODUCT} setup"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "${PUBLISHER}"

!define MUI_ICON "${ROOT}\assets\icon\snappy.ico"
!define MUI_UNICON "${ROOT}\assets\icon\snappy.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\snappy.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open Snappy"
!define MUI_FINISHPAGE_NOREBOOTSUPPORT

!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; Quits a running Snappy the way its menu's Quit does, which brings every hidden workspace
; window back on screen, and waits up to 30 seconds for it to finish. The daemon's hidden
; window takes a close request as Quit; the window goes away when the daemon exits.
!macro QuitSnappy
    FindWindow $0 "${DAEMON_WINDOW_CLASS}"
    ${If} $0 <> 0
        DetailPrint "Quitting Snappy..."
        SendMessage $0 ${WM_CLOSE} 0 0 /TIMEOUT=5000
        StrCpy $1 0
        ${Do}
            Sleep 200
            FindWindow $0 "${DAEMON_WINDOW_CLASS}"
            ${If} $0 = 0
                ${ExitDo}
            ${EndIf}
            IntOp $1 $1 + 1
        ${LoopUntil} $1 >= 150
        ; Give the process a moment to let go of its files after its window closes.
        Sleep 1000
    ${EndIf}
!macroend

Section "Snappy"
    !insertmacro QuitSnappy

    SetOutPath "$INSTDIR"
    File "${BINDIR}\snappy.exe"
    File "${BINDIR}\snappy-app.exe"
    WriteUninstaller "$INSTDIR\uninstall.exe"

    CreateShortcut "$SMPROGRAMS\Snappy.lnk" "$INSTDIR\snappy.exe"

    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "${HOMEPAGE}"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\snappy.exe"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
    !insertmacro QuitSnappy

    ; Remove "Open at login" only when it starts this copy of Snappy.
    ReadRegStr $0 HKCU "${RUN_KEY}" "Snappy"
    StrCpy $1 '"$INSTDIR\snappy.exe"'
    StrLen $2 $1
    StrCpy $3 $0 $2
    ${If} $3 == $1
        DeleteRegValue HKCU "${RUN_KEY}" "Snappy"
    ${EndIf}

    Delete "$SMPROGRAMS\Snappy.lnk"
    Delete /REBOOTOK "$INSTDIR\snappy.exe"
    Delete /REBOOTOK "$INSTDIR\snappy-app.exe"
    Delete "$INSTDIR\uninstall.exe"
    RMDir /REBOOTOK "$INSTDIR"
    DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
