@echo off
setlocal
echo === SynEvid Standalone Desktop Shell ===
cd /d "%USERPROFILE%"
npx --yes electron "\\wsl.localhost\Ubuntu\home\ravi\Projects Beta\SynEvid\apps\desktop-electron\src\main.js"
endlocal
