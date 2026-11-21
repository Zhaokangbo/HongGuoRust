[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Installer
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'NSIS smoke requires Windows' }

$setup = (Resolve-Path -LiteralPath $Installer).Path
$expectedHash = 'df006bfc754d1212fb2084c6a6da04876882648c025ccb56cf1132b27b1e7913'
$dir = Join-Path $env:RUNNER_TEMP ('HongGuoRustM5Smoke-' + [guid]::NewGuid().ToString('N'))
$exe = Join-Path $dir 'hongguorust.exe'
$dll = Join-Path $dir 'libmpv-2.dll'
$installed = $false
$uninstalled = $false

function Run-Nsis([string]$file, [string[]]$arguments, [string]$stage) {
    $proc = Start-Process -FilePath $file -ArgumentList $arguments -PassThru -Wait
    if ($proc.ExitCode -ne 0) { throw "$stage exit code $($proc.ExitCode)" }
}

try {
    if (Test-Path -LiteralPath $dir) { throw 'Refusing to reuse an installation directory' }
    Write-Host "Installing NSIS test candidate into $dir"
    # /D overrides NSIS install directory. It must be the last option, without quotes.
    Run-Nsis $setup @('/S', "/D=$dir") 'NSIS installation'
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

    # The preexisting M1 UI automation asserts Rust IPC and normal process exit.
    & "$PSScriptRoot/smoke-windows.ps1" -Executable $exe
    if (!$?) { throw 'Installed application IPC/window smoke failed' }
    Write-Host 'PASS: installed app launched, rendered Rust Idle and closed cleanly'

    $uninstallers = @(Get-ChildItem -LiteralPath $dir -File |
        Where-Object { $_.Name -match '^uninstall.*\.exe$' })
    if ($uninstallers.Count -ne 1) {
        throw "Expected exactly one uninstaller, found $($uninstallers.Count)"
    }
    Write-Host "Uninstalling from $dir"
    Run-Nsis $uninstallers[0].FullName @('/S') 'NSIS uninstallation'
    $limit = [DateTime]::UtcNow.AddSeconds(20)
    while ((Test-Path -LiteralPath $exe) -and [DateTime]::UtcNow -lt $limit) {
        Start-Sleep -Milliseconds 250
    }
    if (Test-Path -LiteralPath $exe) { throw 'Uninstaller left application EXE behind' }
    if (Test-Path -LiteralPath $dll) { throw 'Uninstaller left app-private libmpv DLL behind' }
    $uninstalled = $true
    Write-Host 'PASS: NSIS silent install, app IPC, DLL SHA-256 and uninstall'
} finally {
    if ($installed -and !$uninstalled) {
        Write-Warning "A failed installer smoke may have left a test installation at $dir"
    }
}
