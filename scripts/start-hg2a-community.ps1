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
# Fail before starting a child process if the fixed loopback port is already
# occupied. Never stop or reuse an unknown process: its session token could
# belong to another application or an earlier HG-2A run.
$listener = Get-NetTCPConnection -LocalPort 19091 -State Listen -ErrorAction SilentlyContinue |
  Select-Object -First 1
if ($listener) {
  $listenerPid = [int]$listener.OwningProcess
  $listenerProcess = Get-Process -Id $listenerPid -ErrorAction SilentlyContinue
  $listenerName = if ($listenerProcess) { $listenerProcess.ProcessName } else { 'unknown' }
  throw ("HG-2A port 127.0.0.1:19091 is already in use by PID {0} ({1}). " +
    "Inspect the process before closing it; this launcher will not terminate or reuse it." -f
    $listenerPid, $listenerName)
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
    throw 'Community sidecar exited during startup; check the Node error above, SDK installation, or another process using 127.0.0.1:19091'
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
