# Exact upstream libmpv binary provenance — M5

**Status: partial provenance evidence, NOT distribution authorization.**

## Binary identity

- `libmpv-2.dll` SHA-256: `df006bfc754d1212fb2084c6a6da04876882648c025ccb56cf1132b27b1e7913`.
- Source archive: `mpv-dev-lgpl-x86_64-20261120-git-94127e1e1a.7z`.
- Archive SHA-256: `6edded03578c35ec1a89c1c50ad92bd52a79521dec6180f49fd697f5e18f367b`.
- [Upstream GitHub Release 2026-11-20-94127e1e1a](https://github.com/zhongfly/mpv-winbuild/releases/tag/2026-11-20-94127e1e1a), published 2026-11-20; GitHub Release asset digest matches the archived hash.
- [Upstream build workflow](https://github.com/zhongfly/mpv-winbuild/actions/runs/44732170261), job [Build MPV (64, true)](https://github.com/zhongfly/mpv-winbuild/actions/runs/44732170261/job/136274516587), SUCCESS.
- The job's artifact upload reported the same mpv archive digest. Its retained `64-lgpl_logs` artifact (ID `14627572112`) was inspected to verify the configure output.

## Exact source commits and observed configuration

- zhongfly/mpv-winbuild workflow commit: [`49520b9987f0e6dc7cf0db83634e45c416bd58b6`](https://github.com/zhongfly/mpv-winbuild/commit/49520b9987f0e6dc7cf0db83634e45c416bd58b6).
- Upstream release tag ref at recording: `40802a561a3f4e2af6d9bfdb996285a45795e951` (distinct from workflow SHA).
- mpv source: [`94127e1e1aa85734482cb563cfb79cf9b6eb7772`](https://github.com/mpv-player/mpv/commit/94127e1e1aa85734482cb563cfb79cf9b6eb7772).
- FFmpeg source: [`7ddf7bc8a482cf28b4fab2e63b7093ba35cb6a80`](https://github.com/FFmpeg/FFmpeg/commit/7ddf7bc8a482cf28b4fab2e63b7093ba35cb6a80); the upstream build also used the short hash `7ddf7bc8a` in `ffmpeg-lgpl-x86_64-git-7ddf7bc8a.7z`.
- The upstream job applied `compile-lgpl-libmpv.patch`; inspect the [exact workflow revision's patch](https://github.com/zhongfly/mpv-winbuild/blob/49520b9987f0e6dc7cf0db83634e45c416bd58b6/compile-lgpl-libmpv.patch).
- From `ffmpeg-configure-out.log`: **License: LGPL version 3 or later**, `static: yes`, `shared: no`. Its external-library list includes libaom, libdav1d, libplacebo, libmp3lame, openssl, libass and others; this list is not yet a complete corresponding-source and license manifest.
- From `mpv-configure-out.log`: Meson `gpl: false`, `libmpv: true`, `cplayer: false`. The configure output identifies FFmpeg libraries and their API versions. These are observed build settings, not a legal conclusion.
- The separate Windows-only app-private Vulkan loader is built from explicitly pinned Khronos source by our own M5 script, not sourced from the mpv archive.

## Configured dependency inventory (2026-11-21)

The retained upstream `64-lgpl_logs` artifact contains `ffmpeg-configure-out.log`.
It reports **37 enabled external libraries** and **14 hardware acceleration
providers**, recorded verbatim as names in
[`ffmpeg-config-inventory.json`](ffmpeg-config-inventory.json).
The list includes `libaom`, `libdav1d`, `libmp3lame`,
`libplacebo`, `libopenmpt`, `openssl`, `libass`, and `libjxl`;
hardware providers include platform APIs and headers, not just libraries.

**Critical distinction:** an enabled configure feature is not evidence that
the corresponding library's code was actually linked into `libmpv-2.dll`.
Conversely, this list is not a complete list of code that was linked.
Individual source revision, license, attribution, static-linking and LGPL
relink conditions are **not yet confirmed**.

A dedicated M5 CI preflight checks identity, unique names, exact counts and
the still-pending review flag. The generated binary-free audit JSON includes
a SHA-256 of the inventory and these counts; this is an **evidence
traceability gate, not a license approval gate**.

## Open blockers for redistribution

1. Preserve **exact** corresponding source archives/build recipes (including shinchiro/mpv-winbuild-cmake revision, toolchain and relevant patches), and account for static FFmpeg dependencies and any LGPL relink/replace requirements.
2. Audit every actually-linked dependency and its notices; the upstream build provider explicitly does not guarantee all LGPL-incompatible packages are disabled.
3. Provide the required source access and attribution/notice distribution mechanisms under the applicable licenses. The current preliminary notices and license text alone are not a compliance signoff.
4. Validate installed playback, dependencies, upgrades and uninstall on clean Windows 10 and Windows 11 x64. A hosted Windows Server 2022 smoke is insufficient.

Official upstream checklist: https://ffmpeg.org/legal.html . Release remains **BLOCKED**; no downloadable bundled installer artifact should be published as complete.
