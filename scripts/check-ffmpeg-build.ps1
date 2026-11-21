# Inspect the companion binary from the same x64 LGPL build as libmpv-2.dll.
$ErrorActionPreference = 'Stop'
$tmp = New-Item -ItemType Directory -Path (Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString()))
try {
  $archive = Join-Path $tmp 'ffmpeg.7z'
  Invoke-WebRequest 'https://github.com/zhongfly/mpv-winbuild/releases/download/2026-11-20-94127e1e1a/ffmpeg-lgpl-x86_64-git-7ddf7bc8a.7z' -OutFile $archive
  if ((Get-FileHash $archive).Hash -ne 'ba0fc7892c4840a6d1f157a59c92581554dd332cdbb369ca4ce2a8fa6de86a4a') { throw 'FFmpeg archive SHA-256 mismatch' }
  & 7z x $archive "-o$tmp" -y | Out-Null
  if ($LASTEXITCODE) { throw 'FFmpeg archive extraction failed' }
  $exe = @(Get-ChildItem $tmp -Filter ffmpeg.exe -Recurse -File)
  if ($exe.Count -ne 1) { throw 'Expected one companion FFmpeg executable' }
  $config = (& $exe[0].FullName -version 2>&1) -join [Environment]::NewLine
  Write-Output $config
  if ($LASTEXITCODE) { throw "Companion FFmpeg -version exited $LASTEXITCODE" }
  if ($config -notmatch '7ddf7bc8a' -or $config -notmatch '--enable-version3' -or
      $config -match '--enable-(gpl|nonfree)') { throw 'Unexpected FFmpeg build configuration' }
  $license = (& $exe[0].FullName -hide_banner -L 2>&1) -join [Environment]::NewLine
  if ($LASTEXITCODE) { throw 'FFmpeg license check failed' }
  Write-Output $license
  if ($license -notmatch 'Lesser General Public License') { throw 'Unexpected FFmpeg license' }
} finally {
  Remove-Item -LiteralPath $tmp.FullName -Recurse -Force
}
