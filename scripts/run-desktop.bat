@echo off
setlocal
echo === SynEvid Standalone Desktop Shell (v0.5.0) ===

echo [1/1] Launching SynEvid Native Desktop Shell...
cd /d "%USERPROFILE%\synevid-shell-launcher"
set "ELECTRON_RUN_AS_NODE="
call "%APPDATA%\npm\electron.cmd" "%USERPROFILE%\synevid-shell-launcher\run.js"
endlocal
