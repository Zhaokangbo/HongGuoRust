"""Inspect the exact runtime downloaded by get-mpv.ps1 on Windows."""
import ctypes as c
import sys

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
finally:
    destroy(ctx)
