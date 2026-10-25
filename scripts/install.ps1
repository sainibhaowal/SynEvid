<#
.SYNOPSIS
    install.ps1 - Native Windows one-line installer, updater, and uninstaller for SynEvid.

.DESCRIPTION
    irm https://raw.githubusercontent.com/sainibhaowal/SynEvid/main/scripts/install.ps1 | iex

    Installs per-user (never requires Administrator privileges), so one installation
    serves every repository on the machine:
      $Prefix\bin\autopsy.exe                   the engine (default Prefix=~\.local)
      $Prefix\bin\synevid-lsp.exe               the IDE language server
      $Prefix\bin\synevid-ui.exe                the embedded web dashboard
      $Prefix\share\synevid\mcp-server\index.js single-file MCP server (agent bridge)
      $Prefix\share\synevid\setup-agent-hosts.py host-wiring helper
      $Prefix\share\synevid\hooks\*.sh          advisory & task-complete agent hooks

.PARAMETER Update
    Explicitly update to the latest release (default behavior).

.PARAMETER Check
    Report what is installed offline without modifying files or PATH.

.PARAMETER Uninstall
    Remove binaries, share files, User PATH entry, and agent-host wiring.

.PARAMETER Help
    Display usage information.
#>
[CmdletBinding()]
param(
    [switch]$Update,
    [switch]$Check,
    [switch]$Uninstall,
    [switch]$Help
)

$ErrorActionPreference = "Stop"

$RepoSlug = if ($env:SYNEVID_REPO) { $env:SYNEVID_REPO } else { "sainibhaowal/SynEvid" }
$Prefix = if ($env:SYNEVID_PREFIX) { $env:SYNEVID_PREFIX } else { Join-Path $HOME ".local" }
$BinDir = Join-Path $Prefix "bin"
$ShareDir = Join-Path $Prefix "share\synevid"
$AssetName = "autopsy-x86_64-pc-windows-msvc.zip"

function Show-Usage {
    Write-Output @"
SynEvid Windows Installer (install.ps1)

Usage:
  .\scripts\install.ps1              Install or update to the latest release (idempotent)
  .\scripts\install.ps1 -Update      Same as above (explicit)
  .\scripts\install.ps1 -Check       Report what is installed (read-only)
  .\scripts\install.ps1 -Uninstall   Remove binaries, share files, PATH entry, and host wiring
  .\scripts\install.ps1 -Help        Show this help

Environment variables:
  SYNEVID_PREFIX       Install prefix (default: ~\.local)
  SYNEVID_REPO         GitHub owner/repo slug (default: sainibhaowal/SynEvid)
  SYNEVID_NO_PATH=1    Do not modify User PATH environment variable
  SYNEVID_STRICT=1     Abort when release checksums cannot be fetched
  AUTOPSY_SKIP_HOSTS=1 Skip agent-host wiring
  AUTOPSY_SKIP_BUILD=1 Skip download/build; wire hosts against existing binaries
"@
}

function Add-UserPathEntry {
    if ($env:SYNEVID_NO_PATH -eq "1") {
        Write-Output "  PATH: skipped (SYNEVID_NO_PATH=1)"
        return
    }
    $currentUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $currentUserPath) { $currentUserPath = "" }
    $parts = $currentUserPath -split ";" | Where-Object { $_ -ne "" }
    if ($parts -contains $BinDir) {
        Write-Output "  PATH: already configured in User environment ($BinDir)"
    } else {
        $newPath = (@($BinDir) + $parts) -join ";"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Output "  PATH: added $BinDir to User PATH"
    }
    if (($env:Path -split ";") -notcontains $BinDir) {
        $env:Path = "$BinDir;$env:Path"
    }
}

function Remove-UserPathEntry {
    $currentUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $currentUserPath) {
        Write-Output "  PATH: no managed entry found"
        return
    }
    $parts = $currentUserPath -split ";" | Where-Object { $_ -ne "" -and $_ -ne $BinDir }
    $newPath = $parts -join ";"
    if ($newPath -ne $currentUserPath) {
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Output "  PATH: removed $BinDir from User PATH"
    } else {
        Write-Output "  PATH: no managed entry found"
    }
}

function Install-ExtrasFromDir([string]$SourceDir) {
    $mcpDir = Join-Path $ShareDir "mcp-server"
    $hooksDir = Join-Path $ShareDir "hooks"
    New-Item -ItemType Directory -Force -Path $mcpDir, $hooksDir | Out-Null

    foreach ($cand in @(
        (Join-Path $SourceDir "mcp-server\index.js"),
        (Join-Path $SourceDir "apps\mcp-server\dist\index.bundle.js"),
        (Join-Path $SourceDir "apps\mcp-server\dist\index.js")
    )) {
        if (Test-Path $cand) {
            Copy-Item -Force $cand (Join-Path $mcpDir "index.js")
            Write-Output "  MCP server  -> $(Join-Path $mcpDir 'index.js')"
            break
        }
    }

    foreach ($cand in @(
        (Join-Path $SourceDir "setup-agent-hosts.py"),
        (Join-Path $SourceDir "scripts\setup-agent-hosts.py")
    )) {
        if (Test-Path $cand) {
            Copy-Item -Force $cand (Join-Path $ShareDir "setup-agent-hosts.py")
            break
        }
    }

    foreach ($hook in @("safe-edit.sh", "autopsy-session.sh", "autopsy-task-complete.sh")) {
        foreach ($cand in @(
            (Join-Path $SourceDir "hooks\$hook"),
            (Join-Path $SourceDir "agents\hooks\$hook")
        )) {
            if (Test-Path $cand) {
                Copy-Item -Force $cand (Join-Path $hooksDir $hook)
                break
            }
        }
    }
}

function Invoke-HostWiring([switch]$Unwire) {
    if ($env:AUTOPSY_SKIP_HOSTS -eq "1" -and -not $Unwire) {
        Write-Output "  hosts: skipped (AUTOPSY_SKIP_HOSTS=1)"
        return
    }
    $helper = Join-Path $ShareDir "setup-agent-hosts.py"
    if (-not (Test-Path $helper)) {
        return
    }
    $py = Get-Command python -ErrorAction SilentlyContinue
    if (-not $py) { $py = Get-Command python3 -ErrorAction SilentlyContinue }
    if (-not $py) {
        Write-Warning "  hosts: python not found; wire agent hosts later with setup-agent-hosts.py"
        return
    }
    if ($Unwire) {
        & $py.Source $helper --unwire
    } else {
        $binPath = Join-Path $BinDir "autopsy.exe"
        $mcpPath = Join-Path $ShareDir "mcp-server\index.js"
        if (Test-Path $mcpPath) {
            & $py.Source $helper --autopsy-bin $binPath --mcp-server $mcpPath
        } else {
            & $py.Source $helper --autopsy-bin $binPath
        }
    }
}

if ($Help) {
    Show-Usage
    exit 0
}

if ($Check) {
    Write-Output "SynEvid installation check (prefix: $Prefix)"
    $autopsyExe = Join-Path $BinDir "autopsy.exe"
    $lspExe = Join-Path $BinDir "synevid-lsp.exe"
    $mcpJs = Join-Path $ShareDir "mcp-server\index.js"
    if (Test-Path $autopsyExe) {
        & $autopsyExe version
    } else {
        Write-Output "  autopsy: not installed (looked in $BinDir)"
    }
    if (Test-Path $lspExe) {
        & $lspExe --version
    }
    if (Test-Path $mcpJs) {
        Write-Output "  MCP server:  $mcpJs"
    } else {
        Write-Output "  MCP server:  not installed"
    }
    Write-Output "  update:      re-run install.ps1 (or: install.ps1 -Update)"
    Write-Output "  uninstall:   install.ps1 -Uninstall"
    exit 0
}

if ($Uninstall) {
    Write-Output "Uninstalling SynEvid from $Prefix ..."
    Invoke-HostWiring -Unwire
    Remove-UserPathEntry
    $removed = $false
    foreach ($name in @("autopsy.exe", "autopsy", "synevid-lsp.exe", "synevid-lsp", "synevid-ui.exe", "synevid-ui")) {
        $p = Join-Path $BinDir $name
        if (Test-Path $p) {
            Remove-Item -Force $p
            Write-Output "  removed $p"
            $removed = $true
        }
    }
    if (Test-Path $ShareDir) {
        Remove-Item -Recurse -Force $ShareDir
        Write-Output "  removed $ShareDir"
        $removed = $true
    }
    if (-not $removed) {
        Write-Output "  nothing to remove (SynEvid is not installed in $Prefix)"
    }
    Write-Output "Done. Open a new terminal so the PATH change takes effect."
    exit 0
}

Write-Output "SynEvid Windows installer (windows-x86_64)"
New-Item -ItemType Directory -Force -Path $BinDir, $ShareDir | Out-Null

if ($env:AUTOPSY_SKIP_BUILD -eq "1") {
    $autopsyExe = Join-Path $BinDir "autopsy.exe"
    if (-not (Test-Path $autopsyExe)) {
        Write-Error "AUTOPSY_SKIP_BUILD=1 but no autopsy.exe in $BinDir"
        exit 1
    }
    Write-Output "skipping download/build (AUTOPSY_SKIP_BUILD=1)"
} else {
    $installed = $false
    $tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("synevid-install-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null
    try {
        $zipPath = Join-Path $tmpDir $AssetName
        $sumsPath = Join-Path $tmpDir "SHA256SUMS.txt"
        $baseUrl = if ($env:SYNEVID_RELEASE_BASE_URL) {
            $env:SYNEVID_RELEASE_BASE_URL
        } else {
            "https://github.com/$RepoSlug/releases/latest/download"
        }

        $downloaded = $false
        try {
            if ($baseUrl.StartsWith("file://")) {
                $localDir = $baseUrl.Substring(7)
                Copy-Item -ErrorAction Stop (Join-Path $localDir $AssetName) $zipPath
                $downloaded = $true
            } else {
                Invoke-WebRequest -Uri "$baseUrl/$AssetName" -OutFile $zipPath -UseBasicParsing -ErrorAction Stop
                $downloaded = $true
            }
        } catch {
            $downloaded = $false
        }

        if ($downloaded) {
            $sumsFetched = $false
            try {
                if ($baseUrl.StartsWith("file://")) {
                    $localDir = $baseUrl.Substring(7)
                    Copy-Item -ErrorAction Stop (Join-Path $localDir "SHA256SUMS.txt") $sumsPath
                    $sumsFetched = $true
                } else {
                    Invoke-WebRequest -Uri "$baseUrl/SHA256SUMS.txt" -OutFile $sumsPath -UseBasicParsing -ErrorAction Stop
                    $sumsFetched = $true
                }
            } catch {
                $sumsFetched = $false
            }

            if ($sumsFetched) {
                $expected = $null
                foreach ($line in Get-Content $sumsPath) {
                    if ($line -match "^([0-9a-fA-F]{64})\s+\*?$([Regex]::Escape($AssetName))$") {
                        $expected = $Matches[1].ToLowerInvariant()
                        break
                    }
                }
                if ($expected) {
                    $actual = (Get-FileHash -Algorithm SHA256 -Path $zipPath).Hash.ToLowerInvariant()
                    if ($actual -ne $expected) {
                        Write-Error "checksum MISMATCH for $AssetName (expected $expected, got $actual); aborting"
                        exit 2
                    }
                    Write-Output "  checksum verified against SHA256SUMS.txt"
                } elseif ($env:SYNEVID_STRICT -eq "1") {
                    Write-Error "checksum entry missing and SYNEVID_STRICT=1; aborting"
                    exit 2
                }
            } elseif ($env:SYNEVID_STRICT -eq "1") {
                Write-Error "release checksums unavailable and SYNEVID_STRICT=1; aborting"
                exit 2
            }

            $outDir = Join-Path $tmpDir "out"
            Expand-Archive -Path $zipPath -DestinationPath $outDir -Force
            $foundExe = Get-ChildItem -Path $outDir -Recurse -Filter "autopsy.exe" | Select-Object -First 1
            if ($foundExe) {
                $rootDir = $foundExe.Directory.FullName
                foreach ($binName in @("autopsy.exe", "synevid-lsp.exe", "synevid-ui.exe")) {
                    $srcBin = Join-Path $rootDir $binName
                    if (Test-Path $srcBin) {
                        Copy-Item -Force $srcBin (Join-Path $BinDir $binName)
                    }
                }
                Install-ExtrasFromDir $rootDir
                $installed = $true
                Write-Output "installed prebuilt release for windows-x86_64"
            }
        }
    } finally {
        Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
    }

    if (-not $installed) {
        Write-Output "no prebuilt release available; building from source with cargo..."
        $scriptDir = if ($PSScriptRoot) { $PSScriptRoot } else { "" }
        $repoCheckout = if ($scriptDir -and (Test-Path (Join-Path $scriptDir "..\Cargo.toml"))) {
            (Resolve-Path (Join-Path $scriptDir "..")).Path
        } else {
            $null
        }
        if (-not $repoCheckout) {
            Write-Error "No prebuilt release found and not running from a repository checkout."
            exit 1
        }
        Push-Location $repoCheckout
        try {
            & cargo build --release --locked -p autopsy-cli -p synevid-lsp -p synevid-ui
            Copy-Item -Force "target\release\autopsy.exe" (Join-Path $BinDir "autopsy.exe")
            if (Test-Path "target\release\synevid-lsp.exe") {
                Copy-Item -Force "target\release\synevid-lsp.exe" (Join-Path $BinDir "synevid-lsp.exe")
            }
            if (Test-Path "target\release\synevid-ui.exe") {
                Copy-Item -Force "target\release\synevid-ui.exe" (Join-Path $BinDir "synevid-ui.exe")
            }
            Install-ExtrasFromDir $repoCheckout
        } finally {
            Pop-Location
        }
        Write-Output "built from source"
    }
}

Write-Output "configuring User PATH..."
Add-UserPathEntry

Write-Output "wiring agent hosts (Claude Code, Claude Desktop, Cursor, VS Code, Windsurf, opencode, Gemini CLI)..."
Invoke-HostWiring

Write-Output "verifying installation..."
$installedAutopsy = Join-Path $BinDir "autopsy.exe"
if (Test-Path $installedAutopsy) {
    & $installedAutopsy version
} else {
    Write-Error "verification failed: autopsy.exe missing in $BinDir"
    exit 1
}

Write-Output ""
Write-Output "SynEvid is installed - one installation serves every repository on this machine."
Write-Output "  autopsy     -> $installedAutopsy"
if (Test-Path (Join-Path $BinDir "synevid-lsp.exe")) {
    Write-Output "  synevid-lsp -> $(Join-Path $BinDir 'synevid-lsp.exe')"
}
if (Test-Path (Join-Path $BinDir "synevid-ui.exe")) {
    Write-Output "  synevid-ui  -> $(Join-Path $BinDir 'synevid-ui.exe')"
}
if (Test-Path (Join-Path $ShareDir "mcp-server\index.js")) {
    Write-Output "  MCP server  -> $(Join-Path $ShareDir 'mcp-server\index.js')"
}
Write-Output ""
Write-Output "Try it:  cd <any-repo>; autopsy init; autopsy doctor"
Write-Output "Update:  re-run install.ps1   Remove: install.ps1 -Uninstall"
