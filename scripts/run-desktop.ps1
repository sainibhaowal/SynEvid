# SynEvid Standalone Desktop Shell Launcher (v0.5.0)
Write-Host "=== Launching SynEvid Desktop Shell (v0.5.0) ===" -ForegroundColor Cyan

# 1. Check & start synevid-ui daemon
try {
    $dCheck = (Invoke-WebRequest -Uri "http://127.0.0.1:3333/api/v1/health" -UseBasicParsing -TimeoutSec 1 -ErrorAction SilentlyContinue).StatusCode
} catch { $dCheck = 0 }

if ($dCheck -ne 200) {
    Write-Host "[1/3] Starting SynEvid Reactive Daemon on port 3333..." -ForegroundColor Yellow
    Start-Process -FilePath "wsl.exe" -ArgumentList "-d", "Ubuntu", "bash", "-lc", "cd '/home/ravi/Projects Beta/SynEvid' && ./target/debug/synevid-ui --port 3333 > /tmp/synevid-ui.log 2>&1" -WindowStyle Hidden
    Start-Sleep -Seconds 2
} else {
    Write-Host "[1/3] SynEvid Daemon is active on port 3333." -ForegroundColor Green
}

# 2. Check & start Next.js Cockpit
try {
    $wCheck = (Invoke-WebRequest -Uri "http://127.0.0.1:3000" -UseBasicParsing -TimeoutSec 1 -ErrorAction SilentlyContinue).StatusCode
} catch { $wCheck = 0 }

if ($wCheck -ne 200) {
    Write-Host "[2/3] Starting Next.js Web Cockpit on port 3000..." -ForegroundColor Yellow
    Start-Process -FilePath "wsl.exe" -ArgumentList "-d", "Ubuntu", "bash", "-lc", "cd '/home/ravi/Projects Beta/SynEvid/apps/web' && npx next dev -p 3000 -H 0.0.0.0 > /tmp/next-dev.log 2>&1" -WindowStyle Hidden
    Start-Sleep -Seconds 3
} else {
    Write-Host "[2/3] Web Cockpit is active on port 3000." -ForegroundColor Green
}

# 3. Launch Electron Desktop Shell
Write-Host "[3/3] Launching SynEvid Native Desktop Shell..." -ForegroundColor Cyan
Set-Location -LiteralPath $env:USERPROFILE
npx --yes electron "\\wsl.localhost\Ubuntu\home\ravi\Projects Beta\SynEvid\apps\desktop-electron\src\main.js"
