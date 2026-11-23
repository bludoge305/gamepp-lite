@echo off
setlocal
cd /d "%~dp0"

set "BINUTILS=%USERPROFILE%\.cargo\mingw-binutils\bin"
if exist "%BINUTILS%\dlltool.exe" (
  set "PATH=%BINUTILS%;%PATH%"
) else (
  echo [warn] binutils not found at %BINUTILS%
  echo [warn] windows-gnu builds of windows-sys need dlltool+as; build may fail.
)

set "CARGO=cargo"
where cargo >nul 2>nul
if errorlevel 1 (
  if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
)

echo [1/1] building gamepp-lite (release) ...
"%CARGO%" build --release --locked --manifest-path "%~dp0Cargo.toml"
if errorlevel 1 goto fail

echo.
echo DONE: %~dp0target\release\gamepp-lite.exe
exit /b 0

:fail
echo.
echo BUILD FAILED.
exit /b 1
