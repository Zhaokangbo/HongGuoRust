[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Installer,
    [string]$AuditReport,
    [string]$CandidateCommit
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'NSIS smoke requires Windows' }

$setup = (Resolve-Path -LiteralPath $Installer).Path
$expectedHash = 'df006bfc754d1212fb2084c6a6da04876882648c025ccb56cf1132b27b1e7913'
# RUNNER_TEMP exists on GitHub Actions but not on a normal Windows desktop.
# Keep local smoke tests runnable without requiring the CI environment.
$smokeRoot = if (![string]::IsNullOrWhiteSpace($env:RUNNER_TEMP) -and
    (Test-Path -LiteralPath $env:RUNNER_TEMP -PathType Container)) {
    $env:RUNNER_TEMP
} else {
    [IO.Path]::GetTempPath()
}
$dir = Join-Path $smokeRoot ('HongGuoRustM5Smoke-' + [guid]::NewGuid().ToString('N'))
$exe = Join-Path $dir 'hongguorust.exe'
$dll = Join-Path $dir 'libmpv-2.dll'
$vulkan = Join-Path $dir 'vulkan-1.dll'
$installed = $false
$uninstalled = $false
$auditEvidence = $null

function Run-Nsis([string]$file, [string[]]$arguments, [string]$stage) {
    $proc = Start-Process -FilePath $file -ArgumentList $arguments -PassThru -Wait
    if ($proc.ExitCode -ne 0) { throw "$stage exit code $($proc.ExitCode)" }
}

try {
    if (Test-Path -LiteralPath $dir) { throw 'Refusing to reuse an installation directory' }
    Write-Host "Installing NSIS test candidate into $dir"
    # /D overrides NSIS install directory. It must be the last option, without quotes.
    try {
        Run-Nsis -file $setup -arguments @('/S', "/D=$dir") -stage 'NSIS installation'
    } catch {
        Write-Warning 'NSIS installation failed. Installed file layout (if any):'
        if (Test-Path -LiteralPath $dir) {
            Get-ChildItem -LiteralPath $dir -Recurse -File -ErrorAction SilentlyContinue |
                Select-Object -First 50 -ExpandProperty FullName |
                ForEach-Object { Write-Host "  $_" }
        } else {
            Write-Host "  No destination directory created: $dir"
        }
        throw
    }
    $installed = $true

    if (!(Test-Path -LiteralPath $exe -PathType Leaf)) {
        throw "NSIS installed no application executable at $exe"
    }
    if (!(Test-Path -LiteralPath $dll -PathType Leaf)) {
        $files = @(Get-ChildItem -LiteralPath $dir -Recurse -Filter 'libmpv-2.dll' -File |
            ForEach-Object FullName)
        throw "NSIS did not place libmpv-2.dll next to EXE. Found: $($files -join '; ')"
    }
    $hash = (Get-FileHash -LiteralPath $dll -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $expectedHash) { throw "Installed libmpv DLL hash mismatch: $hash" }
    Write-Host "PASS: installed EXE and verified libmpv-2.dll SHA256=$hash"
    if (!(Test-Path -LiteralPath $vulkan -PathType Leaf)) {
        throw "Installed app has no private vulkan-1.dll: $vulkan"
    }
    Write-Host "PASS: app-local Vulkan loader SHA256=$((Get-FileHash -LiteralPath $vulkan -Algorithm SHA256).Hash)"

    # Inventory only metadata from the installed app. No binaries are copied
    # into the report or into the GitHub Actions artifact.
    if (![string]::IsNullOrWhiteSpace($AuditReport)) {
        $inventory = @(
            Get-ChildItem -LiteralPath $dir -File -Recurse |
                Sort-Object FullName |
                ForEach-Object {
                    [ordered]@{
                        path = $_.FullName.Substring($dir.Length + 1).Replace('\', '/')
                        bytes = $_.Length
                        sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                    }
                }
        )
        $noticeCandidates = @(
            $inventory | Where-Object {
                (Split-Path $_.path -Leaf) -match '^(LICENSE|COPYING|NOTICE|THIRD[-_.]?PARTY)([._-].*)?$'
            } | ForEach-Object { $_.path }
        )
        # PR workflows check out a synthetic merge commit by default.
        # Record both the real PR head and the tree actually built by Actions.
        $sourceCommit = if (![string]::IsNullOrWhiteSpace($CandidateCommit)) {
            $CandidateCommit
        } else {
            $env:GITHUB_SHA
        }
        $auditEvidence = [ordered]@{
            schema_version = 1
            candidate_commit = $sourceCommit
            workflow_checkout_sha = $env:GITHUB_SHA
            installer_name = (Split-Path -Leaf $setup)
            installer_sha256 = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
            installed_files = $inventory
            installed_notice_candidates = $noticeCandidates
            clean_windows_10_11_verified = $false
            redistribution_provenance_reviewed = $false
            distribution_authorized = $false
            note = 'Hosted runner evidence only; not clean-machine or third-party redistribution acceptance.'
        }
    }

    # The preexisting M1 UI automation asserts Rust IPC and normal process exit.
    try {
        & "$PSScriptRoot/smoke-windows.ps1" -Executable $exe
        if (!$?) { throw 'Installed application IPC/window smoke failed' }
    } catch {
        Write-Warning 'Installed app GUI/IPC smoke failed. Inspect uploaded diagnostics.'
        $diag = Join-Path $PSScriptRoot '../diagnostics'
        $logs = @(Get-ChildItem -LiteralPath $diag -Filter 'player-*.log' -File -ErrorAction SilentlyContinue)
        if ($logs.Count -gt 0) {
            Write-Host "Latest player log: $($logs[-1].Name)"
            Get-Content -LiteralPath $logs[-1].FullName -Tail 55 -Encoding UTF8
        }
        throw
    }
    Write-Host 'PASS: installed app launched, rendered Rust Idle and closed cleanly'

    $uninstallers = @(Get-ChildItem -LiteralPath $dir -File |
        Where-Object { $_.Name -match '^uninstall.*\.exe$' })
    if ($uninstallers.Count -ne 1) {
        throw "Expected exactly one uninstaller, found $($uninstallers.Count)"
    }
    Write-Host "Uninstalling from $dir"
    Run-Nsis -file $uninstallers[0].FullName -arguments @('/S') -stage 'NSIS uninstallation'
    $limit = [DateTime]::UtcNow.AddSeconds(20)
    while ((Test-Path -LiteralPath $exe) -and [DateTime]::UtcNow -lt $limit) {
        Start-Sleep -Milliseconds 250
    }
    if (Test-Path -LiteralPath $exe) { throw 'Uninstaller left application EXE behind' }
    if (Test-Path -LiteralPath $dll) { throw 'Uninstaller left app-private libmpv DLL behind' }
    if (Test-Path -LiteralPath $vulkan) { throw 'Uninstaller left app-private Vulkan DLL behind' }
    $uninstalled = $true
    Write-Host 'PASS: NSIS silent install, app IPC, DLL SHA-256 and uninstall'
    if ($auditEvidence) {
        $reportPath = [IO.Path]::GetFullPath($AuditReport)
        $reportDir = Split-Path -Parent $reportPath
        New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
        $auditEvidence | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $reportPath -Encoding utf8
        Write-Host "Wrote binary-free package evidence: $reportPath"
    }
} finally {
    if ($installed -and !$uninstalled) {
        Write-Warning "A failed installer smoke may have left a test installation at $dir"
    }
}
