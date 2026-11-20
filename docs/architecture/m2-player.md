# Windows native playback probe

M2-A builds the existing Tauri client with the `mpv-probe` Cargo feature. The ordinary M1 build and its Windows IPC / process-exit workflow are independent.

| Candidate | Windows integration | Decision |
| --- | --- | --- |
| tauri-plugin-libmpv 0.3.2, MPL-2.0 | Its Rust MpvExt interface can initialize and destroy a player. Its desktop backend wraps a second native library and asynchronously emits window-labelled JSON events. Its CloseRequested handler owns part of teardown. | A viable Rust integration. The extra DLL and window-scoped asynchronous callback boundary add work to session attribution and shutdown. No plugin code is copied. |
| libmpv2 7.0.1, libmpv2-sys 4.0.1 | Cargo-locked safe methods around the libmpv C client, a single worker polling wait_event, and RAII. The MSVC build requires mpv.lib. | Selected. The import library is generated locally from the verified DLL's exports using MSVC dumpbin and lib. No hand-written mpv ABI or JavaScript player object. |

[Plugin source](https://github.com/nini22P/tauri-plugin-libmpv/tree/5da4e044c276245dcfd8a279310e5106394a0679) · [Rust API](https://docs.rs/libmpv2/7.0.1/libmpv2/struct.Mpv.html)

## Native host and observations

Rust creates a child STATIC HWND above WebView2 in the main client area and supplies it as wid before mpv initialization. The WebView owns the header and footer. The size calculation uses the physical Windows client rectangle and the Tauri scale factor. A render-API integration would also need a graphics context, a presentation loop and surface teardown. The wid probe tests whether the simpler D3D path meets the acceptance criteria. Mixed-DPI monitors, occlusion and restore need desktop checks.

A Unicode Win32 file dialog runs on a blocking thread. IPC accepts no path argument. The native probe command line also accepts an explicitly supplied local fixture. The playback worker owns the libmpv handle. Replacement waits for the previous worker to exit. CloseRequested keeps the HWND alive while the worker releases libmpv, then destroys the window. The feature-gated probe has no detached JavaScript playback state. Production service integration follows the audio and video acceptance gate.

PlaybackRestart measures libmpv's timeline restart. The screen test independently samples the center of the identified child HWND for two known fixture colors. It records elapsed time from process launch to each first sampled color. Sampling gives an upper bound on desktop visibility and does not timestamp a GPU present. Physical output requires an audio endpoint and listening evidence.

## Native dependency provenance

| Item | Pinned input |
| --- | --- |
| mpv | 94127e1e1aa85734482cb563cfb79cf9b6eb7772 |
| LGPL x64 dev archive | mpv-dev-lgpl-x86_64-20261120-git-94127e1e1a.7z · SHA-256 `6edded03578c35ec1a89c1c50ad92bd52a79521dec6180f49fd697f5e18f367b` |
| Extracted libmpv-2.dll | SHA-256 `df006bfc754d1212fb2084c6a6da04876882648c025ccb56cf1132b27b1e7913` |
| FFmpeg in that upstream x64 LGPL job | 7ddf7bc8a, statically included. PE imports contain no avcodec/avformat DLL. |
| libmpv2 7.0.1 / libmpv2-sys 4.0.1 | Crates declare LGPL-2.1. SHA-256 `f5e0af058d13863e58383df4a2b4f5242815afada24506cba24ea049683b9c80` and `edb32cabe7176b7d270b0dca6ff418b75187ae8d3854423e3d06fbff376841e4`. |
| Vulkan loader | Hard PE import even when selecting D3D. Prefer the installed system loader. CI can compile Khronos Vulkan-Loader and Vulkan-Headers v1.4.321, pinned to Git object IDs in the MSVC preparation script. |

The upstream LGPL job patches the pinned CMake recipe, builds mpv with -Dgpl=false, removes FFmpeg --enable-gpl and x264/x265, and retains --enable-version3. The applicable FFmpeg core license for those flags is LGPL v3 or later. mpv's non-GPL subset is available under LGPL v2.1 or later, subject to the included code and other dependency terms. The project remains GPL-3.0-only. The pinned developer archive contains headers, the DLL and a GNU import library. It contains no complete notice or corresponding-source collection. CI publishes our application executable only. Any future binary redistribution of the runtime needs the applicable notices, corresponding sources, build patches and LGPL replacement/relinking provisions for the actual combined build.

[Upstream archive](https://github.com/zhongfly/mpv-winbuild/releases/tag/2026-11-20-94127e1e1a) · [Exact upstream job](https://github.com/zhongfly/mpv-winbuild/actions/runs/44732170261/job/136274516587) · [LGPL build patch](https://github.com/zhongfly/mpv-winbuild/blob/49520b9987f0e6dc7cf0db83634e45c416bd58b6/compile-lgpl-libmpv.patch) · [Base recipe](https://github.com/shinchiro/mpv-winbuild-cmake/blob/7e36e1eab5094a642c5a3c1771551f06b5ec3224/packages/ffmpeg.cmake) · [mpv copyright](https://github.com/mpv-player/mpv/blob/94127e1e1aa85734482cb563cfb79cf9b6eb7772/Copyright) · [FFmpeg license](https://github.com/FFmpeg/FFmpeg/blob/7ddf7bc8a/LICENSE.md)
