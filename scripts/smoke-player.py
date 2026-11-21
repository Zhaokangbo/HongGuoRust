"""Observe changing desktop pixels in the embedded HWND of the real Windows app."""
import ctypes as c
from ctypes import wintypes as wt
from pathlib import Path
import subprocess
import tkinter as tk
import psutil
import win32api
import win32gui
from pywinauto import Desktop
import sys
import os
import tempfile
import time
import math
from array import array

import imageio_ffmpeg
from PIL import ImageGrab

user = c.WinDLL("user32", use_last_error=True)
def bind(name, result, *args):
    fn = getattr(user, name)
    fn.restype = result
    fn.argtypes = args
    return fn

bind("SetThreadDpiAwarenessContext", wt.HANDLE, wt.HANDLE)(-4)
find = bind("FindWindowExW", wt.HWND, wt.HWND, wt.HWND, wt.LPCWSTR, wt.LPCWSTR)
pid_of = bind("GetWindowThreadProcessId", wt.DWORD, wt.HWND, c.POINTER(wt.DWORD))
rect_of = bind("GetWindowRect", wt.BOOL, wt.HWND, c.POINTER(wt.RECT))
close = bind("PostMessageW", wt.BOOL, wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM)
foreground = bind("SetForegroundWindow", wt.BOOL, wt.HWND)

def owned_window(pid):
    hwnd = None
    while hwnd := find(None, hwnd, None, "HongGuoRust"):
        owner = wt.DWORD()
        pid_of(hwnd, c.byref(owner))
        if owner.value == pid:
            return hwnd
    return None

def check_output_endpoint():
    # Only sample the isolated hosted runner; do not capture a developer's desktop audio.
    if os.environ.get("GITHUB_ACTIONS") != "true":
        print("WASAPI endpoint sampling skipped outside CI")
        return
    import pyaudiowpatch as pa
    with pa.PyAudio() as audio:
        devices = list(audio.get_loopback_device_info_generator())
        if not devices:
            print("NO WASAPI LOOPBACK ENDPOINT; physical audio awaiting desktop acceptance")
            return
        device = audio.get_default_wasapi_loopback()
        channels, rate = device["maxInputChannels"], int(device["defaultSampleRate"])
        with audio.open(format=pa.paInt16, channels=channels, rate=rate, input=True,
                        input_device_index=device["index"], frames_per_buffer=2048) as stream:
            samples = array("h", stream.read(rate, exception_on_overflow=False))[::channels]
        energy = sum(x*x for x in samples)
        angular = 2*math.pi*440/rate
        real = sum(x*math.cos(i*angular) for i,x in enumerate(samples))
        imag = sum(x*math.sin(i*angular) for i,x in enumerate(samples))
        tone_fraction = 2*(real*real+imag*imag)/max(1,len(samples)*energy)
        if max(samples, default=0) < 100 or tone_fraction < .5:
            raise RuntimeError(f"Default WASAPI output has no matching tone: {tone_fraction:.3f}")
        print(f"Observed 440 Hz in WASAPI endpoint {device['name']}, fraction {tone_fraction:.3f}")

def until(read, label, timeout=30):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = read()
        if value:
            return value
        time.sleep(.12)
    raise RuntimeError("Timed out waiting for " + label)


def displayed_pixel(child):
    box = wt.RECT()
    if not rect_of(child, c.byref(box)):
        raise RuntimeError("Video host rectangle disappeared")
    x, y = (box.left + box.right)//2, (box.top + box.bottom)//2
    return ImageGrab.grab(bbox=(x,y,x+1,y+1), all_screens=True).getpixel((0,0))[:3]


def frame(child, accepted):
    def sample():
        red, green, blue = displayed_pixel(child)
        if red > 165 and max(blue,green) < 85:
            return "red" if "red" in accepted else None
        if blue > 165 and max(red,green) < 85:
            return "blue" if "blue" in accepted else None
        if green > 80 and max(red,blue) < 65:
            return "green" if "green" in accepted else None
        return None
    return until(sample, "fixture pixels " + str(accepted))


def desktop_checks(main, child):
    show = bind("ShowWindow", wt.BOOL, wt.HWND, c.c_int)
    iconic = bind("IsIconic", wt.BOOL, wt.HWND)
    dpi = bind("GetDpiForWindow", wt.UINT, wt.HWND)
    system_dpi = bind("GetDpiForSystem", wt.UINT)
    tool = os.environ.get("M2_CI_SETDPI") if os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted" else None
    original = subprocess.check_output([tool, "value"], text=True).strip() if tool else ""
    can_set = original.isdigit() and int(original) >= 100
    print(f"Display scale before test: window={dpi(main)}, system={system_dpi()}, configurable={can_set}", flush=True)
    try:
        for target in ([100, 150] if can_set else [None]):
            if target:
                before_window, before_system = dpi(main), system_dpi()
                print(subprocess.check_output([tool, str(target)], text=True, timeout=10), flush=True)
                expected = round(96 * target / 100)
                try:
                    until(lambda: dpi(main) == expected, f"actual {target}% display scaling", 12)
                except RuntimeError:
                    after_window, after_system = dpi(main), system_dpi()
                    if target == 150 and after_window == before_window and after_system == before_system:
                        # SetDPI can update the *configured* scale without applying it
                        # in the GitHub runner's existing interactive desktop session.
                        # This is not evidence of either application success or failure.
                        print(
                            f"SKIP: 150% DPI was not applied to runner desktop "
                            f"(window={after_window}, system={after_system}); "
                            "150% requires physical Windows desktop acceptance",
                            flush=True,
                        )
                        continue
                    # If Windows actually changed DPI but our HWND did not follow,
                    # preserve the test failure: this can be an application bug.
                    raise
            for handle, _, _ in win32api.EnumDisplayMonitors():
                left, top, _, _ = win32api.GetMonitorInfo(handle)["Work"]
                win32gui.SetWindowPos(main, 0, left+25, top+30, 720, 480, 0)
                until(lambda: win32api.MonitorFromWindow(main) == handle, "window on test monitor")
                frame(child, {"green"})
                def correct_bounds():
                    width, height = win32gui.GetClientRect(main)[2:]
                    x, y = win32gui.ClientToScreen(main, (0,0))
                    scale = dpi(main) / 96
                    edges = [x + 18*scale,y + 66*scale,x + width - 18*scale,y + height - 96*scale]
                    return all(abs(a-b) <= 2 for a,b in zip(win32gui.GetWindowRect(child),edges))
                until(correct_bounds, "scaled client bounds")
                print(f"Resized native host verified at actual {dpi(main)} DPI", flush=True)
    finally:
        if can_set:
            subprocess.run([tool, original],check=True,timeout=10)
    show(main, 6)
    until(lambda: iconic(main), "minimized window")
    show(main, 9)
    until(lambda: not iconic(main), "restored window")
    foreground(main)
    frame(child, {"green"})
    x1,y1,x2,y2 = win32gui.GetWindowRect(child)
    mask = tk.Tk()
    try:
        mask.overrideredirect(True)
        mask.attributes("-topmost", True)
        mask.configure(background="#ff00ff")
        mask.geometry(f"80x80{(x1+x2)//2-40:+d}{(y1+y2)//2-40:+d}")
        mask.update()
        until(lambda: displayed_pixel(child) == (255,0,255), "test occluder")
    finally:
        mask.destroy()
    frame(child, {"green"})
    print("Verified minimize, restore, occlusion and repaint", flush=True)


def run(exe, clip, second, corrupt, short_clip, detailed):
    process = subprocess.Popen([exe])
    try:
        main = until(lambda: owned_window(process.pid), "main HWND", 90)
        child = until(lambda: find(main,None,"Static","HongGuoRust video"), "video HWND")
        ui = Desktop(backend="uia").window(handle=main)
        def text_visible(name):
            return ui.child_window(title=name,control_type="Text").exists(timeout=.3)
        def button(name):
            return ui.child_window(title=name,control_type="Button").wrapper_object()
        until(lambda: text_visible("Idle"), "IPC Idle", 90)
        def select(path):
            until(lambda: button("打开本地 MP4").is_enabled(), "available file picker")
            button("打开本地 MP4").invoke()
            dialog = Desktop(backend="win32").window(process=process.pid,class_name="#32770")
            dialog.wait("visible enabled",timeout=15)
            edits = [e for e in dialog.descendants(class_name="Edit") if e.is_visible()]
            candidates = [e for e in edits if e.control_id() in (1148,1152) or
                          e.parent().control_id() == 1148]
            if not candidates and len(edits) == 1:
                candidates = edits
            if len(candidates) != 1:
                raise RuntimeError(f"Filename edit controls: {[(e.control_id(),e.parent().control_id()) for e in edits]}")
            picker = dialog.wrapper_object()
            picker.set_focus()
            edit = candidates[0]
            accept = dialog.child_window(control_id=1,class_name="Button").wrapper_object()
            def submitted():
                # Validate the edit after shell initialization. BM_CLICK can be
                # ignored by an inactive common dialog on a hosted desktop.
                if not win32gui.IsWindow(picker.handle) or not win32gui.IsWindowVisible(picker.handle):
                    return True
                if not win32gui.IsWindowEnabled(picker.handle):
                    return False
                edit.set_edit_text(str(path))
                if edit.window_text() == str(path) and accept.is_enabled():
                    win32gui.PostMessage(picker.handle,0x0111,1,accept.handle)  # WM_COMMAND, IDOK
                    time.sleep(.45)
                return not win32gui.IsWindow(picker.handle) or not win32gui.IsWindowVisible(picker.handle)
            try:
                until(submitted, f"selection of {path.name}", 20)
            except Exception:
                for dlg in Desktop(backend="win32").windows(process=process.pid,class_name="#32770"):
                    print("Picker diagnostic",dlg.window_text(),[(x.class_name(),x.control_id(),x.window_text()) for x in dlg.descendants() if x.is_visible()],flush=True)
                raise
        select(clip)
        foreground(main)
        if detailed:
            t = time.monotonic()
            first = frame(child, {"red","blue"})
            print(f"First sampled {first} pixel {time.monotonic()-t:.3f}s after picker returned", flush=True)
            frame(child, {"blue" if first=="red" else "red"})
            check_output_endpoint()
            until(lambda: text_visible("Playing"), "Rust Playing")
            select(second)
            frame(child, {"green"})
            until(lambda: text_visible(second.name) and text_visible("Playing"), "replacement snapshot")
            desktop_checks(main, child)
            button("停止").invoke()
            until(lambda: text_visible("Idle"), "stopped snapshot")
            select(clip)
            select(second)
            frame(child, {"green"})
            until(lambda: text_visible(second.name) and text_visible("Playing"), "rapid A/B replacement")
            select(corrupt)
            until(lambda: text_visible("Error"), "corrupt MP4 error")
            select(short_clip)
            until(lambda: text_visible("Ended"), "natural end of file")
            select(second)
            frame(child, {"green"})
            until(lambda: text_visible("Playing"), "recovered playback")
        else:
            until(lambda: text_visible("Playing"), "playing before close")
        if detailed:
            button("打开本地 MP4").invoke()
            Desktop(backend="win32").window(process=process.pid,class_name="#32770").wait("visible",timeout=15)
        children = psutil.Process(process.pid).children(recursive=True)
        if not close(main,0x10,0,0) or process.wait(timeout=15) != 0:
            raise RuntimeError("Normal playing-window close failed")
        _, alive = psutil.wait_procs(children, timeout=15)
        if alive:
            raise RuntimeError(f"Surviving child PIDs: {[p.pid for p in alive]}")
    except Exception:
        evidence = Path(os.environ.get("HONGGUORUST_LOG_DIR", tempfile.gettempdir()))
        evidence.mkdir(parents=True, exist_ok=True)
        window = owned_window(process.pid)
        if window:
            ImageGrab.grab(bbox=win32gui.GetWindowRect(window),all_screens=True).save(evidence/"playback-failure.png")
        raise
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()

with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as directory:
    clip = Path(directory) / "视频 test 440Hz.mp4"
    subprocess.run([imageio_ffmpeg.get_ffmpeg_exe(), "-hide_banner", "-loglevel", "error",
        "-f", "lavfi", "-i", "color=red:s=320x180:r=25:d=1",
        "-f", "lavfi", "-i", "color=blue:s=320x180:r=25:d=1",
        "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
        "-filter_complex", "[0:v][1:v]concat=n=2:v=1:a=0,loop=loop=-1:size=50:start=0[v]",
        "-map", "[v]", "-map", "2:a", "-t", "45", "-c:v", "libx264",
        "-pix_fmt", "yuv420p", "-c:a", "aac", str(clip)], check=True)
    dll = str(Path(os.environ.get("MPV_RUNTIME_DIR", Path(sys.argv[1]).resolve().parent)) / "libmpv-2.dll")
    subprocess.run([sys.executable, str(Path(__file__).with_name("probe-mpv.py")),
                    dll, str(clip), str(Path(directory) / "audio.wav")], check=True)
    second = Path(directory) / "切换 B.mp4"
    corrupt = Path(directory) / "损坏.mp4"
    corrupt.write_bytes(b"invalid MP4")
    subprocess.run([imageio_ffmpeg.get_ffmpeg_exe(), "-loglevel", "error", "-f","lavfi",
                    "-i","color=green:s=320x180:r=15:d=90","-c:v","libx264","-pix_fmt","yuv420p",str(second)],check=True)
    os.environ["WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS"] = "--force-renderer-accessibility"
    os.environ["WEBVIEW2_USER_DATA_FOLDER"] = str(Path(directory) / "webview-profile")
    short_clip = Path(directory) / "end.mp4"
    subprocess.run([imageio_ffmpeg.get_ffmpeg_exe(), "-loglevel","error","-i",str(clip),
                    "-t","0.6","-c","copy",str(short_clip)],check=True)
    for cycle in range(1,21):
        run(sys.argv[1], clip, second, corrupt, short_clip, cycle == 1)
        print(f"PASS playing-window close {cycle}/20; no surviving observed descendants", flush=True)
