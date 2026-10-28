@echo off
setlocal
echo === SynEvid Standalone Desktop Shell (v0.5.0) ===

:: 1. Ensure synevid-ui daemon is active on port 3333
curl -s http://127.0.0.1:3333/api/v1/health >nul 2>&1
if %errorlevel% neq 0 (
    echo [1/3] Starting SynEvid Reactive Daemon on port 3333...
    start /b "" wsl.exe -d Ubuntu bash -lc "cd '/home/ravi/Projects Beta/SynEvid' && ./target/debug/synevid-ui --port 3333 > /tmp/synevid-ui.log 2>&1"
    timeout /t 2 /nobreak >nul
) else (
    echo [1/3] SynEvid Daemon active on port 3333.
)

:: 2. Ensure Next.js Cockpit is active on port 3000
curl -s http://127.0.0.1:3000 >nul 2>&1
if %errorlevel% neq 0 (
    echo [2/3] Starting Next.js Web Cockpit on port 3000...
    start /b "" wsl.exe -d Ubuntu bash -lc "cd '/home/ravi/Projects Beta/SynEvid/apps/web' && npx next dev -p 3000 -H 0.0.0.0 > /tmp/next-dev.log 2>&1"
    timeout /t 3 /nobreak >nul
) else (
    echo [2/3] Web Cockpit active on port 3000.
)

:: 3. Launch Native Electron Shell
echo [3/3] Launching SynEvid Native Desktop Shell...
cd /d "%USERPROFILE%"
npx --yes electron "\\wsl.localhost\Ubuntu\home\ravi\Projects Beta\SynEvid\apps\desktop-electron\src\main.js"
endlocal
