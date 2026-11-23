@echo off
setlocal
cd /d "%~dp0"

rem windows-gnu target + windows-sys(raw-dylib) needs dlltool + as on PATH
set "BINUTILS=%USERPROFILE%\.cargo\mingw-binutils\bin"
if exist "%BINUTILS%\dlltool.exe" set "PATH=%BINUTILS%;%PATH%"

set "CARGO=cargo"
where cargo >nul 2>nul
if errorlevel 1 (
  if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
)

rem locate makensis (NSIS 3)
set "MAKENSIS="
if defined NSIS_HOME if exist "%NSIS_HOME%\makensis.exe" set "MAKENSIS=%NSIS_HOME%\makensis.exe"
if not defined MAKENSIS if exist "%ProgramFiles(x86)%\NSIS\makensis.exe" set "MAKENSIS=%ProgramFiles(x86)%\NSIS\makensis.exe"
if not defined MAKENSIS if exist "%ProgramFiles%\NSIS\makensis.exe" set "MAKENSIS=%ProgramFiles%\NSIS\makensis.exe"
if not defined MAKENSIS (
  echo NSIS not found. Install with: winget install NSIS.NSIS
  exit /b 1
)

rem dist file name follows gamepp-lite's Cargo.toml version
set "VER="
for /f "tokens=2 delims==" %%v in ('findstr /b /c:"version" gamepp-lite\Cargo.toml') do if not defined VER set "VER=%%v"
if defined VER set "VER=%VER: =%"
set VER=%VER:"=%
if not defined VER (
  echo Cannot read version from gamepp-lite\Cargo.toml
  exit /b 1
)

echo [1/2] building gamepp-lite (release) ...
"%CARGO%" build --release --locked --manifest-path gamepp-lite\Cargo.toml
if errorlevel 1 goto fail

echo [2/2] packing installer with NSIS (v%VER%) ...
if not exist dist mkdir dist
"%MAKENSIS%" /DVERSION=%VER% /DOUTFILE="%~dp0dist\gamepp-lite-setup-%VER%.exe" "%~dp0installer\gamepp-lite.nsi"
if errorlevel 1 goto fail

echo.
echo DONE: dist\gamepp-lite-setup-%VER%.exe
exit /b 0

:fail
echo.
echo BUILD FAILED.
exit /b 1
