# Windows native playback probe

M2-A validated the native child window using a feature-gated prototype. The default client now uses that same host and the Rust PlayerService. Both CI workflows exercise the production service.

| Candidate | Windows integration | Decision |
| --- | --- | --- |
| tauri-plugin-libmpv 0.3.2, MPL-2.0 | Its Rust MpvExt interface can initialize and destroy a player. Its desktop backend wraps a second native library and asynchronously emits window-labelled JSON events. Its CloseRequested handler owns part of teardown. | A viable Rust integration. The extra DLL and window-scoped asynchronous callback boundary add work to session attribution and shutdown. No plugin code is copied. |
| libmpv2 7.0.1, libmpv2-sys 4.0.1 | Cargo-locked safe methods around the libmpv C client, a single worker polling wait_event, and RAII. The MSVC build requires mpv.lib. | Selected. The import library is generated locally from the verified DLL's exports using MSVC dumpbin and lib. No hand-written mpv ABI or JavaScript player object. |

[Plugin source](https://github.com/nini22P/tauri-plugin-libmpv/tree/5da4e044c276245dcfd8a279310e5106394a0679) · [Rust API](https://docs.rs/libmpv2/7.0.1/libmpv2/struct.Mpv.html)

## Native host and observations

Rust creates a child STATIC HWND above WebView2 in the main client area and supplies it as wid before mpv initialization. The WebView owns the header and footer. The size calculation uses the physical Windows client rectangle and the Tauri scale factor. A render-API integration would also need a graphics context, a presentation loop and surface teardown. The wid probe tests whether the simpler D3D path meets the acceptance criteria. Mixed-DPI monitors, occlusion and restore need desktop checks.

A Unicode Win32 file dialog returns a local path in Rust. IPC accepts no path argument. One worker owns the current libmpv instance. The command queue serializes replacement, stop and shutdown. Session IDs guard state transitions. CloseRequested waits for the worker to release libmpv before destroying its parent HWND.

PlaybackRestart measures libmpv's timeline restart. The screen test independently samples the center of the identified child HWND for two known fixture colors. It records elapsed time from process launch to each first sampled color. Sampling gives an upper bound on desktop visibility and does not timestamp a GPU present. Physical output requires an audio endpoint and listening evidence.

The first Windows run observed gpu-next, pixel RGB (254, 0, 0) at 18.016 s and (0, 0, 254) at 18.922 s after process launch, and closed the playing window with exit 0. The same run reported no active audio driver. [Windows probe log](https://github.com/Zhaokangbo/HongGuoRust/actions/runs/44755879638/job/136355344096)

## Native dependency provenance

| Item | Pinned input |
| --- | --- |
| mpv | 94127e1e1aa85734482cb563cfb79cf9b6eb7772 |
| LGPL x64 dev archive | mpv-dev-lgpl-x86_64-20261120-git-94127e1e1a.7z · SHA-256 `6edded03578c35ec1a89c1c50ad92bd52a79521dec6180f49fd697f5e18f367b` |
| Extracted libmpv-2.dll | SHA-256 `df006bfc754d1212fb2084c6a6da04876882648c025ccb56cf1132b27b1e7913` |
| FFmpeg in that upstream x64 LGPL job | 7ddf7bc8a, statically included. PE imports contain no avcodec/avformat DLL. |
| libmpv2 7.0.1 / libmpv2-sys 4.0.1 | Crates declare LGPL-2.1. SHA-256 `f5e0af058d13863e58383df4a2b4f5242815afada24506cba24ea049683b9c80` and `edb32cabe7176b7d270b0dca6ff418b75187ae8d3854423e3d06fbff376841e4`. |
| Vulkan loader | Hard PE import even when selecting D3D. Prefer the installed system loader. CI can compile Khronos Vulkan-Loader and Vulkan-Headers v1.4.321, pinned to Git object IDs in the MSVC preparation script. |

The DLL reports client API 2.6, mpv v0.42.0-172-g94127e1e1, FFmpeg N-129150-g7ddf7bc8a and -Dgpl=false. Its stripped image did not expose the full FFmpeg configure command. CI checks the same upstream job's companion FFmpeg executable, archive SHA-256 `ba0fc7892c4840a6d1f157a59c92581554dd332cdbb369ca4ce2a8fa6de86a4a`, against the DLL's FFmpeg revision and the expected configure flags and license. The companion is only used for provenance.

The upstream LGPL job patches the pinned CMake recipe, builds mpv with -Dgpl=false, removes FFmpeg --enable-gpl and x264/x265, and retains --enable-version3. The applicable FFmpeg core license for those flags is LGPL v3 or later. mpv's non-GPL subset is available under LGPL v2.1 or later, subject to the included code and other dependency terms. The project remains GPL-3.0-only. The pinned developer archive contains headers, the DLL and a GNU import library. It contains no complete notice or corresponding-source collection. CI publishes our application executable only. Any future binary redistribution of the runtime needs the applicable notices, corresponding sources, build patches and LGPL replacement/relinking provisions for the actual combined build.

[Upstream archive](https://github.com/zhongfly/mpv-winbuild/releases/tag/2026-11-20-94127e1e1a) · [Exact upstream job](https://github.com/zhongfly/mpv-winbuild/actions/runs/44732170261/job/136274516587) · [LGPL build patch](https://github.com/zhongfly/mpv-winbuild/blob/49520b9987f0e6dc7cf0db83634e45c416bd58b6/compile-lgpl-libmpv.patch) · [Base recipe](https://github.com/shinchiro/mpv-winbuild-cmake/blob/7e36e1eab5094a642c5a3c1771551f06b5ec3224/packages/ffmpeg.cmake) · [mpv copyright](https://github.com/mpv-player/mpv/blob/94127e1e1aa85734482cb563cfb79cf9b6eb7772/Copyright) · [FFmpeg license](https://github.com/FFmpeg/FFmpeg/blob/7ddf7bc8a/LICENSE.md)
