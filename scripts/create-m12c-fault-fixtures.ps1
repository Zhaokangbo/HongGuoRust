param(
  [string]$FfmpegPath = 'D:\26-008\26-008\voip4\run\run_windows\bin\ffmpeg.exe',
  [string]$OutputDirectory = ''
)

# M12-C: Generate ONLY new synthetic media in an isolated, never-before-used
# directory. Do not alter original media, existing app data or network policy.
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $FfmpegPath -PathType Leaf)) {
  throw "Cannot find FFmpeg at $FfmpegPath. Pass -FfmpegPath explicitly."
}
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
  $OutputDirectory = Join-Path ([System.IO.Path]::GetTempPath()) (
    'HongGuoRust-M12C-Faults-' + [guid]::NewGuid().ToString('N')
  )
}
$target = [System.IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $target) {
  throw "Output directory already exists. Choose a NEW empty directory (no overwrite): $target"
}

New-Item -ItemType Directory -Path $target -ErrorAction Stop | Out-Null
$healthy = Join-Path $target 'healthy.mp4'
Write-Host "Building isolated synthetic fixtures at $target"

$ffmpegArgs = @(
  '-hide_banner', '-nostdin', '-loglevel', 'error', '-n',
  '-f', 'lavfi', '-i', 'testsrc2=size=320x180:rate=15:duration=20',
  '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000:duration=20',
  '-map', '0:v:0', '-map', '1:a:0',
  '-c:v', 'mpeg4', '-q:v', '8', '-c:a', 'aac', '-b:a', '64k',
  '-movflags', '+faststart', '-t', '20', $healthy
)
& $FfmpegPath @ffmpegArgs
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $healthy -PathType Leaf)) {
  throw "FFmpeg fixture generation failed (exit code: $LASTEXITCODE). Inspect new folder before retrying."
}
$bytes = [System.IO.File]::ReadAllBytes($healthy)
if ($bytes.Length -lt 8192 -or $bytes.Length -gt 32MB) {
  throw "FFmpeg fixture size out of expected bounded range; stop without creating malformed derivatives."
}

[System.IO.File]::WriteAllBytes((Join-Path $target 'empty.mp4'), [byte[]]::new(0))
[System.IO.File]::WriteAllBytes(
  (Join-Path $target 'random-bytes.mp4'),
  [byte[]](0..255 | ForEach-Object { [byte](($_ * 73 + 19) % 256) })
)
[System.IO.File]::WriteAllText(
  (Join-Path $target 'html-disguised.mp4'),
  '<!doctype html><html><body>not an MP4</body></html>',
  [System.Text.UTF8Encoding]::new($false)
)
$cutHead = [math]::Min(1024, [math]::Floor($bytes.Length / 8))
$head = New-Object byte[] ([int]$cutHead)
[Array]::Copy($bytes, $head, $head.Length)
[System.IO.File]::WriteAllBytes((Join-Path $target 'truncated-header.mp4'), $head)

$cutTail = [int][math]::Floor($bytes.Length * 0.75)
$tail = New-Object byte[] $cutTail
[Array]::Copy($bytes, $tail, $tail.Length)
[System.IO.File]::WriteAllBytes((Join-Path $target 'truncated-tail.mp4'), $tail)

[System.IO.File]::WriteAllText(
  (Join-Path $target 'wrong-extension.txt'),
  'This is a harmless synthetic text file, not MP4.',
  [System.Text.UTF8Encoding]::new($false)
)

$scenario = @(
  [pscustomobject]@{ id='healthy'; fileName='healthy.mp4'; expectation='normal playback and EOF' },
  [pscustomobject]@{ id='empty'; fileName='empty.mp4'; expectation='reject or error, never natural EOF' },
  [pscustomobject]@{ id='random'; fileName='random-bytes.mp4'; expectation='reject or error, never natural EOF' },
  [pscustomobject]@{ id='html'; fileName='html-disguised.mp4'; expectation='reject or error, never natural EOF' },
  [pscustomobject]@{ id='header'; fileName='truncated-header.mp4'; expectation='reject or error, never natural EOF' },
  [pscustomobject]@{ id='tail'; fileName='truncated-tail.mp4'; expectation='may play partially or fail; never crash/hang' },
  [pscustomobject]@{ id='extension'; fileName='wrong-extension.txt'; expectation='file picker or Rust type validation rejects' }
)
$manifest = [pscustomobject]@{ schemaVersion=1; content='locally-generated-synthetic'; scenarios=$scenario }
$manifestPath = Join-Path $target 'scenario-manifest.json'
$manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath -Encoding UTF8

Write-Host ('Created ' + $scenario.Count + ' synthetic scenario fixtures in a fresh directory.')
Write-Host 'Do not enable automatic local-list auto-next when evaluating invalid MP4: error must not advance the queue.'
Write-Host 'No existing user media or persistent app data was modified.'
Write-Host ('Manifest: ' + $manifestPath)
