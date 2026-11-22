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

rem dist file name follows the installer's Cargo.toml version (no drift)
set "VER="
for /f "tokens=2 delims==" %%v in ('findstr /b /c:"version" gamepp-winopt-installer\Cargo.toml') do if not defined VER set "VER=%%v"
if defined VER set "VER=%VER: =%"
set VER=%VER:"=%
if not defined VER (
  echo Cannot read version from gamepp-winopt-installer\Cargo.toml
  exit /b 1
)

echo [1/3] building gamepp-lite ...
"%CARGO%" build --release --manifest-path gamepp-lite\Cargo.toml
if errorlevel 1 goto fail

echo [2/3] building installer (embeds the exe built above) ...
"%CARGO%" build --release --manifest-path gamepp-winopt-installer\Cargo.toml
if errorlevel 1 goto fail

echo [3/3] publishing to dist\ ...
if not exist dist mkdir dist
copy /y gamepp-winopt-installer\target\release\gamepp-lite-setup.exe dist\gamepp-lite-setup-%VER%.exe >nul
if errorlevel 1 goto fail

echo.
echo DONE: dist\gamepp-lite-setup-%VER%.exe
exit /b 0

:fail
echo.
echo BUILD FAILED.
exit /b 1
