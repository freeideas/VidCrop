@echo off
rem Builds the Windows installer inside the Windows guest; output goes to build.log.
rem tools/release.py starts this through schtasks, because programs started from an
rem SSH session are killed when the session closes.
if not "%~1"=="run" (
  call "%~f0" run > "%~dp0..\build.log" 2>&1
  exit /b
)
cd /d "%~dp0.."
set "PATH=%USERPROFILE%\.cargo\bin;%LOCALAPPDATA%\Microsoft\WinGet\Links;%PATH%"
set CARGO_BUILD_JOBS=2
call npm ci --no-audit --no-fund || goto fail
uv run -q tools\fetch_ffmpeg.py || goto fail
call npx tauri build --bundles nsis --config src-tauri\tauri.release.json || goto fail
echo BUILD-OK
exit /b 0
:fail
echo BUILD-FAILED
exit /b 1
