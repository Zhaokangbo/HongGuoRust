# HongGuoRust — AI 多对话开发交接与持续维护记录

> **长期事实来源（跨对话使用）**。请每次接手开发时，先读取本文件，再核对 GitHub 上的 PR、分支 HEAD、CI 与相关源码。**不要仅凭本文件或聊天记忆假定 CI/合并已完成**。
>
> 更新基准：2026-11-22 UTC。维护方式：该文件直接保存在 GitHub `main` 的 `docs/` 下，独立于尚未合并的功能 PR，因此新对话可以始终从稳定 URL 访问。开发阶段变化后应及时更新此文件。

## 0. HG-2B：HEIC 封面解码兼容修复（2026-11-22 UTC）

- **PR #32 当前状态**：[Draft PR #32](https://github.com/Zhaokangbo/HongGuoRust/pull/32)，open / draft / 未合并，base `main`，分支 `feat/hg2b-real-covers-and-episode-status`。最新 HEAD：`ac6614c94ef35095666054971a7c42c302982e2e`。不得擅自合并。
- **最新 HEAD CI 已全部通过**：
  - Windows M1 run `45018039636` / job `137194513689`：completed / success。通过 `npm run build`（包含 HG2A gateway 15/15 与 HG2B cover 5/5）、依赖漂移检查、`cargo fmt --check`、`cargo test --locked`、`cargo tauri build --no-bundle --locked`、Windows smoke IPC/正常退出检查。产物 M1/M4 validation artifact `14753110754`（`HongGuoRust-M4-windows-x64`），SHA256 `fc398751543d39cf9a34719d3593d38926ebf6c9079c36c62405ab130b1cfb60`；该 release validation artifact 不是本次 HG2B debug 包。
  - Windows M2 playback run `45018039660` / job `137194513799`：completed / success。通过 `npm run build`、`npm audit --audit-level=high`、`npm audit --omit=dev`、`cargo fmt --check`、`cargo test --locked`、debug player build、embedded-pixel 与 playing-window-close 自动探针。Hosted runner 未检测到 WASAPI loopback endpoint（日志：`NO WASAPI LOOPBACK ENDPOINT; physical audio awaiting desktop acceptance`），因此物理声卡/真实扬声器仍须用户 Windows 实机验收。M2 artifact `14753145036`（`HongGuoRust-M2-debug-x64`）。
- **精确构建产物与校验值**：
  - M2 artifact ZIP SHA256：`73eaf34331f173e163c083da54f247a32cda4e678b88cc3a1c6a25c5f4590604`。
  - 内部 `hongguorust.exe`：14,367,744 bytes；SHA256 `f8e177c15ef0e35e394e1cb4aad44ff62c5decbd735bcace9395738d6f0fdf9c`。
  - 已制作 Windows test kit：`HongGuoRust-HG2B-ac6614c-Windows-test-kit.zip`；外层 ZIP SHA256：`f0d3ced8314dc506801a15c842d2d6b55c115277a3e150ca75ae35a668904967`。内含精确 M2 artifact ZIP、部署脚本、README 与 SHA256SUMS。部署目标为 `%LOCALAPPDATA%\\HongGuoRust-HG2B-ac6614c-Test`；复用用户已有可信 `libmpv-2.dll` 与 `vulkan-1.dll`，不会覆盖主用户数据目录或旧测试目录。
- **最新 Windows 实机反馈（2026-11-22）**：用户已在本机 `D:\Download\HongGuoRust-HG2A-Source` checkout 精确 commit `ac6614c94ef35095666054971a7c42c302982e2e`，安装可选 sidecar 依赖并启动 `%LOCALAPPDATA%\HongGuoRust-HG2B-ac6614c-Test\hongguorust.exe`。最新截图显示 9 张不同的真实社区海报，确认 HG2B 清单第 3 项通过。当前仅确认卡片列表封面；详情、分集状态、默认仅浏览及完整回归仍待逐项验收。此前详情 `hgcommunity_http_503` 尚未由新版本实机复测。
- **CI 故障诊断（前一提交 `a2dc7715e6a6a61d8261ed2c2798fb50d2f3d7fd`）**：M1 run `45017603249` / job `137193225513` 与 M2 run `45017603283` / job `137193208192` 均在 `npm run build` 失败。唯一失败为 `heicRuntimeAvailable()` 返回 false（`tests/hg2a-community-gateway.test.mjs:349`）。根因是 `libheif-js/wasm-bundle` 的 CommonJS Node 入口与 ESM extensionless dynamic import 不兼容；最新 HEAD 已用 `createRequire(import.meta.url)` 和 `require('libheif-js/wasm-bundle')` 修复，并在上述新 CI 全部通过。
- **用户 Windows 本地当前状态（2026-11-22 更新）**：用户已切换到 detached HEAD `ac6614c94ef35095666054971a7c42c302982e2e`，安装可选 Sidecar 依赖并成功启动精确的 `HongGuoRust-HG2B-ac6614c-Test` EXE。最新实机截图显示 9 张不同真实封面，HG2B 验收第 3 项通过；详情/剧集和完整回归待继续。不要删除或重置 `scripts/hg2a-sdk/package-lock.json`，也不要覆盖旧测试目录或正式用户数据。
- **根因证据**：用户在真正附加到 HongGuoRust WebView2 的 DevTools Network → Img 中看到封面 HTTP `200 OK`，Remote Address `127.0.0.1:7897`，Response `Content-Type: image/heic`、`Content-Length: 33636`。Preview 面板破图且应用 `CoverImage.tsx` 触发 `img onError`，说明问题是 HEIC 解码兼容而不是 CDN 请求未到达。DevTools Console 的 `.well-known/appspecific/com.chrome.devtools.json` CSP 报错是 DevTools 辅助请求，不是封面请求；不扩大 `connect-src`。
- **代码与安全边界**：
  - `scripts/hg2a-sdk/server.mjs`：受限 Loopback Image Relay，仅允许实测海报域 `fqnovelpic.com` 根域及子域；原始签名 URL 仅保存在 sidecar 进程内存，UI/Rust/诊断不暴露 signed URL。返回随机 128-bit opaque ID 的 `http://127.0.0.1:19091/v1/covers/<id>`。
  - 复用用户现有 `HTTPS_PROXY` / `ALL_PROXY` / `HTTP_PROXY` 环境代理；代理失败不回退直连。最多两次 allowlist 内重定向，源/输出限制 5 MiB、HEIC 解码 12 MP、最多 4 并发、内存缓存 16 MiB/100 张、opaque ID 15 分钟过期；不落盘、不自动重试。
  - HEIC/HEIF 使用 `libheif-js@1.23.5` 解码并以 `jpeg-js@0.4.4` 转 JPEG；`https-proxy-agent@9.1.0` 提供代理支持。LGPL-3.0 第三方许可证/源码信息需在发布时保留。
  - `src/catalog/coverUrl.ts`、`src-tauri/src/catalog/hgcommunity.rs` 接受固定 opaque loopback URL；Tauri CSP 仅在 `img-src` 增加本机图片源，`connect-src` 不变。普通演示/本地内容不改变。
  - 当前只从用户截图确认 `fqnovelpic.com`；如发现其他 CDN host，先核实真实 Network host，再按精确域名后缀扩展。禁止改成任意 URL proxy。DNS 重绑定/代理侧解析差异仍是进一步公开发布审查项。此功能只处理公开封面，不触碰媒体流、DRM、会员权限或播放授权。
- **下一步（当前实机验收）**：无需重新部署/编译。继续使用已启动的 `ac6614c` EXE + Sidecar，先点击一张真实海报，验证详情封面、简介与剧集列表，复测旧 `hgcommunity_http_503`；之后按 `docs/acceptance/hg2b.md` 第 5–10 项检查首页卡片、不可播放剧集和默认仅浏览、安全失败回退、搜索/刷新竞态以及 M7–HG2A 回归。若详情失败，提供终端脱敏 `HG2B_DETAIL_DIAG code=... status=...` 行与 UI 截图，不要复制 signed URL、Cookie、token 或请求头。CI 成功不等于整个 HG2B 实机验收通过；PR #32 继续保持 Draft。

## 1. 项目入口与用户工作方式

- GitHub：`https://github.com/Zhaokangbo/HongGuoRust`。
- 默认分支：`main`；用户 Windows 实机验收后才允许按依赖顺序合并。当前前沿开发会有 **stacked Draft PR**；多个 PR 指向 `main` 仅为了触发工作流，**不是合并授权**。
- 主要技术：Tauri 2 / Rust / libmpv2，前端 React 19 + TypeScript + Vite；Windows x64 是首要运行与验收目标。
- 用户希望 AI **直接使用 GitHub 连接器改文件、建分支和 Draft PR**，不要默认改为“给 Codex 提示词”。用户自己在实际 Windows 机器上运行调试 ZIP 并反馈结果。
- 用户明确要求：**不要持续轮询 CI**。开发提交后确认一次工作流已触发，等待用户说“编译完成”，然后仅查询最终 HEAD 的状态一次；失败则查看日志修复并让下一轮重新构建。
- 对耗时工作及时简短说明进度；绝不承诺后台完成。避免重复询问用户已回答的问题。
- 绝不把“编译成功”说成“Windows 实机通过”，也不能只因用户笼统说“继续”就悄悄合并前置 PR。

## 2. 架构边界（必须保留）

- `src/main.tsx`：播放器 UI/进度条/快捷键；`src/player/*`：纯前端工具、状态与队列；`src/catalog/*`：内容剧库、最近播放；`src/style.css`：UI。
- `src-tauri/src/player/engine.rs`：主 libmpv 实例，只在专用 `libmpv-player` worker 线程中使用；`service.rs` 负责编排 session、控制 IPC、Stop/Close；`state.rs` 为主状态机。
- `src-tauri/src/player/history.rs`：本地历史；`provider_history.rs`、`src-tauri/src/catalog/progress.rs`：Provider 剧集历史/原子化持久化。请维护媒体 URL、凭证和本地绝对路径不在进度文件或 UI 历史卡片中泄漏。
- `src-tauri/src/main.rs` 注册 Tauri IPC，`commands.rs` 提供桥接；`window.rs` 原生 Win32 HWND 视频窗口。HTML 悬浮 UI 不能假定会覆盖原生 HWND：需避开视频区域。
- Native session ID 是播放生命周期的唯一身份；任何异步 Seek、预览、EOF、进度回写和跨集切换必须验证 session，禁止旧会话覆盖新会话。
- M7 本地历史/收藏、M8 本地自动连播、M10-B 手动剧集切换/EOF、M10-C1/C2 Provider 续播与首页历史，不得因新功能回归。
- Windows M1/M2 GitHub Actions 工作流仅在 `pull_request` 指向 `main` 时触发；改变 PR base 本身曾经未触发新构建，需正常提交新变更触发 `synchronize`。

## 3. 当前里程碑与验收状态

| 阶段 | 代码/PR | 自动化/实机状态 | 后续合并条件 |
| --- | --- | --- | --- |
| M1–M10-B | 已合入 main | 历史阶段已验收 | 已完成 |
| M10-C1 Provider 剧集断点、跨重启 | [PR #20](https://github.com/Zhaokangbo/HongGuoRust/pull/20), `feat/m10c1-provider-progress`, HEAD `8da84c56f5b0655623504462ebf836b28e7e062b` | Windows M1/M2 已通过；用户只明确反馈了部分旧验收项目 | **其余手动回归仍需明确确认**，保持 Draft |
| M10-C2 首页继续观看、剧集进度与本地历史合并 | [PR #21](https://github.com/Zhaokangbo/HongGuoRust/pull/21), `feat/m10c2-history-home`, HEAD `096d73b4b12247343b9e4277ea5e360ad38f4e9a` | M1/M2 通过；Windows **第 1–8 项明确通过**，第 9–11 项未被逐项确认 | 先验收并合并 #20，补齐 #21 余下验收，重检最终 HEAD 后合并 |
| M11-A 键盘快捷键/帮助 | [PR #22](https://github.com/Zhaokangbo/HongGuoRust/pull/22), `feat/m11a-player-shortcuts`, HEAD `ba4160dfd422d1f1124b42ec7cfb173c189029a9` | M1/M2 通过；**Windows 10/10 已由用户明确报告全通过** | 先完成并合并 #20、#21，再确认 stacked diff/CI，才合并 #22 |
| M11-B 进度条悬停缩略图 | [PR #23](https://github.com/Zhaokangbo/HongGuoRust/pull/23), `feat/m11b-local-thumbnail-preview`, HEAD `6446271e17b6dc3d59a0a252bd0d65b84d07ad77` | **同一最终 HEAD Windows M1/M2 全部成功**：M1 #44987943244、M2 #44987943231；调试 ZIP/EXE 哈希已验证；**用户已明确报告 Windows 12/12 实机全部通过** | 测试真实 JPEG 缩略图、主播放器无 Seek/音画不中断、跨 session 隔离、弱网降级；#20–#22 先满足合并门槛 |
| M11-C1 音轨/内嵌字幕选择 | [PR #24](https://github.com/Zhaokangbo/HongGuoRust/pull/24), `feat/m11c1-track-selection`, HEAD `f1e281c8f53b25d185efb6a0b28896159689b1ad` | 前一代码 SHA `8f62012fe938188358fb6704f9aff65695bc3de1` 的 M1/M2 均成功，后续仅调整 FFmpeg 测试脚本路径；**用户明确报告 M11-C1 Windows 12/12 全部通过** | 仍 Draft/未合并：前置 #20–#23 尚需各自验收/用户合并授权 |
| M11-C2 音量/倍速/自动连播设置持久化 | [PR #25](https://github.com/Zhaokangbo/HongGuoRust/pull/25), `feat/m11c2-persistent-player-preferences`, HEAD `816721a89848f275159b6c1a70306c3b3e8af63e` | **同一 HEAD Windows M1 #44992071107、M2 #44992071153 completed/success，用户明确反馈 Windows 12/12 实机验收全部通过** | 仍 Draft/未合并；需等 #20–#24 的独立验收与合并授权 |
| M12-A 生命周期诊断/资源基线 | [PR #26](https://github.com/Zhaokangbo/HongGuoRust/pull/26), `feat/m12a-runtime-diagnostics`, HEAD `91a4f488a41391a06bb6e6190149b0e00b534f07` | **同一 SHA 的 Windows M1 #44996601517 / M2 #44996601528 全绿，用户明确反馈 Windows 10/10 实机验收全部通过** | 仍 Draft/未合并，独立的前置 #20–#25 验收与合并授权要求有效 |
| M12-B 长运行/资源趋势/弱网恢复测试 | [PR #27](https://github.com/Zhaokangbo/HongGuoRust/pull/27), `feat/m12b-long-run-stress`, HEAD `a8357f11c670205362ed44293eb85d3388aa51ab` | **同一 HEAD M1 #44998408125/M2 #44998408101 全绿，用户明确报告 Windows 10/10 真机实测全部通过**（包含长运行/弱网/快速切集测试）；已记录到 PR #27 | 仍 Draft/未合并；前置 #20–#26 验收与明确合并授权独立有效 |
| M12-C 异常媒体恢复/原生错误脱敏 | [Draft PR #28](https://github.com/Zhaokangbo/HongGuoRust/pull/28), `feat/m12c-failure-recovery`, HEAD `8067f881a12f624413eedd8895b851d1bb05a551` | **最终 HEAD M1 #44999689761 / M2 #44999689707 全绿，用户明确报告 M12-C Windows 10/10 实机验收全部通过**（合成异常媒体/恢复/脱敏及回归），已更新 PR #28 | 仍 Draft/未合并，前置 #20–#27 依赖和用户明确合并授权要求不变 |
| HG-1 红果授权 API 调研/适配合约 | [Draft PR #29](https://github.com/Zhaokangbo/HongGuoRust/pull/29), `feat/hg1-authorized-provider-gate`, HEAD `a036d65e6212bb324325423e3550ef952395d05d` | **同 SHA Windows M1 #45001304015/M2 #45001304036 均成功，用户明确确认 HG-1 Windows 10/10 实机验收全部通过**。HG-1 的 `hongguo` 仍仅是待授权、无内容、原生禁播占位；#29 仍 Draft | 已开始 HG-2A 直接参考非官方开源 SDK 真实目录/搜索/选集实验，不假定已取得官方内容授权 |
| HG-2A 社区 SDK 真实数据试验 | [Draft PR #31](https://github.com/Zhaokangbo/HongGuoRust/pull/31), latest known SHA `50ff5f07018e84bae93def02bea54543b840d7e7` | 旧 SHA `81d0ac1cde46191961cfa1f148eda3a27aa56838` Windows M1 #45004024720/M2 #45004024741 双绿、EXE/ZIP 哈希核实。**用户 Windows 截图实际显示社区 SDK 搜索“重生”返回 6 部不同真实剧名和集数**，证明真实目录元数据检索已初步可用；截图里的统一蓝绿色封面是 `artwork-studio` CSS 占位，而非 SDK 已加载的海报。用户尚未报告 HG-2A 10/10 完整验收或真实片源实际播放。PR #31 的新脚本已增加 19091 端口 PID 预检及 `EADDRINUSE` 诊断，不擅自杀 Node 进程 | HG-2B 重点补真实封面/详情/不可播放分集 UI；不要将真实元数据搜索成功推断为合法/可播放视频 |
| HG-2B 真实封面及分集状态 | [Draft PR #32](https://github.com/Zhaokangbo/HongGuoRust/pull/32), `feat/hg2b-real-covers-and-episode-status`, **HEAD `ac6614c94ef35095666054971a7c42c302982e2e`** | 同一 HEAD 的 Windows M1 `45018039636` / M2 `45018039660` 均 success。用户已安装可选 sidecar 依赖并启动精确 EXE+sidecar；2026-11-22 最新 Windows 截图显示 9 张不同真实社区封面，**HG2B 验收第 3 项（搜索列表真实封面）明确通过**。此前 image/heic 解码故障已由受限 Loopback HEIC→JPEG relay 修复。详情页封面/剧集加载（第 4 项，之前曾见 `hgcommunity_http_503`）、首页卡片（第 5 项）、不可播放集禁用和默认仅浏览（第 6–7 项）、失败回退/安全边界（第 8 项）、快速刷新与 M7–HG2A 回归（第 9–10 项）仍待逐项确认 | 继续 HG2B 实机验收，先点击作品检查详情和剧集；若失败只提供脱敏错误码/诊断行。PR 保持 Draft，不合并 |
| HG-1 原并行前端/禁播方案 | [PR #30](https://github.com/Zhaokangbo/HongGuoRust/pull/30), `feat/hg1-hongguo-provider-gate`, SHA `9e9f68b9e5f30483fc17bc043dc7666682cc4575` | **已关闭（closed, NOT merged）**：其独特的 React tab、Provider capability 及 native 禁播能力已移植到 #29，避免重复维护；没有任何 PR 合并到 main | 后续以 **#29 为唯一 HG-1 活跃主线**，不要尝试独立合并 #30 |

**验收口径**：用户已明确确认 M11-A 10/10、M11-B 12/12、M11-C1 12/12、M11-C2 12/12、M12-A 10/10、M12-B 10/10、M12-C 10/10、HG-1 10/10 Windows 实机通过。HG-2A 截图只证明真实目录搜索元数据有效，未完整验收或验证真实视频播放。**HG-2B 最新进展（2026-11-22）**：PR #32 精确 HEAD `ac6614c94ef35095666054971a7c42c302982e2e` 的 M1/M2 CI 均通过；用户最新 Windows 截图显示 9 张真实不同社区海报，已明确通过 HG2B 验收第 3 项（列表封面）。详情页/分集、第 5 项首页封面、第 6–7 项不可播放保护与默认仅浏览、第 8 项安全失败回退、第 9–10 项竞态与回归尚未逐项验收，不能将整个 HG-2B 标记完成。此前 `image/heic` 导致 WebView2 `img onError` 的根因已由本机受限 HEIC→JPEG relay 修复。#20/#21 部分验收仍未确认。

## 4. M11-B 当前开发实现与风险

- 分支：`feat/m11b-local-thumbnail-preview`；最终 HEAD 请在 GitHub 连接器重新核对。
- 新增 `src-tauri/src/player/preview.rs`：独立 libmpv `vo=image`/JPEG 生成、5 秒时间桶、280ms 前端去抖、最长约 2.4 秒轮询、一组 12 张/约 2MiB LRU 缓存，临时目录清理，generation/Native session 隔离。
- `service.rs` 在 Rust 内存保留活动 session->MediaSource；异步 `player_preview_frame` IPC 只向 UI 发送会话 ID、时间和 data:image/jpeg;base64，不暴露媒体 URL 或文件路径。
- `src/player/thumbnailPreview.ts` 计算指针时间、桶和数据有效性；`src/main.tsx` 的进度条 hover 弹层；`src/style.css` 将缩略图限制在 HTML footer 中，避免被原生视频 HWND 遮盖。
- `tests/thumbnail-preview.test.mjs` 已加入 `npm run build` 的 Node 测试链；Rust 新增缓存/时间和 base64 单元测试。
- **已验证**：M11-B Windows 真机 12/12 通过（用户明确反馈），包括真实缩略图、主音画不受干扰及异常场景；未来更换 DLL 时仍需回归。
- **注意**：本方案未加入新的 Cargo 依赖；新旧 Rust 源码的 fmt、编译或运行错误应优先用 M1/M2 CI 日志精确定位。可能出现 `libmpv2` 参数类型/Windows image VO 的实际兼容问题，须以 CI/实机为准。
- 完整验收见 `docs/acceptance/m11-b.md`（**仅存在于尚未合并的 M11-B 分支**，查看时务必用对应分支）。

## 5. 当前下一步操作（每次开始应重新核对）

1. **HG-2B 详情页实机验收（2026-11-22）已取得关键通过项**：[Draft PR #32](https://github.com/Zhaokangbo/HongGuoRust/pull/32)，`feat/hg2b-real-covers-and-episode-status`，经重新核对仍为 Open / Draft / 未合并，HEAD `ac6614c94ef35095666054971a7c42c302982e2e`。用户最新 Windows 截图显示真实作品封面、作品简介、共 107 集的真实剧集列表，以及顶部明确的“真实剧集目录浏览模式 / 未配置允许的媒体域名 / 不会尝试解析或播放视频”提示；每个剧集按钮均显示“仅可浏览”。本次记为：详情封面通过、简介通过、107 集列表通过、全局仅浏览/播放保护通过。**单集上游 `playable=false` 状态与全局仅浏览禁用的视觉区分尚未由该截图证明**，仍需单独验收；也未据此宣称封面失败回退、搜索刷新竞态及 M7–HG2A 全回归通过。此前同一 HEAD 的 Windows M1/M2 CI 均 completed/success（见下方 run）；截图本身未展示 EXE 哈希，若后续测试包不是该 HEAD 对应产物需重新核对。PR 不得擅自合并。
2. **核心改动已提交**：`scripts/hg2a-sdk/server.mjs` 现在从开源 `openhongguo@0.4.0` 返回的 `cover_url` 转发安全合法的 `coverUrl`，保留签名查询参数；`src-tauri/src/catalog/mod.rs` 的 `Series.cover_url: Option<String>`，`src-tauri/src/catalog/hgcommunity.rs` 独立 URL 校验；`src/catalog/coverUrl.ts` 和 `CoverImage.tsx` 懒加载，错误回退旧渐变封面；页面卡片、首页焦点/普通卡片和作品详情使用真海报。Tauri CSP 仅放行 `img-src https:`，`connect-src` 不变，图片发往 CDN 仍有 IP/远端可观测性问题；字符串校验不是完整 SSRF/DNS/redirect 安全方案。
3. **分集状态**：Node SDK 保留 `playable=false` 条目而不删掉，Rust `Episode.playable: bool` 各现有 Provider 默认 true，HG community 从上游真实字段读取；界面保留禁播集数并提示上游不可播放，native resolve 再次强制检验；未明确允许媒体主机时社区源详情只能浏览、选集按钮不可点。真实元数据可见不等于 DRM/会员/播放授权，禁止绕过。
4. **测试与文档**：`tests/hg2b-cover.test.mjs` 覆盖 Node 前端双向 URL 拒绝 HTTP、内网 IP、带密码 URL、非标准端口、控制字符、过长 URL，合法 HTTPS 带签名 query 保留；`tests/hg2a-community-gateway.test.mjs` 模拟真实 `cover_url` 和不可播放选集，Node 测试链加入 `npm run build`。Rust 增加 `valid_cover_url` 单测。[HG-2B Windows 10 项实机验收](https://github.com/Zhaokangbo/HongGuoRust/blob/feat/hg2b-real-covers-and-episode-status/docs/acceptance/hg2b.md) 要求看到实际不同封面/详情、失败回退、`playable=false` 禁播及 M7–HG2A 回归。
5. **HG-2B 最新 CI 与 Windows 调试包已完成**：精确 HEAD `ac6614c94ef35095666054971a7c42c302982e2e` 的 Windows M1 [#45018039636](https://github.com/Zhaokangbo/HongGuoRust/actions/runs/45018039636) 与 M2 playback [#45018039660](https://github.com/Zhaokangbo/HongGuoRust/actions/runs/45018039660) 均 completed/success；M2 artifact `14753145036`，ZIP SHA256 `73eaf34331f173e163c083da54f247a32cda4e678b88cc3a1c6a25c5f4590604`，内部 EXE SHA256 `f8e177c15ef0e35e394e1cb4aad44ff62c5decbd735bcace9395738d6f0fdf9c`。Windows test kit 外层 SHA256 `f0d3ced8314dc506801a15c842d2d6b55c115277a3e150ca75ae35a668904967`，用户已部署并安装 Sidecar 依赖，最新截图确认 9 张真实封面显示。M2 runner 无 WASAPI loopback endpoint，物理音频仍须实机验收。接下来不需要重编译，先用当前运行版本验收详情/剧集和剩余 HG2B 项；只有发现代码缺陷才改代码并触发新 CI。
6. **后续方向 HG-2C/3（HG-2B 10 项验收完成后启动）**：进行合法媒体能力与格式兼容调查。先静态审查 openhongguo `v0.4.0` 的 Node 类型/Go SDK 源码，不对真实作品调用播放解析，不采集/保存媒体 URL 或密钥；只建立 `protected`、HLS、DASH、直接 MP4、编码/请求头依赖的支持矩阵。上游 README 明确声明仅供学习研究、禁止商业用途和大规模采集；该非官方 SDK 不是平台授权 API，公开代码/Apache-2.0 包许可证也不等于平台内容/播放授权。未来若无平台明确许可或用户自有内容权利，只做浏览和不支持原因提示；不读取/使用 contentKey、不做 CENC/DRM 解密、不绕过会员/登录/验证码、不下载或分发短剧。优先评估用户自有/获授权媒体清单的直接 MP4/HLS/DASH 支持，单独建阶段和验收；Issue #7 第三方 DLL/SDK/版权发布许可仍独立阻断公开发行。

## 6. 构建/交付标准与协作手册

- Windows 工作流：`Windows M1` 和 `Windows M2 playback`，同一最新 HEAD 的 `status=completed` + `conclusion=success` 才算 CI 通过。GitHub Actions 页面：`https://github.com/Zhaokangbo/HongGuoRust/actions`。
- M2 调试产物通常名 `HongGuoRust-M2-debug-x64`，ZIP 内是 `hongguorust.exe`；`libmpv-2.dll`、`vulkan-1.dll` 需复用用户旧版已验证的私有 runtime，部署到新测试目录（例：`%LOCALAPPDATA%\HongGuoRust-M11B-Test`）。
- **不要覆盖、删除** `%LOCALAPPDATA%\HongGuoRust` 数据目录（`library.json`、`provider-progress.json`）。第三方二进制发布合规议题为 Issue #7；未经处理不得做公开发行。
- 实际 Windows 验收由用户报告，不能用 Github runner 的 CLI probe 替代。
- 修改源文件时：Windows 路径大小写不敏感，**不要创建仅大小写不同的 TypeScript 模块名**；此前 `recentActivity.ts` 与 `RecentActivity.tsx` 导致 CI 失败，后改成 `ContinueWatchingShelf.tsx`。
- git SHA、artifact ID、运行状态等是会变化的；永远从 GitHub 校验，而非直接复述文件中可能过期的快照。

## 7. 跨对话接力约定

**新对话可直接发送下面这段话：**

> 继续开发我的 GitHub 仓库 `Zhaokangbo/HongGuoRust`。请先读取 main 上的 `AGENTS.md` 和 `docs/AI_DEVELOPMENT_HANDOFF.md`，再通过 GitHub 核实当前所有相关 PR 的状态/HEAD、Windows Actions 和实际代码，不要把交接文档中的旧 SHA 当作当前事实。HG-2B PR #32 当前记录的基线是 `feat/hg2b-real-covers-and-episode-status` / `ac6614c94ef35095666054971a7c42c302982e2e`；2026-11-22 我已在 Windows 实机确认详情真实封面、简介、107 集列表和全局仅浏览/不解析播放保护通过，但单集上游 `playable=false` 的独立状态显示、封面失败回退、搜索竞态及 M7–HG2A 回归仍未全部验收。下一步按交接文档进入 UI-1：先使用 Product Design 完成三套视觉方向并让我选择，再创建独立 Draft PR；选择方向前不要大规模改 UI。必须保持 Rust/libmpv worker、native session ID、Tauri IPC、Provider 数据契约、媒体域名白名单和原生视频 HWND 几何不变。由你直接维护 GitHub 代码和 PR，我负责 Windows 实机验收；我说“编译完成”后再查看 CI；没有我的明确授权不要合并。**每一项实质代码进展、验收结论或里程碑，都要及时提交 GitHub 并同步更新 main 上的交接文档**，不要等到会话结束才补记。

**交接文件更新纪律：**每个阶段至少维护：当前阶段、PR/branch/HEAD、构建状态+artifact ID、已经明确通过的验收条目、尚待验证的条目、遇到的问题/修复、下一步动作。**禁止把未运行/未通过状态写成通过。**优先将本文件更新到 main，确保下一段对话不会从旧功能分支获取过期状态；如果 main 提交权限不可用，才更新工作分支并明确告知用户路径。

## 9. 用户要求的 GitHub 同步纪律

- 每一项实质代码进展、用户实机验收结果、里程碑完成或开发路线调整，都应在该项完成后及时提交 GitHub；优先同步更新 `main` 上的 `docs/AI_DEVELOPMENT_HANDOFF.md`，必要时同时更新对应验收文档。
- 不能只在聊天中口头记录进度，也不能等会话结束才集中补记。每次提交后核实 commit SHA 和文件内容，再向用户报告链接。
- 仅讨论/分析且没有形成新结论时不制造空提交；没有代码改动时可只提交交接文档，不为刷新 CI 而改动代码。
- 会话切换前，必须提供当前 PR/branch/HEAD、已通过/未通过验收、最新 GitHub commit、下一步和一段可直接复制给新会话的 prompt。会话切换不代表自动合并 PR。

## 8. 参考入口

- 仓库主文档：`README.md`、`docs/roadmap.md`。
- 既有验收：`docs/acceptance/m10-c1.md`、`m10-c2.md`、`m11-a.md`、`m11-b.md`（**部分只在对应功能分支**）。
- 相关 PR：[#20](https://github.com/Zhaokangbo/HongGuoRust/pull/20)、[#21](https://github.com/Zhaokangbo/HongGuoRust/pull/21)、[#22](https://github.com/Zhaokangbo/HongGuoRust/pull/22)、[#23](https://github.com/Zhaokangbo/HongGuoRust/pull/23)、[#24](https://github.com/Zhaokangbo/HongGuoRust/pull/24)、[#25](https://github.com/Zhaokangbo/HongGuoRust/pull/25)、[#26](https://github.com/Zhaokangbo/HongGuoRust/pull/26)、[#27](https://github.com/Zhaokangbo/HongGuoRust/pull/27)、[#28](https://github.com/Zhaokangbo/HongGuoRust/pull/28)、[#29](https://github.com/Zhaokangbo/HongGuoRust/pull/29)、[#30](https://github.com/Zhaokangbo/HongGuoRust/pull/30)、[#31](https://github.com/Zhaokangbo/HongGuoRust/pull/31)、[#32](https://github.com/Zhaokangbo/HongGuoRust/pull/32)。


## UI/UX 改版规划（2026-11-22）

### 当前基线核对

- 本次重新读取 main 的 `AGENTS.md` 与本交接文档，并核对 GitHub PR、分支和 Actions；没有仅凭聊天记录推断状态。
- HG2B PR #32：Open / Draft / 未合并，base `main`，分支 `feat/hg2b-real-covers-and-episode-status`，HEAD `ac6614c94ef35095666054971a7c42c302982e2e`。
- 同一 HEAD 的 Windows M1 run `45018039636` 和 Windows M2 playback run `45018039660` 均 completed / success。当前未发现额外的 commit status 记录；以 workflow run 结果为 CI 依据。
- 用户已提供两次 Windows 实机截图：此前列表页显示多张不同真实海报；2026-11-22 详情页显示真实封面、简介、107 集列表和全局仅浏览保护。单集上游 `playable=false` 的独立状态显示、失败回退及剩余回归仍待验收。PR #32 不得擅自合并。

### 前端代码审查（仅检查，未修改代码）

- 技术栈为 Tauri 2 / Rust / libmpv2 + React 19 / TypeScript / Vite，样式集中在单一全局 `src/style.css`，当前没有引入专门的 UI 组件库或 utility CSS 框架。
- `src/main.tsx` 集中承载播放器状态、Tauri IPC、会话/选集队列、键盘快捷键、诊断面板和播放器控件；播放器主控件仍在单一横向 footer 中，公开 HTTPS 地址输入、诊断、轨道、倍速、音量等控件同时占据该区域。
- `src/catalog/CatalogView.tsx` 负责内容首页、来源切换、搜索、详情、剧集、历史和收藏等多个视图状态；导航仍是顶部横向 tabs。首页另由 `DiscoveryHome.tsx` 实现。
- `src/style.css` 采用全局元素选择器和按里程碑追加的页面样式；根布局沿用 66px header / 内容区 / 96px player footer。此几何关系与原生 libmpv 视频 HWND 的窗口布局有关，不能在视觉重构中随意改变。
- 截图和源码共同支持的主要 UX 问题：导航层级不够清晰；首页焦点区缺少强视觉媒体内容；海报网格在宽屏下卡片偏小；本地/历史/收藏使用偏工具型文字行；播放器控件密度过高、网络地址和诊断操作暴露在常驻主界面；空白区域与内容密度不均。
- 以上是静态代码与用户截图审查，不是完整交互/无障碍审计；尚未对焦点顺序、键盘可达性、缩放、窄窗口和实际 hover 状态做逐项测试。

### 已同意的改版范围与约束

- 采用现代桌面流媒体/媒体中心方向：深色沉浸式界面、真实海报优先、明确导航层级、统一卡片/表单/按钮/状态、适配 Windows 窗口尺寸。
- UI-1：设计 token、应用导航/页面框架、首页；UI-2：探索页/海报墙/详情；UI-3：播放器控件；UI-4：本地媒体/历史/收藏；UI-5：Windows 实机和全量回归。
- 用户明确要求由当前 Chat 直接接手 GitHub 工程开发，不切换 Work、不再要求复制提示词或先选择视觉概念。已采用此前双方确认的现代流媒体影院风格作为默认方向。
- UI 改版使用独立开发分支与 Draft PR；从 HG-2B 代码基线派生，PR 合并顺序必须遵守 #31 → #32 → UI PR。未经用户明确授权不得合并。
- 必须保持 Rust 播放内核、libmpv worker 所有权、Native session ID、Tauri IPC、Provider 数据契约、播放权限/仅浏览边界不变；不得把敏感媒体 URL、凭证或本地绝对路径暴露到 UI/历史。
- 原生视频 HWND 的尺寸/定位、全屏与 catalog 显示切换是硬约束。UI 可重新组织 HTML 控件，但不得假设 HTML overlay 能覆盖原生 HWND，也不得在无专项验证时改变播放器窗口几何。
- 玩家主窗口既有 66px header / 96px player footer 是硬约束；UI-1 只调整 catalog header 内部排版和 catalog 内容样式，不调整视频区域或播放器 footer。

### UI-1 当前实现与 GitHub 状态（2026-11-22 UTC）

- **开发分支**：`feat/ui-1-cinematic-discovery`，从 HG-2B PR #32 精确 HEAD `ac6614c94ef35095666054971a7c42c302982e2e` 创建。
- **Draft PR #33**：[feat(ui-1): cinematic desktop shell and discovery home](https://github.com/Zhaokangbo/HongGuoRust/pull/33)，Open / Draft / 未合并，base `main`，当前 HEAD `e3e26294feb92d5d130a9dddfa5f07aff1c20d80`。该 PR 是累积 stacked diff（77 个文件），其中 UI-1 新增/修改的实际文件为 `src/main.tsx`、`src/catalog/CatalogView.tsx`、`src/catalog/DiscoveryHome.tsx`、`src/style.css`、`docs/acceptance/ui-1.md`；前置 HG2A/HG2B 代码随堆叠基线一并出现在相对 main 的 PR diff 中。
- **UI-1 实际代码**：
  - `src/main.tsx`：应用品牌区加入简洁的 PERSONAL MEDIA CENTER 标识；保留现有 header 行高与播放器结构。
  - `src/catalog/DiscoveryHome.tsx`：首页文案调整为准确描述实际接入内容，不暗示虚构榜单或未授权内容。
  - `src/catalog/CatalogView.tsx`：详情页保留“返回列表”操作；非详情页隐藏重复的“返回播放器”按钮，统一使用应用 header 的返回播放入口。
  - `src/style.css`：新增 UI-1 设计 token 与现代影院风格覆盖；优化 catalog header、标题、tabs、来源工具、搜索框、焦点/悬停状态；真实海报卡片改为 2:3 竖版展示；强化首页 banner、真实内容焦点卡片、筛选和继续观看卡片；增加窄窗口布局与 focus-visible 状态。
  - `docs/acceptance/ui-1.md`：新增 10 项 Windows 实机验收清单。
- **最近提交**：`62b384861118d7450a0f829a1347683836a8600a`（品牌 header）、`2f2df6f9280648d8e1af35f0791c5302972b2293`（首页文案）、`1150de644993bf056726ecc246d9b45e1b007261`（视觉 token 与样式）、`916474d5ea25cbc8a009631f36d6818b53e20f66`（UI-1 验收文档）、`0d2d4104942bdc1240e9b68b2e4024a3d20caf4a`（去除重复返回入口）、`e3e26294feb92d5d130a9dddfa5f07aff1c20d80`（更新验收步骤，当前 HEAD）。
- **当前 CI**：精确 HEAD `916474d5ea25cbc8a009631f36d6818b53e20f66` 的 Windows M1 run `45026280804` 与 Windows M2 run `45026280683` 在首次查询时均为 `in_progress`。按用户协作约定，不持续轮询；等待用户通知“编译完成”后再检查最终结果。当前不得宣称 CI 通过，也没有可供用户验收的 UI-1 Windows artifact。
- **Windows 实机验收**：尚未开始；按 `docs/acceptance/ui-1.md` 逐项执行。UI-1 代码提交不代表 UI-1 视觉、键盘或播放器回归已通过。
- **合并纪律**：#31、#32、#33 均保持 Draft/未合并；#32 的 HG-2B 实机验收仍有未完成项。必须先完成各自验收、按 #31 → #32 → #33 顺序处理，并取得用户明确合并授权。

### 最新实机结果与下一步

- HG-2B 已确认的用户实机结果仍是：列表真实封面通过；详情真实封面、简介、107 集列表与全局仅浏览提示/禁播状态已由用户截图确认。单集上游 `playable=false` 独立状态显示、失败回退、搜索刷新竞态及 M7–HG2A 完整回归仍未全部验收。
- UI-1 当前只提交了前端展示层改动，没有修改 Rust/libmpv/IPC/Provider 或播放授权逻辑。
- 等待用户报告 Windows M1/M2 构建完成；收到后核对精确 HEAD `e3e26294feb92d5d130a9dddfa5f07aff1c20d80` 的两条 workflow。若通过，再提供匹配 SHA 的 debug artifact / 部署步骤；若失败，读取对应 job 日志修复。
- UI-1 实机验收重点：品牌 header/连接状态、全部导航 tab、首页搜索与来源/分类筛选、真实海报 2:3 展示、详情跳转、继续观看、窗口缩放/无横向溢出、键盘焦点，以及返回播放器后 HWND/96px footer/播放控制回归。
- UI-1 验收通过后，再开始 UI-2（探索列表、海报墙与详情页）；随后 UI-3 播放器控件、UI-4 本地媒体/历史/收藏、UI-5 全量 Windows 回归。
- 不因 UI 开发而误标 HG-2B 全量验收通过，也不得自动合并任何 Draft PR。
