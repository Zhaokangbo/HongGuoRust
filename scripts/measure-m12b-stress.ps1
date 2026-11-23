param(
  [ValidateRange(1, 120)][int]$DurationMinutes = 30,
  [ValidateRange(1, 30)][int]$IntervalSeconds = 5,
  [ValidateRange(0, 2147483647)][int]$TargetPid = 0,
  [string]$ProcessName = 'hongguorust',
  [ValidatePattern('^[a-zA-Z0-9_-]{1,32}$')][string]$Stage = 'playback',
  [string]$CsvPath = ''
)

$ErrorActionPreference = 'Stop'
# M12-B: passive, bounded sampling only. This script does not open any media,
# modify playback, disconnect the network, or touch the application's data.
# A CSV is written only when -CsvPath is explicitly provided.
if ($ProcessName -notmatch '^[a-zA-Z0-9._-]{1,64}$') {
  throw "Invalid ProcessName"
}

if ($TargetPid -gt 0) {
  $process = Get-Process -Id $TargetPid -ErrorAction SilentlyContinue
  if ($null -eq $process) { throw "PID $TargetPid is not running" }
  if ($process.ProcessName -ine $ProcessName) {
    throw "PID $TargetPid does not match ProcessName '$ProcessName'"
  }
} else {
  $candidates = @(Get-Process -Name $ProcessName -ErrorAction SilentlyContinue)
  if ($candidates.Count -ne 1) {
    throw "Expected exactly one '$ProcessName' process, found $($candidates.Count). Use -TargetPid to select one."
  }
  $process = $candidates[0]
}
$targetId = [int]$process.Id
$originalStart = $process.StartTime.ToUniversalTime().Ticks
$logicalCores = [math]::Max(1, [Environment]::ProcessorCount)
$rows = New-Object 'System.Collections.Generic.List[object]'
$timer = [System.Diagnostics.Stopwatch]::StartNew()
$previous = $null
$exitedEarly = $false

Write-Host "M12-B passive resource sampling: PID=$targetId Stage=$Stage Duration=$DurationMinutes minute(s) Every=$IntervalSeconds second(s)"
Write-Host "No media paths, URLs, user data or command lines are read. The process is never controlled."
$endSeconds = $DurationMinutes * 60
while ($timer.Elapsed.TotalSeconds -lt $endSeconds) {
  $current = Get-Process -Id $targetId -ErrorAction SilentlyContinue
  if ($null -eq $current) {
    $exitedEarly = $true
    Write-Warning "Target process exited during sampling (PID=$targetId)"
    break
  }
  # Process ID can be reused; never mix a replacement process into the same CSV.
  if ($current.StartTime.ToUniversalTime().Ticks -ne $originalStart) {
    $exitedEarly = $true
    Write-Warning "Target PID reused; stopping before mixing different processes"
    break
  }
  $elapsed = $timer.Elapsed.TotalSeconds
  $totalCpu = [double]$current.CPU
  $cpuPercent = 0.0
  if ($null -ne $previous) {
    $deltaSeconds = $elapsed - $previous.ElapsedSec
    if ($deltaSeconds -gt 0) {
      $deltaCpu = [math]::Max(0.0, $totalCpu - $previous.CpuSec)
      $cpuPercent = [math]::Min(100.0, (100.0 * $deltaCpu / $deltaSeconds / $logicalCores))
    }
  }
  $rows.Add([pscustomobject]@{
    TimestampUtc = [DateTime]::UtcNow.ToString('o')
    ElapsedSec = [math]::Round($elapsed, 3)
    Pid = $targetId
    Stage = $Stage
    CpuSeconds = [math]::Round($totalCpu, 3)
    CpuPercentMachine = [math]::Round($cpuPercent, 2)
    WorkingSetMiB = [math]::Round($current.WorkingSet64 / 1MB, 2)
    PrivateMiB = [math]::Round($current.PrivateMemorySize64 / 1MB, 2)
    Handles = [int]$current.HandleCount
    Threads = [int]$current.Threads.Count
  })
  $previous = [pscustomobject]@{ ElapsedSec = $elapsed; CpuSec = $totalCpu }

  $remaining = $endSeconds - $timer.Elapsed.TotalSeconds
  if ($remaining -gt 0) {
    Start-Sleep -Milliseconds ([int][math]::Min($IntervalSeconds * 1000, $remaining * 1000))
  }
}
$timer.Stop()
if ($rows.Count -eq 0) { throw 'No valid measurements collected' }

# The default mode has no disk writes. The specified output directory must
# exist; never overwrite a report or silently replace an existing file.
if (-not [string]::IsNullOrWhiteSpace($CsvPath)) {
  $fullPath = [System.IO.Path]::GetFullPath($CsvPath)
  $folder = [System.IO.Path]::GetDirectoryName($fullPath)
  if (-not (Test-Path -LiteralPath $folder -PathType Container)) {
    throw "CSV destination directory does not exist: $folder"
  }
  if (Test-Path -LiteralPath $fullPath) {
    throw "CSV already exists; choose a new file name: $fullPath"
  }
  $pending = $fullPath + '.pending'
  if (Test-Path -LiteralPath $pending) {
    throw "Pending report exists; inspect it before retrying: $pending"
  }
  try {
    $rows | Export-Csv -LiteralPath $pending -NoTypeInformation -Encoding UTF8
    Move-Item -LiteralPath $pending -Destination $fullPath -ErrorAction Stop
  } finally {
    # Do not touch a pre-existing report or application data.
    if (Test-Path -LiteralPath $pending) {
      Remove-Item -LiteralPath $pending -Force -ErrorAction SilentlyContinue
    }
  }
  Write-Host "Saved opt-in CSV metrics to: $fullPath"
}
$first = $rows[0]
$last = $rows[$rows.Count - 1]
Write-Host "Samples=$($rows.Count) ElapsedSec=$([math]::Round($timer.Elapsed.TotalSeconds, 1))"
Write-Host "PrivateMiB start=$($first.PrivateMiB) end=$($last.PrivateMiB); Handles start=$($first.Handles) end=$($last.Handles); Threads start=$($first.Threads) end=$($last.Threads)"
Write-Host 'These raw start/end values are NOT a memory leak verdict. Use the M12-B trend analyzer and repeat the run.'
if ($exitedEarly) {
  Write-Warning 'Process exited before the requested duration; the run is incomplete.'
  exit 2
}
