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
rem The guest's C: drive is nearly full; keep downloads, caches and temp files on the build drive.
set "CARGO_HOME=%~d0\cargo-home"
set "npm_config_cache=%~d0\npm-cache"
set "UV_CACHE_DIR=%~d0\uv-cache"
set "TEMP=%~d0\tmp"
set "TMP=%~d0\tmp"
if not exist "%TEMP%" mkdir "%TEMP%"
call npm ci --no-audit --no-fund || goto fail
uv run -q tools\fetch_ffmpeg.py || goto fail
call npx tauri build --bundles nsis --config src-tauri\tauri.release.json || goto fail
echo BUILD-OK
exit /b 0
:fail
echo BUILD-FAILED
exit /b 1
