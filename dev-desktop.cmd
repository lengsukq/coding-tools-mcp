@echo off
REM Desktop dev launcher (uses @tauri-apps/cli from node_modules)
cd /d "%~dp0"
if not exist "node_modules\@tauri-apps\cli" (
  echo Installing pnpm dependencies...
  call pnpm install --frozen-lockfile
  if errorlevel 1 exit /b 1
)
call pnpm run desktop
