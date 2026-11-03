# SynEvid live native Electron launcher.
$ErrorActionPreference = 'Stop'

$env:ELECTRON_RUN_AS_NODE = ''
$env:SYNEVID_DAEMON_PORT = '3333'
$env:SYNEVID_COCKPIT_PORT = '3000'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$desktopRoot = Join-Path $repoRoot 'apps\desktop-electron'
$node = (Get-Command node.exe -ErrorAction Stop).Source

Write-Host "Launching SynEvid Desktop live from $repoRoot"
Push-Location $desktopRoot
try {
    & $node 'scripts\dev.js' @args
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}
