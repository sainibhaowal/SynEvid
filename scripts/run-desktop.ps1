# SynEvid Standalone Desktop Shell Launcher
Write-Host "=== Launching SynEvid Desktop Shell ===" -ForegroundColor Cyan
Set-Location -LiteralPath $env:USERPROFILE
npx --yes electron "\\wsl.localhost\Ubuntu\home\ravi\Projects Beta\SynEvid\apps\desktop-electron\src\main.js"
