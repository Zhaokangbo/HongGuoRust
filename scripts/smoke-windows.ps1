param(
    [string]$Executable = (Join-Path $PSScriptRoot '..\src-tauri\target\release\hongguorust.exe')
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$exe = (Resolve-Path -LiteralPath $Executable).Path
$manifest = Get-Content (Join-Path $PSScriptRoot '..\src-tauri\Cargo.toml') -Raw
$version = [regex]::Match($manifest, '(?m)^version = "([^"]+)"').Groups[1].Value
if (!$version) { throw 'Missing Cargo package version' }
$connected = -join ([char[]](0x5DF2, 0x8FDE, 0x63A5))
$names = @('HongGuoRust', $connected, 'Idle', $version)
$process = Start-Process -FilePath $exe -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    $missing = $names
    do {
        $process.Refresh()
        if ($process.HasExited) { throw "Application exited before IPC verification: $($process.ExitCode)" }
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
            $root = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
            $missing = @(foreach ($name in $names) {
                $condition = New-Object System.Windows.Automation.PropertyCondition(
                    [System.Windows.Automation.AutomationElement]::NameProperty, $name)
                if (!$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)) { $name }
            })
        }
        if ($missing.Count) { Start-Sleep -Milliseconds 300 }
    } while ($missing.Count -and [DateTime]::UtcNow -lt $deadline)
    if ($missing.Count) { throw "Window is missing expected accessible text: $($missing -join ', ')" }
    Write-Output "PASS: desktop window shows connected, Idle and version $version"

    # Capture this window's process tree before closing it. Other WebView2 clients
    # on the desktop may legitimately outlive our application.
    $processes = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId)
    $parents = @($process.Id)
    $children = @(while ($parents.Count) {
        $parents = @($processes | Where-Object { $_.ParentProcessId -in $parents } |
            Select-Object -ExpandProperty ProcessId)
        foreach ($childId in $parents) {
            $child = Get-Process -Id $childId -ErrorAction SilentlyContinue
            if ($child -and !$child.HasExited) { $child }
        }
    })
    if (!$process.CloseMainWindow()) { throw 'Window rejected the close request' }
    if (!$process.WaitForExit(10000)) { throw 'Process remained alive after normal window close' }
    if ($process.ExitCode -ne 0) { throw "Abnormal exit: $($process.ExitCode)" }
    Write-Output 'PASS: normal last-window close, process exit 0'
    $exitDeadline = [DateTime]::UtcNow.AddSeconds(15)
    foreach ($child in $children) {
        $remaining = [Math]::Max(0, [int]($exitDeadline - [DateTime]::UtcNow).TotalMilliseconds)
        if (!$child.HasExited -and !$child.WaitForExit($remaining)) {
            throw "Child process remained after close: $($child.ProcessName) (PID $($child.Id))"
        }
    }
    Write-Output "PASS: no surviving processes among $($children.Count) observed descendants"
} finally {
    if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
    $process.Dispose()
}
