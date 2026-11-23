param(
  [ValidateRange(2, 3600)][int]$Samples = 30,
  [ValidateRange(1, 60)][int]$IntervalSeconds = 2,
  [string]$ProcessName = 'hongguorust',
  [string]$CsvPath = ''
)

$ErrorActionPreference = 'Stop'
# M12-A: metadata-only read-only process sampling. Does not open or scan media.
# Does not write a report unless the user explicitly passes -CsvPath.
if ($ProcessName -notmatch '^[a-zA-Z0-9._-]{1,64}$') {
  throw "Invalid ProcessName"
}

$process = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue |
  Sort-Object -Property Id |
  Select-Object -First 1
if ($null -eq $process) {
  throw "Cannot find process '$ProcessName'. Start HongGuoRust before sampling."
}

$pidToSample = $process.Id
Write-Host "M12-A process metrics: PID=$pidToSample, samples=$Samples, interval=$IntervalSeconds sec"
Write-Host 'The script does not collect media URLs, titles, file paths or playback data.'
$data = New-Object System.Collections.Generic.List[object]
for ($i = 0; $i -lt $Samples; $i++) {
  $current = Get-Process -Id $pidToSample -ErrorAction SilentlyContinue
  if ($null -eq $current) {
    Write-Warning "Process exited after $i samples"
    break
  }
  $data.Add([pscustomobject]@{
    Sample = $i + 1
    Timestamp = (Get-Date).ToString('yyyy-MM-ddTHH:mm:ssK')
    PID = $pidToSample
    TotalCPUSeconds = [math]::Round($current.CPU, 3)
    WorkingSetMiB = [math]::Round($current.WorkingSet64 / 1MB, 2)
    PrivateBytesMiB = [math]::Round($current.PrivateMemorySize64 / 1MB, 2)
    Handles = $current.HandleCount
    Threads = $current.Threads.Count
  })
  if ($i -lt $Samples - 1) {
    Start-Sleep -Seconds $IntervalSeconds
  }
}

if ($data.Count -gt 0) {
  $data | Format-Table -AutoSize
  if ($CsvPath.Trim()) {
    $full = [System.IO.Path]::GetFullPath($CsvPath)
    $parent = [System.IO.Path]::GetDirectoryName($full)
    if (-not (Test-Path -LiteralPath $parent)) {
      throw "CSV directory does not exist: $parent"
    }
    $data | Export-Csv -LiteralPath $full -NoTypeInformation -Encoding UTF8
    Write-Host "Saved opt-in metrics to: $full"
  }
}
