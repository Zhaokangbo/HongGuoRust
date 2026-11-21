[CmdletBinding()]
param([switch]$SkipTests)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'Windows x64 is required' }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Set-Location -LiteralPath $root
$temp = Join-Path ([IO.Path]::GetTempPath()) ('hongguorust-m5-' + [guid]::NewGuid().ToString('N'))
$dll = Join-Path $root 'src-tauri/resources/libmpv-2.dll'
$expected = 'df006bfc754d1212fb2084c6a6da04876882648c025ccb56cf1132b27b1e7913'
function Check([string]$stage) {
    if ($LASTEXITCODE -ne 0) { throw "$stage failed (exit $LASTEXITCODE)" }
}
if (Test-Path -LiteralPath $dll) { throw "Refusing to overwrite existing staging DLL: $dll" }
try {
    & "$PSScriptRoot/check-windows.ps1"
    & npm ci; Check 'npm ci'
    & npm run build; Check 'frontend build'
    & "$PSScriptRoot/prepare-mpv-msvc.ps1" -Destination $temp
    $source = Join-Path $temp 'libmpv-2.dll'
    $sha = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($sha -ne $expected) { throw "libmpv checksum mismatch: $sha" }
    New-Item -ItemType Directory -Force -Path (Split-Path $dll) | Out-Null
    Copy-Item -LiteralPath $source -Destination $dll
    $env:LIB = "$temp;$env:LIB"
    $env:PATH = "$temp;$env:PATH"
    if (-not $SkipTests) {
        & cargo fmt --manifest-path src-tauri/Cargo.toml -- --check; Check 'cargo fmt'
        & cargo test --manifest-path src-tauri/Cargo.toml --locked; Check 'cargo test'
    }
    & cargo tauri build --config 'src-tauri/tauri.bundle.conf.json' --bundles nsis -- --locked
    Check 'NSIS bundle'
    $out = Join-Path $root 'src-tauri/target/release/bundle/nsis'
    $setups = @(Get-ChildItem -LiteralPath $out -Filter '*-setup.exe' -File)
    if ($setups.Count -eq 0) { throw "NSIS setup not found in $out" }
    foreach ($file in $setups) {
        Write-Host "Installer candidate: $($file.FullName)"
        Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256
    }
    Write-Warning 'TEST BUILD ONLY: no public release before third-party license compliance and clean-machine validation.'
} finally {
    Remove-Item -LiteralPath $dll -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
}
