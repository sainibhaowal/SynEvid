@echo off
setlocal
set "ELECTRON_RUN_AS_NODE="
set "SYNEVID_DAEMON_PORT=3333"
set "SYNEVID_COCKPIT_PORT=3000"
for %%I in ("%~dp0..") do set "REPO_ROOT=%%~fI"
cd /d "%REPO_ROOT%\apps\desktop-electron"
echo Launching SynEvid Desktop live from %REPO_ROOT%
node.exe scripts\dev.js %*
endlocal
