[CmdletBinding()]
param([Parameter(Mandatory)][string]$Destination)
$ErrorActionPreference = 'Stop'
$source = 'https://github.com/zhongfly/mpv-winbuild/releases/download/2026-11-20-94127e1e1a/mpv-dev-lgpl-x86_64-20261120-git-94127e1e1a.7z'
$expected = '6edded03578c35ec1a89c1c50ad92bd52a79521dec6180f49fd697f5e18f367b'
$tempDir = New-Item -ItemType Directory -Path (Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString()))
try {
  $archive = Join-Path $tempDir 'dev.7z'
  Invoke-WebRequest $source -OutFile $archive
  if ((Get-FileHash $archive).Hash -ne $expected) { throw 'libmpv archive SHA-256 mismatch' }
  & 7z x $archive "-o$tempDir" -y | Out-Null
  if ($LASTEXITCODE) { throw 'libmpv extraction failed' }
  $dll = @(Get-ChildItem $tempDir -Filter libmpv-2.dll -File -Recurse)
  if ($dll.Count -ne 1) { throw 'Expected exactly one libmpv-2.dll' }
  if ((Get-FileHash $dll[0].FullName).Hash -ne 'df006bfc754d1212fb2084c6a6da04876882648c025ccb56cf1132b27b1e7913') { throw 'Unexpected libmpv DLL' }
  New-Item -ItemType Directory -Force -Path $Destination | Out-Null
  Copy-Item -LiteralPath $dll[0].FullName -Destination $Destination
  Write-Output "Archive SHA256 $expected"
  Get-FileHash -LiteralPath $dll[0].FullName | Format-List
  Get-ChildItem $tempDir -Recurse -File | ForEach-Object { $_.FullName.Substring($tempDir.FullName.Length + 1) }
} finally {
  Remove-Item -LiteralPath $tempDir.FullName -Recurse -Force
}
