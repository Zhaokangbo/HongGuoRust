"""Observe changing desktop pixels in the embedded HWND of the real Windows app."""
import ctypes as c
from ctypes import wintypes as wt
from pathlib import Path
import subprocess
import sys
import tempfile
import time

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

def run(exe, clip):
    started = time.monotonic()
    process = subprocess.Popen([exe, "--probe-local", str(clip)])
    colors = set()
    last = None
    try:
        while time.monotonic() - started < 35:
            if process.poll() is not None:
                raise RuntimeError(f"Early probe exit {process.returncode}")
            main = owned_window(process.pid)
            child = find(main, None, "Static", "HongGuoRust video") if main else None
            bounds = wt.RECT()
            if child and rect_of(child, c.byref(bounds)):
                foreground(main)
                x, y = (bounds.left + bounds.right)//2, (bounds.top + bounds.bottom)//2
                last = ImageGrab.grab(bbox=(x,y,x+1,y+1), all_screens=True).getpixel((0,0))
                red, green, blue = last[:3]
                color = "red" if red > 165 and max(blue,green) < 85 else (
                    "blue" if blue > 165 and max(red,green) < 85 else None)
                if color and color not in colors:
                    print(f"Observed {color} pixel {last} at {time.monotonic()-started:.3f}s after launch", flush=True)
                    colors.add(color)
                if len(colors) == 2:
                    if not close(main, 0x10, 0, 0) or process.wait(timeout=12) != 0:
                        raise RuntimeError("Normal window close failed")
                    return
            time.sleep(.12)
        raise RuntimeError(f"Missing embedded colors: seen={colors}, last={last}")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()

with tempfile.TemporaryDirectory() as directory:
    clip = Path(directory) / "视频 test 440Hz.mp4"
    subprocess.run([imageio_ffmpeg.get_ffmpeg_exe(), "-hide_banner", "-loglevel", "error",
        "-f", "lavfi", "-i", "color=red:s=320x180:r=25:d=1",
        "-f", "lavfi", "-i", "color=blue:s=320x180:r=25:d=1",
        "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
        "-filter_complex", "[0:v][1:v]concat=n=2:v=1:a=0,loop=loop=-1:size=50:start=0[v]",
        "-map", "[v]", "-map", "2:a", "-t", "45", "-c:v", "libx264",
        "-pix_fmt", "yuv420p", "-c:a", "aac", str(clip)], check=True)
    run(sys.argv[1], clip)
