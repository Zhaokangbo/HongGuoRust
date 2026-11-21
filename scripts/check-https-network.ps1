[CmdletBinding()]
param(
  [string]$Url = 'https://samplelib.com/lib/preview/mp4/sample-20s.mp4'
)

$ErrorActionPreference = 'Stop'
if (-not (Get-Command curl.exe -ErrorAction SilentlyContinue)) {
  throw 'curl.exe is required for network diagnostics'
}
$target = [Uri]$Url
if ($target.Scheme -ne 'https' -or -not $target.Host) {
  throw 'Use an HTTPS URL for a public test media resource'
}

Write-Host ('HTTPS connectivity check: ' + $target.Host + ' (URL path and query omitted)')
Write-Host 'Proxy settings for this PowerShell process (secrets omitted):'
foreach ($name in @('HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY')) {
  $value = [Environment]::GetEnvironmentVariable($name, 'Process')
  if (-not $value) {
    Write-Host ('  ' + $name + ': unset')
    continue
  }
  if ($name -eq 'NO_PROXY') {
    Write-Host ('  ' + $name + ': configured (value hidden)')
    continue
  }
  $parsed = $null
  if ([Uri]::TryCreate($value, [UriKind]::Absolute, [ref]$parsed)) {
    Write-Host ('  ' + $name + ': ' + $parsed.Scheme + '://' + $parsed.Host + ':' + $parsed.Port)
  } else {
    Write-Host ('  ' + $name + ': configured (unparseable; value hidden)')
  }
}

$curlArgs = @(
  '-L', '-sS', '--max-time', '25', '--range', '0-1023',
  '-o', 'NUL', '-w', 'HTTP=%{http_code} TYPE=%{content_type} BYTES=%{size_download}\n'
)

Write-Host ''
Write-Host 'Test 1: curl with current process proxy environment'
& curl.exe @curlArgs $Url
$proxyExit = $LASTEXITCODE
Write-Host ('curl exit code: ' + $proxyExit)

Write-Host ''
Write-Host 'Test 2: curl with explicit HTTP proxy bypass'
& curl.exe --noproxy '*' @curlArgs $Url
$directExit = $LASTEXITCODE
Write-Host ('curl exit code: ' + $directExit)

Write-Host ''
Write-Host 'Only an HTTP 200/206 response with video/mp4 confirms this media URL is reachable.'
Write-Host 'A failed proxy test with a successful direct test suggests a stale or unreachable HTTP proxy.'
Write-Host 'This is a curl check, not proof that libmpv used the same proxy configuration.'
Write-Host 'The script has not changed Windows system proxy settings or environment variables.'
