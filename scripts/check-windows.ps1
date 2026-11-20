$ErrorActionPreference = 'Stop'
$script:missing = 0

function Check([string]$name, [bool]$ok, [string]$detail) {
    if ($ok) { Write-Output "[OK] $name - $detail" }
    else {
        Write-Output "[MISSING] $name - $detail"
        $script:missing++
    }
}

Check 'Windows x64' ([Environment]::Is64BitOperatingSystem -and $env:OS -eq 'Windows_NT') ([Environment]::OSVersion.VersionString)
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vs = if (Test-Path -LiteralPath $vswhere) {
    & $vswhere -latest -version '[17.0,18.0)' -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
Check 'VS 2022 C++ x64 tools' ([bool]$vs) "$vs"

$kits = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue).KitsRoot10
$sdk = if ($kits) {
    Get-ChildItem (Join-Path $kits 'Include') -Directory -ErrorAction SilentlyContinue |
        Where-Object {
            (Test-Path (Join-Path $_.FullName 'um\Windows.h')) -and
            (Test-Path (Join-Path $kits "Lib\$($_.Name)\um\x64\kernel32.lib")) -and
            (Test-Path (Join-Path $kits "bin\$($_.Name)\x64\rc.exe"))
        } | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
}
Check 'Windows SDK (header, x64 library, resource compiler)' ([bool]$sdk) "$sdk"

$node = Get-Command node -ErrorAction SilentlyContinue
$lts = if ($node) { & node -p "process.release.lts || ''" }
$nodeVersion = if ($node) { & node -p 'process.versions.node' }
$validNode = $lts -and $nodeVersion -and (([version]$nodeVersion).Major -eq 24 -or
    (([version]$nodeVersion).Major -eq 22 -and [version]$nodeVersion -ge [version]'22.12.0'))
Check 'Node.js project LTS' ([bool]$validNode) "$nodeVersion $lts"
$npm = Get-Command npm.cmd -ErrorAction SilentlyContinue
Check 'npm' ([bool]$npm) "$($npm.Source)"

$webview = Get-ChildItem 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients','HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients','HKCU:\Software\Microsoft\EdgeUpdate\Clients' -ErrorAction SilentlyContinue |
    Get-ItemProperty | Where-Object { $_.name -match 'WebView2' -and $_.pv -and $_.pv -ne '0.0.0.0' } |
    Select-Object -First 1
Check 'WebView2 Runtime' ([bool]$webview) "$($webview.pv)"

$rustup = Get-Command rustup -ErrorAction SilentlyContinue
$active = if ($rustup) { & rustup show active-toolchain }
Check 'rustup / stable MSVC x64' ([bool]($active -match '^stable-x86_64-pc-windows-msvc')) "$active"
$rustc = Get-Command rustc -ErrorAction SilentlyContinue
$hostLine = if ($rustc) { & rustc -vV | Where-Object { $_ -eq 'host: x86_64-pc-windows-msvc' } }
Check 'Rust compiler host' ([bool]$hostLine) "$hostLine"
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
Check 'Cargo' ([bool]$cargo) "$($cargo.Source)"
$cli = if ($cargo) { & cargo tauri --version 2>$null }
Check 'Tauri CLI 2' ([bool]($cli -match 'tauri-cli 2\.')) "$cli"
if ($script:missing) { exit 1 }
