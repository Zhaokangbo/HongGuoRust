param(
  [string]$FfmpegPath = 'ffmpeg.exe',
  [string]$OutputPath = (Join-Path $env:USERPROFILE 'Videos\HongGuoRust-M11C1-tracks.mp4')
)

$ErrorActionPreference = 'Stop'
if (-not (Get-Command $FfmpegPath -ErrorAction SilentlyContinue)) {
  throw "Cannot locate ffmpeg.exe. Install FFmpeg, add it to PATH, or specify -FfmpegPath."
}

$target = [System.IO.Path]::GetFullPath($OutputPath)
$parent = Split-Path -Parent $target
New-Item -ItemType Directory -Force -Path $parent | Out-Null
$srtFile = Join-Path ([System.IO.Path]::GetTempPath()) ("hongguorust-m11c1-" + [guid]::NewGuid().ToString('N') + ".srt")

try {
  $srtText = @"
1
00:00:01,000 --> 00:00:06,000
HongGuoRust M11-C1 embedded subtitle test

2
00:00:08,000 --> 00:00:14,000
Try changing audio and subtitle tracks

3
00:00:16,000 --> 00:00:23,000
Track selection should not restart the video
"@
  [System.IO.File]::WriteAllText($srtFile, $srtText, [System.Text.UTF8Encoding]::new($false))

  & $FfmpegPath -hide_banner -loglevel error -y `
    -f lavfi -i 'testsrc2=size=640x360:rate=24:duration=25' `
    -f lavfi -i 'sine=frequency=440:sample_rate=48000:duration=25' `
    -f lavfi -i 'sine=frequency=660:sample_rate=48000:duration=25' `
    -i $srtFile `
    -map '0:v:0' -map '1:a:0' -map '2:a:0' -map '3:s:0' `
    -c:v mpeg4 -q:v 6 -c:a aac -b:a 96k -c:s mov_text `
    -metadata:s:a:0 language=zho -metadata:s:a:0 title='Test Audio 440Hz' `
    -metadata:s:a:1 language=eng -metadata:s:a:1 title='Test Audio 660Hz' `
    -metadata:s:s:0 language=eng -metadata:s:s:0 title='Embedded Test Subtitle' `
    -movflags +faststart -t 25 $target
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $target)) {
    throw "FFmpeg failed to create the multi-track MP4 (exit code: $LASTEXITCODE)."
  }
  Write-Host "Created multi-track test media: $target"
  Write-Host "Two distinguishable audio tracks: 440 Hz / 660 Hz; one embedded subtitle."
} finally {
  Remove-Item -LiteralPath $srtFile -Force -ErrorAction SilentlyContinue
}
