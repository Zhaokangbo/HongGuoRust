"""Inspect the exact runtime downloaded by get-mpv.ps1 on Windows."""
import ctypes as c
import sys
import os
import re
import time
import wave
from array import array
from pathlib import Path

# FFmpeg embeds its av*_configuration output even when statically linked.
image = Path(sys.argv[1]).read_bytes()
configs = {s.decode("ascii") for s in re.findall(rb"[ -~]{100,}", image)
           if b"--" in s and b"--enable-version3" in s}
if not configs:
    raise RuntimeError("No embedded FFmpeg configure string found")
for config in sorted(configs):
    print("FFmpeg configure", config)
    if "--enable-gpl" in config or "--enable-nonfree" in config:
        raise RuntimeError("Unexpected FFmpeg licensing configuration")
dll_directory = os.add_dll_directory(str(Path(sys.argv[1]).resolve().parent))

mpv = c.CDLL(sys.argv[1])
def bind(name, result, *args):
    fn = getattr(mpv, "mpv_" + name)
    fn.restype = result
    fn.argtypes = args
    return fn

ptr = c.c_void_p
create = bind("create", ptr)
initialize = bind("initialize", c.c_int, ptr)
get = bind("get_property_string", ptr, ptr, c.c_char_p)
destroy = bind("terminate_destroy", None, ptr)
free = bind("free", None, ptr)
api = bind("client_api_version", c.c_ulong)()
print("libmpv client API", api >> 16, api & 65535)
ctx = create()
if not ctx:
    raise RuntimeError("mpv_create failed")
try:
    if len(sys.argv) == 4:
        option = bind("set_option_string", c.c_int, ptr, c.c_char_p, c.c_char_p)
        for key, value in {"vid":"no", "ao":"pcm", "ao-pcm-file":sys.argv[3],
                           "audio-format":"s16", "audio-channels":"mono", "length":"1"}.items():
            if option(ctx, key.encode(), value.encode()) < 0:
                raise RuntimeError("Rejected option " + key)
    if initialize(ctx) < 0:
        raise RuntimeError("mpv_initialize failed")
    for key in ("mpv-version", "ffmpeg-version", "mpv-configuration"):
        value = get(ctx, key.encode())
        if not value:
            raise RuntimeError("Missing property " + key)
        try:
            print(key, c.string_at(value).decode())
        finally:
            free(value)
    if len(sys.argv) == 4:
        command = bind("command", c.c_int, ptr, c.POINTER(c.c_char_p))
        args = (c.c_char_p * 3)(b"loadfile", sys.argv[2].encode(), None)
        if command(ctx, args) < 0:
            raise RuntimeError("loadfile rejected")
        # event_id is the first C int in mpv_event; 7 is MPV_EVENT_END_FILE.
        wait = bind("wait_event", c.POINTER(c.c_int), ptr, c.c_double)
        deadline = time.monotonic() + 15
        while wait(ctx, 0.1)[0] != 7:
            if time.monotonic() >= deadline:
                raise RuntimeError("Timed out decoding fixture audio")
finally:
    destroy(ctx)

if len(sys.argv) == 4:
    with wave.open(sys.argv[3]) as wav:
        if wav.getsampwidth() != 2 or wav.getnchannels() != 1:
            raise RuntimeError("Unexpected PCM format")
        rate = wav.getframerate()
        samples = array("h", wav.readframes(rate))
    if len(samples) < rate // 2 or max(samples) < 500:
        raise RuntimeError("Decoded audio is short or silent")
    frequency = sum(a <= 0 < b for a, b in zip(samples, samples[1:])) * rate / len(samples)
    if not 430 < frequency < 450:
        raise RuntimeError(f"Wrong decoded frequency {frequency:.2f} Hz")
    print(f"Verified decoded AAC tone {frequency:.2f} Hz, {len(samples)} PCM samples")
