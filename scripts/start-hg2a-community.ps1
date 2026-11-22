param(
  [string]$ExePath = '',
  [string]$RepoRoot = '',
  [string]$AllowedMediaHosts = ''
)
$ErrorActionPreference = 'Stop'
# HG-2A explicit local opt-in. Installs NOTHING, reads no stored credentials,
# touches no HKCU policy, and does not change network/proxy configuration.
if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
  $RepoRoot = Split-Path -Parent $PSScriptRoot
}
if ([string]::IsNullOrWhiteSpace($ExePath)) {
  $ExePath = Join-Path $env:LOCALAPPDATA 'HongGuoRust-HG2A-Test\hongguorust.exe'
}
$server = Join-Path $RepoRoot 'scripts\hg2a-sdk\server.mjs'
$dependency = Join-Path $RepoRoot 'scripts\hg2a-sdk\node_modules\openhongguo\package.json'
foreach ($path in @($ExePath, $server, $dependency)) {
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw "Required path not found: $path (build HG-2A and install optional SDK first)"
  }
}
if ($AllowedMediaHosts -and
    $AllowedMediaHosts -notmatch '^[a-zA-Z0-9.-]+(?:,[a-zA-Z0-9.-]+)*$') {
  throw 'AllowedMediaHosts must be a comma-separated explicit hostname allowlist'
}
$node = (Get-Command node.exe -ErrorAction Stop).Source
$tokenBytes = New-Object byte[] 32
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
try { $rng.GetBytes($tokenBytes) } finally { $rng.Dispose() }
$token = -join ($tokenBytes | ForEach-Object { $_.ToString('x2') })
$env:HONGGUORUST_HG2A_EXPERIMENT = '1'
$env:HONGGUORUST_HG2A_TOKEN = $token
$env:HONGGUORUST_HG2A_MEDIA_HOSTS = $AllowedMediaHosts
$serverProcess = $null
$appProcess = $null
try {
  $serverProcess = Start-Process -FilePath $node -ArgumentList ('"' + $server + '"') -PassThru -NoNewWindow
  Start-Sleep -Milliseconds 700
  if ($serverProcess.HasExited) {
    throw 'Community sidecar exited during startup; check SDK installation and port 19091'
  }
  Write-Host 'HG-2A: launching experimental nonofficial SDK source, loopback only.'
  Write-Host 'This is not an official HongGuo client; no media decryption is implemented.'
  $appProcess = Start-Process -FilePath $ExePath -PassThru
  $appProcess.WaitForExit()
} finally {
  if ($serverProcess -and -not $serverProcess.HasExited) {
    Stop-Process -Id $serverProcess.Id -Force -ErrorAction SilentlyContinue
  }
  # Clear all opt-in settings from this PowerShell process after launch.
  Remove-Item Env:\HONGGUORUST_HG2A_EXPERIMENT -ErrorAction SilentlyContinue
  Remove-Item Env:\HONGGUORUST_HG2A_TOKEN -ErrorAction SilentlyContinue
  Remove-Item Env:\HONGGUORUST_HG2A_MEDIA_HOSTS -ErrorAction SilentlyContinue
}
