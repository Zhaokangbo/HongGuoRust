param(
    [string]$Executable = (Join-Path $PSScriptRoot '..\src-tauri\target\release\hongguorust.exe')
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (!('HongGuoNativeClick' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class HongGuoNativeClick {
    [DllImport("user32.dll", SetLastError=true)]
    public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll", SetLastError=true)]
    public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
}
'@
}
$exe = (Resolve-Path -LiteralPath $Executable).Path
$manifest = Get-Content (Join-Path $PSScriptRoot '..\src-tauri\Cargo.toml') -Raw
$version = [regex]::Match($manifest, '(?m)^version = "([^"]+)"').Groups[1].Value
if (!$version) { throw 'Missing Cargo package version' }
$connected = -join ([char[]](0x5DF2, 0x8FDE, 0x63A5))
$home = -join ([char[]](0x9996, 0x9875))
$names = @('HongGuoRust', $connected, $home, '播放器', $version)
$diag = (New-Item -ItemType Directory -Force (Join-Path $PSScriptRoot '../diagnostics')).FullName
$env:HONGGUORUST_LOG_DIR = $diag
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS --force-renderer-accessibility".Trim()
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $env:TEMP ('hgr-webview-' + [guid]::NewGuid())
$timer = [Diagnostics.Stopwatch]::StartNew()
$root = $null
$focused = $false
$process = Start-Process -FilePath $exe -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(90)
    $missing = $names
    do {
        $process.Refresh()
        if ($process.HasExited) { throw "Application exited before IPC verification: $($process.ExitCode)" }
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
            if (!$focused) {
                $focused = (New-Object -ComObject WScript.Shell).AppActivate($process.Id)
            }
            try {
            $root = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
            $missing = @(foreach ($name in $names) {
                $condition = New-Object System.Windows.Automation.PropertyCondition(
                    [System.Windows.Automation.AutomationElement]::NameProperty, $name)
                if (!$root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)) { $name }
            })
            } catch [System.Windows.Automation.ElementNotAvailableException] { $missing = $names }
        }
        if ($missing.Count) { Start-Sleep -Milliseconds 300 }
    } while ($missing.Count -and [DateTime]::UtcNow -lt $deadline)
    if ($missing.Count) { throw "Window is missing expected accessible text: $($missing -join ', ')" }
    $playerCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty, '播放器')
    $playerButton = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $playerCondition)
    if (!$playerButton) { throw 'Missing player surface entry button' }
    if (!$playerButton.Current.IsEnabled) { throw 'Player surface entry button is disabled' }
    $bounds = $playerButton.Current.BoundingRectangle
    if ($bounds.Width -le 0 -or $bounds.Height -le 0) { throw 'Player surface entry button has no visible bounds' }
    if (![HongGuoNativeClick]::SetCursorPos(
        [int]($bounds.Left + $bounds.Width / 2),
        [int]($bounds.Top + $bounds.Height / 2))) {
        throw 'Could not focus the player surface entry button'
    }
    [HongGuoNativeClick]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [HongGuoNativeClick]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)

    $idleCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty, 'Idle')
    $idleDeadline = [DateTime]::UtcNow.AddSeconds(30)
    $idle = $null
    do {
        $process.Refresh()
        if ($process.HasExited) { throw "Application exited while opening player surface: $($process.ExitCode)" }
        $root = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        $idle = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $idleCondition)
        if (!$idle) { Start-Sleep -Milliseconds 300 }
    } while (!$idle -and [DateTime]::UtcNow -lt $idleDeadline)
    if (!$idle) { throw 'Player surface did not expose the Rust Idle snapshot' }

    $log = Join-Path $diag "player-$($process.Id).log"
    if (!(Select-String -Path $log -Pattern 'first snapshot IPC.*state=Idle' -Quiet)) { throw 'Missing Rust Idle IPC evidence' }
    Write-Output "UIA opened the player surface from Home after $($timer.ElapsedMilliseconds) ms"
    Write-Output "PASS: desktop window exposes Home, connected, player Idle and version $version"

    # Capture this window's process tree before closing it. Other WebView2 clients
    # on the desktop may legitimately outlive our application.
    # Toolhelp works without WMI/CIM access on a restricted desktop.
    if (!('HongGuoProcessTree' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class HongGuoProcessTree {
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    struct Entry {
        public uint size, usage, pid;
        public UIntPtr heap;
        public uint module, threads, parent;
        public int priority;
        public uint flags;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=260)] public string name;
    }
    public struct Relation { public uint ProcessId, ParentProcessId; }
    [DllImport("kernel32",SetLastError=true)] static extern IntPtr CreateToolhelp32Snapshot(uint flags,uint pid);
    [DllImport("kernel32",SetLastError=true)] static extern bool Process32FirstW(IntPtr snapshot,ref Entry e);
    [DllImport("kernel32",SetLastError=true)] static extern bool Process32NextW(IntPtr snapshot,ref Entry e);
    [DllImport("kernel32")] static extern bool CloseHandle(IntPtr h);
    public static Relation[] Read() {
        IntPtr h = CreateToolhelp32Snapshot(2,0);
        if (h == new IntPtr(-1)) throw new Win32Exception();
        try {
            var e = new Entry { size = (uint)Marshal.SizeOf(typeof(Entry)) };
            var list = new List<Relation>();
            if (!Process32FirstW(h,ref e)) throw new Win32Exception();
            do { list.Add(new Relation { ProcessId=e.pid,ParentProcessId=e.parent }); } while (Process32NextW(h,ref e));
            if (Marshal.GetLastWin32Error() != 18) throw new Win32Exception();
            return list.ToArray();
        } finally { CloseHandle(h); }
    }
}
'@
    }
    $processes = [HongGuoProcessTree]::Read()
    $parents = @($process.Id)
    $children = @(while ($parents.Count) {
        $parents = @($processes | Where-Object { $_.ParentProcessId -in $parents } |
            Select-Object -ExpandProperty ProcessId)
        foreach ($childId in $parents) {
            $child = Get-Process -Id $childId -ErrorAction SilentlyContinue
            if ($child -and !$child.HasExited) {
                # Only track WebView2 subprocesses owned by the desktop app.
                # Build tools (e.g. MSBuild) share this CI desktop; PID reuse
                # or stale parent IDs must not count as a player leak.
                if ($child.ProcessName -eq 'msedgewebview2') { $child }
            }
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
    Write-Output "PASS: no surviving WebView2 subprocesses among $($children.Count) observed descendants"
} catch {
    $failure = $_
    try {
        $process.Refresh()
        "PID=$($process.Id) title=$($process.MainWindowTitle) hwnd=$($process.MainWindowHandle) elapsed_ms=$($timer.ElapsedMilliseconds) missing=$missing" | Out-File "$diag/window.txt" -Encoding utf8
        if ($root) {
            $rect = $root.Current.BoundingRectangle
            Add-Type -AssemblyName System.Drawing
            if ($rect.Width -gt 0 -and $rect.Height -gt 0) {
                $bitmap = New-Object Drawing.Bitmap ([int]$rect.Width),([int]$rect.Height)
                $graphics = [Drawing.Graphics]::FromImage($bitmap)
                try {
                    $graphics.CopyFromScreen([int]$rect.Left,[int]$rect.Top,0,0,$bitmap.Size)
                    $bitmap.Save("$diag/window.png")
                } finally { $graphics.Dispose(); $bitmap.Dispose() }
            }
        }
    } catch { Write-Warning "Window diagnostic: $_" }
    try {
        if ($root) {
            function Write-Tree($element, $depth) {
                if ($depth -gt 20 -or $script:nodeCount -gt 600) { return }
                $script:nodeCount++
                "  " * $depth + "$($element.Current.ControlType.ProgrammaticName) | $($element.Current.Name) | $($element.Current.AutomationId)"
                $child = $walker.GetFirstChild($element)
                while ($child) { Write-Tree $child ($depth+1); $child = $walker.GetNextSibling($child) }
            }
            $walker = [System.Windows.Automation.TreeWalker]::RawViewWalker
            $script:nodeCount = 0
            Write-Tree $root 0 | Out-File "$diag/uia-tree.txt" -Encoding utf8
        }
    } catch { Write-Warning "UIA diagnostic: $_" }
    throw $failure
} finally {
    if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
    $process.Dispose()
}
