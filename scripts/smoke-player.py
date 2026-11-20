"""Observe changing desktop pixels in the embedded HWND of the real Windows app."""
import ctypes as c
from ctypes import wintypes as wt
from pathlib import Path
import subprocess
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
                    check_output_endpoint()
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
    dll = str(Path(os.environ.get("MPV_RUNTIME_DIR", Path(sys.argv[1]).resolve().parent)) / "libmpv-2.dll")
    subprocess.run([sys.executable, str(Path(__file__).with_name("probe-mpv.py")),
                    dll, str(clip), str(Path(directory) / "audio.wav")], check=True)
    run(sys.argv[1], clip)
