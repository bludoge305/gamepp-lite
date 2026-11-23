; GamePP-Lite NSIS 安装脚本
;
; 构建方式（由 build-installer.cmd 自动调用）：
;   makensis -DVERSION=<版本> "-DOUTFILE=..\dist\gamepp-lite-setup-<版本>.exe" gamepp-lite.nsi
;
; 支持的命令行（与 NSIS 约定一致）：
;   gamepp-lite-setup.exe                     交互式向导
;   gamepp-lite-setup.exe /S [/D=目录]        静默安装（/D 必须是最后一个参数）
;   ... /desktop                              同时创建桌面快捷方式
;   ... /run                                  装完立即启动
;   ... /noshortcuts                          不创建快捷方式
;   ... /noregistry                           不写控制面板卸载项
;   uninstall.exe /S                          静默卸载

Unicode true

!define APP_NAME "GamePP-Lite"
!define APP_EXE "gamepp-lite.exe"
!define APP_REGKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\GamePP-Lite"
!define APP_RUNKEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define APP_RUNVALUE "GamePPLite"
!define APP_PUBLISHER "gamepp-lite"
!define APP_URL "https://github.com/bludoge305/gamepp-lite"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef OUTFILE
  !define OUTFILE "gamepp-lite-setup.exe"
!endif

!include "MUI2.nsh"
!include "FileFunc.nsh"

; ---------------------------------------------------------------------------
; 常规设置

Name "${APP_NAME} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${APP_NAME}"
InstallDirRegKey HKLM "${APP_REGKEY}" "InstallLocation"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

; PE 版本信息（右键属性可见）
VIProductVersion "${VERSION}.0"
VIFileVersion "${VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "${APP_NAME}"
VIAddVersionKey /LANG=1033 "FileDescription" "${APP_NAME} Setup - installer and uninstaller"
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "CompanyName" "${APP_PUBLISHER}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "MIT License"
VIAddVersionKey /LANG=1033 "OriginalFilename" "gamepp-lite-setup.exe"
VIAddVersionKey /LANG=2052 "ProductName" "${APP_NAME}"
VIAddVersionKey /LANG=2052 "FileDescription" "${APP_NAME} 安装程序"
VIAddVersionKey /LANG=2052 "LegalCopyright" "MIT License"

!define MUI_ICON "..\gamepp-lite\assets\app.ico"
!define MUI_UNICON "..\gamepp-lite\assets\app.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "$(LNG_RUN_AFTER_INSTALL)"

; ---------------------------------------------------------------------------
; 页面

!insertmacro MUI_PAGE_LICENSE "..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; 语言（第一项为默认）
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

; ---------------------------------------------------------------------------
; 多语言文案

LangString LNG_RUN_AFTER_INSTALL ${LANG_SIMPCHINESE} "安装完成后运行 ${APP_NAME}"
LangString LNG_RUN_AFTER_INSTALL ${LANG_ENGLISH} "Run ${APP_NAME} after installation"
LangString LNG_DESKTOP_SHORTCUT ${LANG_SIMPCHINESE} "创建桌面快捷方式"
LangString LNG_DESKTOP_SHORTCUT ${LANG_ENGLISH} "Create desktop shortcut"
LangString LNG_NO_SHORTCUTS ${LANG_SIMPCHINESE} "不创建任何快捷方式"
LangString LNG_NO_SHORTCUTS ${LANG_ENGLISH} "Do not create any shortcuts"
LangString LNG_NO_REGISTRY ${LANG_SIMPCHINESE} "不写控制面板卸载项"
LangString LNG_NO_REGISTRY ${LANG_ENGLISH} "Do not write the Add/Remove Programs entry"
LangString LNG_INSTALLING ${LANG_SIMPCHINESE} "正在安装 ${APP_NAME} ${VERSION}..."
LangString LNG_INSTALLING ${LANG_ENGLISH} "Installing ${APP_NAME} ${VERSION}..."
LangString LNG_DONE ${LANG_SIMPCHINESE} "安装完成。"
LangString LNG_DONE ${LANG_ENGLISH} "Installation complete."
LangString LNG_UNINSTALLED ${LANG_SIMPCHINESE} "${APP_NAME} 已卸载。"
LangString LNG_UNINSTALLED ${LANG_ENGLISH} "${APP_NAME} has been uninstalled."

; ---------------------------------------------------------------------------
; 安装段

Section "Install" SEC_INSTALL
  SectionIn RO

  SetOutPath "$INSTDIR"
  ; 升级安装时先结束正在运行的实例，否则 exe 被占用会写失败
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM "${APP_EXE}"'
  Pop $0
  File "..\gamepp-lite\target\release\${APP_EXE}"
  File "..\gamepp-lite\README.md"
  File "..\LICENSE"

  SetOutPath "$INSTDIR\docs"
  File "..\report\gamepp-four-features-reverse-report.md"
  File "..\report\gamepp-winopt-reverse-report.md"
  File "..\gamepp-lite\docs\architecture.md"

  ; 卸载器（放最后，包含前面所有文件的清单）
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; 快捷方式（/noshortcuts 可关掉；默认只建开始菜单，/desktop 加桌面）
  ReadIniStr $0 "$PLUGINSDIR\flags.ini" "flags" "noshortcuts"
  ${If} $0 != "1"
    SetShellVarContext all
    CreateDirectory "$SMPROGRAMS\${APP_NAME}"
    CreateShortCut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
    CreateShortCut "$SMPROGRAMS\${APP_NAME}\卸载 ${APP_NAME}.lnk" "$INSTDIR\uninstall.exe" "" "$INSTDIR\uninstall.exe" 0
    ReadIniStr $1 "$PLUGINSDIR\flags.ini" "flags" "desktop"
    ${If} $1 == "1"
      CreateShortCut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
    ${EndIf}
  ${EndIf}

  ; 控制面板卸载项（/noregistry 可关掉）
  ReadIniStr $2 "$PLUGINSDIR\flags.ini" "flags" "noregistry"
  ${If} $2 != "1"
    WriteRegStr HKLM "${APP_REGKEY}" "DisplayName" "${APP_NAME}"
    WriteRegStr HKLM "${APP_REGKEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKLM "${APP_REGKEY}" "Publisher" "${APP_PUBLISHER}"
    WriteRegStr HKLM "${APP_REGKEY}" "URLInfoAbout" "${APP_URL}"
    WriteRegStr HKLM "${APP_REGKEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKLM "${APP_REGKEY}" "DisplayIcon" "$INSTDIR\${APP_EXE}"
    WriteRegStr HKLM "${APP_REGKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKLM "${APP_REGKEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
    WriteRegDWORD HKLM "${APP_REGKEY}" "NoModify" 1
    WriteRegDWORD HKLM "${APP_REGKEY}" "NoRepair" 1

    ; EstimatedSize = 安装内容总大小 (KB)
    ${GetSize} "$INSTDIR" "/S=0K" $3 $4 $5
    IntFmt $3 "0x%08X" $3
    WriteRegDWORD HKLM "${APP_REGKEY}" "EstimatedSize" "$3"
  ${EndIf}
SectionEnd

; ---------------------------------------------------------------------------
; 卸载段

Function un.onInit
  ; 与安装一致：删 HKLM 卸载项必须用 64 位注册表视图
  SetRegView 64
FunctionEnd

Section "Uninstall"
  SetShellVarContext all

  ; 卸载时进程还在跑会导致删不掉 exe、留下半个安装（需重启才能清）
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM "${APP_EXE}"'
  Pop $0

  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\卸载 ${APP_NAME}.lnk"
  RMDir "$SMPROGRAMS\${APP_NAME}"
  Delete "$DESKTOP\${APP_NAME}.lnk"

  DeleteRegKey HKLM "${APP_REGKEY}"
  ; 用户可能在程序里开过"开机自启"（写 HKCU Run），卸载时一并清掉
  DeleteRegValue HKCU "${APP_RUNKEY}" "${APP_RUNVALUE}"

  ; 只删除我们安装过的文件；目录里有用户自己的文件时 RMDir 会安全失败
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\uninstall.exe"
  RMDir /r "$INSTDIR\docs"
  RMDir "$INSTDIR"
SectionEnd

; ---------------------------------------------------------------------------
; .onInit：解析命令行开关，处理 /uninstall

Function .onInit
  ; NSIS 本体是 32 位进程；目标是 64 位应用，注册表必须用 64 位视图，
  ; 否则卸载项会跑到 WOW6432Node 下，"程序和功能"里读不到。
  SetRegView 64

  ; 把开关落成 ini，给 Section 用（GetOptions 结果不能跨段直接当字符串比较用，落盘最省心）
  InitPluginsDir

  ; /desktop
  ${GetOptions} $CMDLINE "/desktop" $R0
  ${If} ${Errors}
    WriteINIStr "$PLUGINSDIR\flags.ini" "flags" "desktop" "0"
  ${Else}
    WriteINIStr "$PLUGINSDIR\flags.ini" "flags" "desktop" "1"
  ${EndIf}

  ; /noshortcuts
  ${GetOptions} $CMDLINE "/noshortcuts" $R0
  ${If} ${Errors}
    WriteINIStr "$PLUGINSDIR\flags.ini" "flags" "noshortcuts" "0"
  ${Else}
    WriteINIStr "$PLUGINSDIR\flags.ini" "flags" "noshortcuts" "1"
  ${EndIf}

  ; /noregistry
  ${GetOptions} $CMDLINE "/noregistry" $R0
  ${If} ${Errors}
    WriteINIStr "$PLUGINSDIR\flags.ini" "flags" "noregistry" "0"
  ${Else}
    WriteINIStr "$PLUGINSDIR\flags.ini" "flags" "noregistry" "1"
  ${EndIf}

  ; /run（静默安装时装完启动；交互式交给完成页的复选框）
  ${GetOptions} $CMDLINE "/run" $R0
  ${IfNot} ${Errors}
    WriteINIStr "$PLUGINSDIR\flags.ini" "flags" "run" "1"
  ${EndIf}

  ; /uninstall：复用安装包本体当卸载器（读注册表里的 InstallLocation）
  ${GetOptions} $CMDLINE "/uninstall" $R0
  ${IfNot} ${Errors}
    ReadRegStr $0 HKLM "${APP_REGKEY}" "InstallLocation"
    ${If} $0 == ""
      StrCpy $0 "$EXEDIR"
    ${EndIf}
    ${If} ${FileExists} "$0\uninstall.exe"
      ExecWait '"$0\uninstall.exe" /S _?=$0'
    ${EndIf}
    Quit
  ${EndIf}
FunctionEnd

; 静默安装 + /run：装完拉起主程序
Function .onInstSuccess
  ReadIniStr $0 "$PLUGINSDIR\flags.ini" "flags" "run"
  ${If} $0 == "1"
    ${IfNot} ${Silent}
      ; 交互式时完成页已有 MUI_FINISHPAGE_RUN，这里不再重复启动
      Return
    ${EndIf}
    Exec "$INSTDIR\${APP_EXE}"
  ${EndIf}
FunctionEnd
