# SynEvid Standalone Desktop Shell Launcher (v0.5.0)
Write-Host "=== Launching SynEvid Desktop Shell (v0.5.0) ===" -ForegroundColor Cyan

Write-Host "[1/1] Launching SynEvid Native Desktop Shell..." -ForegroundColor Cyan
Set-Location -LiteralPath "$env:USERPROFILE\synevid-shell-launcher"
$previousElectronRunAsNode = $env:ELECTRON_RUN_AS_NODE
$env:ELECTRON_RUN_AS_NODE = ''
try {
    Start-Process -FilePath "$env:APPDATA\npm\electron.cmd" -ArgumentList "$env:USERPROFILE\synevid-shell-launcher\run.js"
} finally {
    if ($null -eq $previousElectronRunAsNode) {
        Remove-Item Env:ELECTRON_RUN_AS_NODE -ErrorAction SilentlyContinue
    } else {
        $env:ELECTRON_RUN_AS_NODE = $previousElectronRunAsNode
    }
}
