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

> 继续开发我的 GitHub 仓库 `Zhaokangbo/HongGuoRust`。请先读取 main 上的 `AGENTS.md` 和 `docs/AI_DEVELOPMENT_HANDOFF.md`，再通过 GitHub 核实实际 PR、branch、HEAD、Windows Actions 和代码，不要把交接文档里的旧 SHA 当成当前事实。当前重点是 **UI-3 Netflix 风格首页重构**：Draft PR #36，分支 `feat/ui-3-netflix-home`，当前记录 HEAD `6cc2c45a9736c015f7f12cf109ea3894c970e937`；PR #36 是基于 UI-2 PR #34 的累积 stacked diff，PR #35 是同一 UI-3 分支的重复 PR，已关闭且未合并。UI-3 已实现启动默认进入内容首页、首次挂载隐藏原生视频 HWND、横向内容导航、真实海报 Hero、继续观看/类型横向轨道，以及黑/灰/红的全局视觉（播放器 66px header / 96px footer 几何不变）。当前 UI-3 的 Windows M1 run `45038072535` 和 Windows M2 playback run `45038072543` 在首次核对时均为 in_progress；不要持续轮询，等我说“编译完成”后再查一次最终结果并准备 Windows Debug/validation kit。UI-3 的 18 项 Windows 实机验收尚未完成；UI-2 PR #34 的实机验收也不要擅自标记通过。依赖顺序为 #31 → #32 → #33 → #34 → #36；所有 PR 保持 Draft/未合并，除非我明确授权。必须保留 Rust/libmpv worker、native session ID、Tauri IPC、Provider 数据契约、封面安全入口、媒体权限和 HWND 几何。由你直接维护 GitHub 代码、Draft PR、CI 修复和 Windows 调试包，我负责 Windows 实机验收。**每个开发阶段都要提交 GitHub 并及时更新 main 上的交接文档**，确保新会话可以无缝接手。

**交接文件更新纪律：**每个阶段至少维护：当前阶段、PR/branch/HEAD、构建状态+artifact ID、已经明确通过的验收条目、尚待验证的条目、遇到的问题/修复、下一步动作。**禁止把未运行/未通过状态写成通过。**优先将本文件更新到 main，确保下一段对话不会从旧功能分支获取过期状态；如果 main 提交权限不可用，才更新工作分支并明确告知用户路径。

## 9. 用户要求的 GitHub 同步纪律

- 每一项实质代码进展、用户实机验收结果、里程碑完成或开发路线调整，都应在该项完成后及时提交 GitHub；优先同步更新 `main` 上的 `docs/AI_DEVELOPMENT_HANDOFF.md`，必要时同时更新对应验收文档。
- 不能只在聊天中口头记录进度，也不能等会话结束才集中补记。每次提交后核实 commit SHA 和文件内容，再向用户报告链接。
- 仅讨论/分析且没有形成新结论时不制造空提交；没有代码改动时可只提交交接文档，不为刷新 CI 而改动代码。
- 会话切换前，必须提供当前 PR/branch/HEAD、已通过/未通过验收、最新 GitHub commit、下一步和一段可直接复制给新会话的 prompt。会话切换不代表自动合并 PR。

## 8. 参考入口

- 仓库主文档：`README.md`、`docs/roadmap.md`。
- 既有验收：`docs/acceptance/m10-c1.md`、`m10-c2.md`、`m11-a.md`、`m11-b.md`（**部分只在对应功能分支**）。
- 相关 PR：[#20](https://github.com/Zhaokangbo/HongGuoRust/pull/20)、[#21](https://github.com/Zhaokangbo/HongGuoRust/pull/21)、[#22](https://github.com/Zhaokangbo/HongGuoRust/pull/22)、[#23](https://github.com/Zhaokangbo/HongGuoRust/pull/23)、[#24](https://github.com/Zhaokangbo/HongGuoRust/pull/24)、[#25](https://github.com/Zhaokangbo/HongGuoRust/pull/25)、[#26](https://github.com/Zhaokangbo/HongGuoRust/pull/26)、[#27](https://github.com/Zhaokangbo/HongGuoRust/pull/27)、[#28](https://github.com/Zhaokangbo/HongGuoRust/pull/28)、[#29](https://github.com/Zhaokangbo/HongGuoRust/pull/29)、[#30](https://github.com/Zhaokangbo/HongGuoRust/pull/30)、[#31](https://github.com/Zhaokangbo/HongGuoRust/pull/31)、[#32](https://github.com/Zhaokangbo/HongGuoRust/pull/32)、[#33](https://github.com/Zhaokangbo/HongGuoRust/pull/33)、[#34](https://github.com/Zhaokangbo/HongGuoRust/pull/34)、[#35（已关闭重复项）](https://github.com/Zhaokangbo/HongGuoRust/pull/35)、[#36（当前 UI-3）](https://github.com/Zhaokangbo/HongGuoRust/pull/36)。


## UI/UX 改版规划（2026-11-22，持续更新）

### 当前 GitHub 基线（已重新核对）

- 技术栈：Tauri 2 / Rust / libmpv2 + React 19 / TypeScript / Vite；样式集中在全局 `src/style.css`，没有专门 UI 组件库。
- HG2B PR #32：Open / Draft / 未合并，base `main`，分支 `feat/hg2b-real-covers-and-episode-status`，HEAD `ac6614c94ef35095666054971a7c42c302982e2e`。
- UI-1 PR #33：Open / Draft / 未合并，base `main`，分支 `feat/ui-1-cinematic-discovery`，HEAD `e3e26294feb92d5d130a9dddfa5f07aff1c20d80`。
- UI-2 PR #34：Open / Draft / 未合并，base `main`，分支 `feat/ui-2-streaming-catalog`，最新 HEAD `78eee73d5a3d84e4f26d76ca23905439b860c600`。
- UI-1 精确 HEAD 的 Windows M1 run `45026280804` 与 Windows M2 playback run `45026280683` 均 completed / success。M2 Debug artifact：`HongGuoRust-M2-debug-x64`，artifact ID `14756091026`，SHA-256 `0a9cde3cb2dd1e9ced4cb906ac4341d7da57ef8d67d44ed2c3e54ecaea0ab38d`。
- 用户已明确确认 UI-1 的 Windows 实机验收全部通过（10 项）；但用户反馈视觉变化不明显，认为 UI-1 更接近主题换肤而非完整产品改版。UI-1 可作为稳定样式基础，不代表整体 UI/UX 改版已完成。
- PR #31、#32、#33、#34、#36 均保持 Open / Draft / 未合并；PR #35 是 UI-3 同分支重复 PR，已关闭且未合并。不得擅自合并。当前依赖顺序为 #31 → #32 → #33 → #34 → #36。

### UI-1 已完成并验收

- `src/main.tsx`：品牌 header 标识；保留 66px header 与播放器结构。
- `src/catalog/DiscoveryHome.tsx`：首页文案调整为准确描述已接入内容。
- `src/catalog/CatalogView.tsx`：详情返回入口与全局播放器入口去重。
- `src/style.css`：设计 token、深色配色、焦点/悬停、2:3 海报卡片、首页 banner、响应式样式。
- `docs/acceptance/ui-1.md`：10 项 Windows 实机验收清单，用户已全部确认通过。
- UI-1 没有修改 Rust、libmpv worker、native HWND、Tauri IPC、Provider 契约或播放权限。

### UI-2 已提交，CI 全绿，等待 Windows 实机验收

用户明确反馈 UI-1 视觉变化不明显，因此 UI-2 从“主题换肤”升级为实质性信息架构和内容编排改造。

- **分支**：`feat/ui-2-streaming-catalog`，从 UI-1 精确 HEAD `e3e26294feb92d5d130a9dddfa5f07aff1c20d80` 派生。
- **Draft PR #34**：[feat(ui-2): redesign streaming catalog and discovery experience](https://github.com/Zhaokangbo/HongGuoRust/pull/34)，Open / Draft / 未合并，base `main`，最新 HEAD `78eee73d5a3d84e4f26d76ca23905439b860c600`。这是累积 stacked diff；合并顺序 #31 → #32 → #33 → #34。
- **实际修改文件**：
  - `src/catalog/CatalogView.tsx`：把顶部横向 tabs 改为侧边栏分组导航（首页、探索全部、本地视频、最近播放、收藏、动态 Provider 来源）；探索页增加真实类型筛选、来源上下文和结果数；详情页改为真实封面背景 + 独立海报 + 作品信息 + 进度/续看/下一集 + 剧集卡片。保留原来源状态、搜索、分页、取消、manifest 二次确认、详情请求取消/竞态、播放权限与进度逻辑。
  - `src/catalog/DiscoveryHome.tsx`：首页采用 Provider 真实作品作为焦点，不标注虚构热度；继续观看下移至焦点之后；按实际 genre 元数据构建横向海报轨道；保留来源/类型筛选、搜索、刷新和加载更多。
  - `src/catalog/ContinueWatchingShelf.tsx`：Provider 历史卡片使用真实 coverUrl；本地 MP4 使用中性文件标识；续看进度与播放行为不变。
  - `src/style.css`：新增 sidebar、hero、横向海报轨道、探索海报墙、详情背景/海报、episode cards 和 480px 最小窗口适配样式。
  - `docs/acceptance/ui-2.md`：新增 16 项 Windows 实机验收清单。
- **首次 CI 失败与修复**：首次 M1/M2 因 `CatalogView.tsx(470,5): TS1005 '}' expected` 失败；已在 commit `06e6463bf070cb2a6501a88e9beff8076683c6b5` 补齐条件 JSX 的闭合括号。随后 commit `78eee73d5a3d84e4f26d76ca23905439b860c600` 避免首页焦点作品在类型轨道中重复出现。
- **最新精确 HEAD CI：全部通过**：
  - Windows M1 run `45030036582`：success。包含 `npm run build`、Rust fmt/test、`cargo tauri build --no-bundle`、live IPC 和正常进程退出验证。
  - Windows M2 playback run `45030036487`：success。包含 npm audit、`npm run build`、播放器 build/test、embedded-pixel 和 playing-window-close 验证。
  - M1 artifact：`HongGuoRust-M4-windows-x64`，ID `14757667246`，SHA-256 `8c760d7a04b2092bc8d1beb6cbba1680db93b1532e683200cbd8ca1faeb72bd8`。这是更适合用户实机验收的完整 Windows validation kit，包含 `hongguorust.exe`、`get-mpv.ps1`、`run-windows.md` 等文件；已通过 GitHub Connector 下载并本地复核 SHA-256。
  - M2 Debug artifact：`HongGuoRust-M2-debug-x64`，ID `14757477264`，SHA-256 `a52aa043b64789a2d382300a9cb81e490c5768e7fcd519460ca4abbb058f8474`。该 ZIP 仅包含 `hongguorust.exe`，不含运行时部署脚本；浏览器直接访问 Actions API 会返回 HTTP 401，必须通过认证下载。
  - **用户实机验收首选 M1 完整 validation kit**：解压后在该目录运行 `.`get-mpv.ps1 -Destination .`，再运行 `.`hongguorust.exe`。需要 Edge WebView2 Runtime；`libmpv-2.dll` 必须与 EXE 同目录。若系统缺少 Vulkan loader，按包内 `run-windows.md` 指引处理。
- **Windows 实机验收**：现在开始。只使用上述精确 HEAD 对应的 M2 Debug artifact，不得使用 UI-1 旧 artifact。按 `docs/acceptance/ui-2.md` 的 16 项检查；特别确认 sidebar、首页真实焦点和横向轨道、探索海报墙、详情页真实封面/剧集状态、播放续接、480×320 响应式、键盘操作及播放器 HWND/96px footer 没有回归。
- 用户尚未确认 UI-2 验收；PR #34 必须保持 Draft，不得合并。


### UI-3 Netflix 风格首页重构已提交，等待 CI 与 Windows 实机验收

用户明确选择了 Netflix 首页参考图，并要求不再以空播放器作为启动入口。UI-3 已从 UI-2 PR #34 的精确 HEAD 派生，按用户要求由 Chat 端直接维护 GitHub 代码，不依赖 Work 原型流程。

- **分支**：`feat/ui-3-netflix-home`。
- **主 Draft PR #36**：[feat(ui-3): Netflix-inspired browse-first desktop experience](https://github.com/Zhaokangbo/HongGuoRust/pull/36)，base `main`，Open / Draft / 未合并；当前 HEAD `6cc2c45a9736c015f7f12cf109ea3894c970e937`。
- **重复 PR 处理**：PR #35 原先以 UI-2 分支为 base，且与 #36 指向同一 UI-3 head；为避免两个活动 PR 混淆，#35 已关闭并标注由 #36 取代，未合并。今后只跟进 #36。
- **依赖链**：#31 → #32 → #33 → #34 → #36。#31–#34 仍为 Draft/未合并；不得合并或擅自调整依赖。
- **代码提交**：
  - `e282c4a77eac03e06da127e364120faf995f0a0c`：UI-3 首轮首页/导航/样式/验收文档实现。
  - `7de8574ddd4b89ada26bf5489ce38f59e9e412b8`：启动时尽早调用既有 `set_catalog_visible(true)`，避免先呈现空播放器或 native video HWND 遮挡内容首页。
  - `bf361a6e6a4649c5a8ff4bef3db1878f8104c819`：将首页搜索/来源/类型筛选移出 Hero 条件分支，保证无焦点作品时仍可恢复筛选。
  - `0f7343d02623983cad1a1abe150457c9d507eb7c`、`6cc2c45a9736c015f7f12cf109ea3894c970e937`：更新 M2/M1 Windows smoke tests，先从默认 Home 显式进入 Player，再检查 Idle IPC 和本地文件播放。
  - `6cc2c45a9736c015f7f12cf109ea3894c970e937`：M2 smoke test 进一步等待“播放器”按钮变为 enabled，避免 Rust bridge 尚未连接时提前点击。
- **实际改动**：
  - `src/main.tsx`：`catalogOpen` 默认 true；应用首次挂载时通过既有 Tauri IPC 隐藏 native video HWND；用户主动播放后仍走原有 Rust/libmpv 路径，返回播放器行为不变。
  - `src/catalog/CatalogView.tsx`：将 UI-2 侧边栏改为 Netflix 风格横向导航，保留首页、探索、我的视频、最近播放、收藏和动态 Provider 来源。
  - `src/catalog/DiscoveryHome.tsx`：Hero 优先；Hero 使用当前 Provider 真实作品的封面/标题/简介/类型/集数，点击只进入详情，不擅自播放；搜索、来源和真实类型筛选移至 Hero 之后；继续观看和真实类型海报轨道横向排列。
  - `src/catalog/ContinueWatchingShelf.tsx`：首页没有真实观看历史时不显示空的“继续观看”区块。
  - `src/style.css`：全局黑/炭灰/红色视觉系统；统一首页、探索、详情、本地媒体、历史、收藏与播放器控制的颜色；播放器 66px header / 96px footer 几何不变。
  - `docs/acceptance/ui-3.md`：18 项 Windows 实机验收清单。
- **数据与工程边界**：没有修改 Rust、libmpv worker、native session ID、Tauri IPC 契约、Provider 数据契约、媒体授权或 HWND 几何；封面仍走 `CoverImage` / `safeCoverUrl`；不生成虚假海报、评分、热度或可播放状态。
- **CI 首次核对**：HEAD `7de8574ddd4b89ada26bf5489ce38f59e9e412b8` 的 Windows M2 run `45036493718`、M1 run `45036493727` 首次核对时均为 `in_progress`，后续 HEAD 已前进，不再作为当前验收依据。
- **CI 失败定位与修复**：HEAD `bf361a6e6a4649c5a8ff4bef3db1878f8104c819` 的 M2 run `45036753289` 中，`npm run build`、`cargo fmt`、Rust tests、Debug EXE 构建和 artifact 上传均通过；失败发生在 `scripts/smoke-player.py` 等待 UIA `Idle`，因为 UI-3 默认 Home 隐藏播放器 footer。已在 `0f7343d` / `9e6e563` 更新 M2/M1 smoke scripts：先点击首页的“播放器”入口，再验证 Idle、文件选择和播放/关闭流程。该失败是旧测试脚本未适配新启动流程，不是编译失败。
- **最新精确 HEAD CI**：当前 HEAD `6cc2c45a9736c015f7f12cf109ea3894c970e937` 的 Windows M1 run `45038072535`、Windows M2 playback run `45038072543` 在最近一次核对时均为 `in_progress`；不得持续轮询或写成通过。此前 HEAD `9e6e563` 对应的 run 已被新提交取代，不作为最终验收依据。
- **Artifact**：旧 M2 失败 run `45036753289` 曾上传 Debug artifact `HongGuoRust-M2-debug-x64` ID `14760131534`（SHA-256 `f4e362a31eee558f55addf81f1bd89395ebcb028d72c7b9b7adfa779c9f2f7c2`）和诊断 artifact `windows-playback-diagnostics` ID `14760311041`。前者不作为最终验收包分发，因为该 run 的 M2 smoke 未通过。当前 HEAD `6cc2c45` 的正式 artifact 尚待 CI 完成；用户通知“编译完成”后再检查最终结果与 artifact ID。
- **Windows 实机验收**：尚未开始，18 项均待用户验收。UI-2 PR #34 的 16 项验收也不能因 UI-1 通过而推定通过。
- **下一步**：等待用户说“编译完成” → 一次性核对 #36 精确 HEAD 的 M1/M2 最终 CI → 若通过则准备 Windows 调试/validation kit 和 SHA-256 → 用户按 `docs/acceptance/ui-3.md` 验收 → 每次验收结论同步更新 main handoff。PR #36 保持 Draft，不合并。

### UI-2 设计目标与数据边界

1. **全局内容导航**：侧边栏分组为发现/探索、个人媒体（本地视频/观看历史/收藏）和动态 Provider 来源；保留原有入口及状态语义。
2. **沉浸式首页**：使用 Provider 实际返回的封面、标题、简介、类型和集数构建大幅焦点区；继续观看使用真实封面卡片；按真实类型分组横向轨道；不虚构热门榜单、评分或推荐分数。
3. **探索海报墙**：强化搜索/来源/分类/结果数量工具栏；桌面宽屏增加海报密度，使用真实 2:3 海报、信息层级和悬停操作；分页、加载更多、取消/刷新、来源选择保持原行为。
4. **作品详情页**：使用同一真实封面构建背景/海报布局；清楚展示来源、类型、简介、集数、观看进度、继续观看/下一集入口；剧集卡片展示真实可播放/仅浏览/上游不可播放状态。
5. **个人媒体区**：本轮保持原有本地/历史/收藏功能与数据行为，视觉统一可分阶段处理；不能为 UI 变更修改磁盘文件行为、播放列表语义或历史持久化。

### 硬约束

- 不修改 Rust 播放内核、libmpv worker 所有权、Native session ID、Tauri IPC、Provider 数据契约和播放授权边界，除非后续任务明确要求且单独验收。
- 不暴露签名媒体 URL、凭证、用户本地绝对路径；不制造海报、元数据、评分、热度、可播放状态。
- `safeCoverUrl` 和现有 CoverImage 处理仍是唯一封面展示入口；图片加载失败时保留真实失败/占位状态，不通过假图掩盖问题。
- 原生视频 HWND 的尺寸/定位、全屏与 catalog 显示切换是硬约束。Catalog 内容可重组为侧边栏 + 内部滚动布局，但不得假设 HTML overlay 能覆盖原生 HWND，也不得在无专项验证时改变播放器窗口几何。
- 正常播放器模式既有 66px header / 96px player footer 是硬约束；catalog 模式现有隐藏 footer 的行为必须保持。不得因 UI 重构改变正常播放器的 96px footer。
- 所有原有来源切换、搜索、分页、取消、导入/清空 manifest 二次确认、详情加载竞态、播放进度、不可播放集禁用状态必须保留。
- UI 改版继续使用独立分支与 Draft PR；UI-2 从 UI-1 精确 HEAD 派生，PR 目标仍为 `main`，作为累积 stacked diff。未经用户明确授权不得合并。
- CI 启动后不持续轮询，等待用户通知“编译完成”再检查。

### 后续路线

- UI-2：Draft PR #34；精确 HEAD CI 已全绿，Windows 实机验收仍待用户明确确认。
- UI-3：Draft PR #36；最新 HEAD `d85f131e6fa4e2c4c481cc6148bd9ecd7e9d4224` 的 Windows M1/M2 CI 已双绿，精确 HEAD Debug 验收包已准备；18 项 Windows 实机验收待用户完成。
- UI-4：本地媒体、历史、收藏进一步海报化/统一交互；需保留现有磁盘与观看记录语义。
- UI-5：详情页与播放器控制栏的深度体验优化、性能与 Windows 全量回归。
- HG2B 的独立验收状态不得因 UI-1 通过而误标完成；PR #31/#32/#33/#34/#36 均不得自动合并。

### 跨对话接力约定

新会话开始时先读取 main 分支 `AGENTS.md` 和本交接文档，再核对实际 PR HEAD、分支和精确 commit 的 Windows M1/M2 CI。当前 UI-3 接续分支为 `feat/ui-3-netflix-home`，Draft PR #36，最新 HEAD `d85f131e6fa4e2c4c481cc6148bd9ecd7e9d4224`；UI-2 基线为 PR #34 / `78eee73d5a3d84e4f26d76ca23905439b860c600`。PR #35 已关闭为重复项；最新 HEAD `d85f131e6fa4e2c4c481cc6148bd9ecd7e9d4224` 的 M1/M2 已 completed/success，Debug 验收包已准备；Windows 实机验收待用户完成。

### 2026-11-22 接手复核：UI-3 最新 HEAD 与当前验收门槛

- 本次按仓库 AGENTS.md 要求重新读取 main handoff，并通过 GitHub 连接器核对 PR、HEAD、文件差异与 CI 进度；没有合并任何 PR。
- 当前最新 UI 主线为 Draft PR #36：feat/ui-3-netflix-home，base main，最新 HEAD 84537517b383c43dd5bee5ea18981b6621a5f8b6。PR #36 open / draft / unmerged；前一 UI-3 重复 PR #35 closed / not merged。
- UI 依赖链仍为 #31 → #32 → #33 → #34 → #36。#31/#32/#33/#34 均 open / Draft / unmerged；#33 UI-1 HEAD e3e26294feb92d5d130a9dddfa5f07aff1c20d80；#34 UI-2 HEAD 78eee73d5a3d84e4f26d76ca23905439b860c600。不因后续 UI 分支已通过编译而跳过上游 Windows 实机验收或合并授权。
- UI-3 相对 UI-2 精确基线 78eee73d5a3d84e4f26d76ca23905439b860c600 有 8 个提交、8 个文件差异：src/main.tsx、src/catalog/DiscoveryHome.tsx、src/catalog/CatalogView.tsx、src/catalog/ContinueWatchingShelf.tsx、src/style.css、两份 Windows smoke 脚本及新增 docs/acceptance/ui-3.md。不改 Rust/libmpv worker、native session、HWND 几何、Tauri IPC 合约、Provider 合约或播放授权。
- UI-3 当前实现：启动默认进入内容首页并调用既有 set_catalog_visible(true) 隐藏原生视频 HWND；首页真实数据 Hero、搜索/来源/类型筛选、真实类型横向海报轨道、仅在存在观看记录时展示继续观看；导航、探索、详情、本地媒体、历史、收藏及播放器周边采用黑/炭灰/红色主题。18 项 Windows 验收见 docs/acceptance/ui-3.md。
- CI 当前状态（不作为最终通过）：PR #36 描述记录最新 HEAD 84537517b383c43dd5bee5ea18981b6621a5f8b6 的 Windows M1/M2 工作流正在运行；M2 前端构建、Rust 构建/测试和播放/窗口 smoke 已报告通过（含 20 次播放器窗口关闭循环）；M1 前端构建和 Rust 测试已报告通过，release 构建与 Home→Player IPC/正常退出 smoke 仍在运行。遵守不持续轮询约定：不要查询或宣称最终 CI 结果，直到用户通知“编译完成”。此前 HEAD 6cc2c45a9736c015f7f12cf109ea3894c970e937 的 run 45038072535 / 45038072543 不可用于替代最新 HEAD 验收。
- 当前 HEAD 的最终 M2 Debug artifact 尚未确认；此前旧失败 run 45036753289 的 artifact 14760131534 不得分发。待最新 HEAD M1/M2 同时 completed/success 后，再下载该 HEAD 的 M2 artifact、核对 ZIP/EXE SHA-256，并制作独立目录 Windows test kit；不要覆盖 %LOCALAPPDATA%\HongGuoRust。
- HG-2B PR #32 当前 HEAD ac6614c94ef35095666054971a7c42c302982e2e 的 Windows M1 45018039636 / M2 45018039660 已 completed/success。用户最新实机截图显示 9 张不同真实社区海报，只确认 HG-2B 验收第 3 项（搜索列表真实封面）通过；详情/剧集及其余 HG-2B 手动项仍须独立验收。
- 下一步：等待用户“编译完成”通知；然后仅核对 PR #36 最新 HEAD 的 M1/M2 最终结果一次。若双绿，准备并验证同 SHA Windows 调试包，用户按 docs/acceptance/ui-3.md 18 项实机验收。PR #36 继续 Draft，不合并；不把 CI 成功等同于实机通过。
### 2026-11-22 UI-3 编译完成复核与 Windows Debug 包准备

- 用户通知 UI-3 编译完成后，已按精确 PR HEAD 一次性核对：PR #36 旧 HEAD `84537517b383c43dd5bee5ea18981b6621a5f8b6` 的 Windows M2 run `45038401115` completed/success；Windows M1 run `45038401090` completed/failure。PR #36 仍 open / Draft / unmerged。
- 旧 HEAD M1 失败根因已由 job log 确认：`scripts/smoke-windows.ps1` 第 25 行及后续 player button selector 使用中文字面量“播放器”；Windows PowerShell 5.1 在 CI runner 对无 BOM UTF-8 源文件按本地 ANSI code page 解码，产生 `æ’­æ”¾å™¨`，随后 parser error。`npm ci`、Node tests、Vite build、Rust fmt、Rust tests、Windows no-bundle compile、validation staging 与 artifact upload 均成功；失败在应用启动前的 PowerShell smoke 脚本解析。
- 已直接修改 PR #36 分支 `scripts/smoke-windows.ps1` 并提交 `35a8ab4355718efa7fe87bfbf13d5b7c172db165`（blob `907820bd6fd7bae5f9e27a2d454143b4685b1719`）：把“播放器”标签改为 ASCII-only `[char[]](0x64AD, 0x653E, 0x5668)` 构造，并复用 `$playerLabel`；已在写入前断言脚本不含任何非 ASCII 字符。该提交只改 smoke 脚本，不改应用代码/二进制逻辑。
- 修复提交后的 PR #36 仍 open / Draft / unmerged，base main，最新 HEAD `35a8ab4355718efa7fe87bfbf13d5b7c172db165`。一次性确认新 CI 已启动：Windows M1 run `45040076708`、Windows M2 run `45040076678` 均 in_progress；M5 NSIS candidate skipped。不要轮询；等待用户再次通知“编译完成”后再核对精确 HEAD。
- 同一应用代码 HEAD `84537517` 的 M2 Debug artifact `HongGuoRust-M2-debug-x64` ID `14760343547`（ZIP SHA-256 `c1d888667308f3d49fcda2106bc80ca36feee20151fbcc461a72ffa86c25984a`）。其中 `hongguorust.exe` 为 PE32+ x86-64，动态导入 `libmpv-2.dll`；EXE SHA-256 `630f9f0f1f99687a37c284e353d7f8ad73406479a1e5e53a58a584f80796aabf`。M1 job 也上传了 `HongGuoRust-M4-windows-x64` artifact ID `14760344463`，仅复用其中经过固定 SHA 校验的运行时部署/网络诊断脚本，不用其中 release EXE 替代 M2 Debug EXE。
- 已准备临时 Windows Debug validation kit `HongGuoRust-UI3-Windows-Debug-84537517.zip`：包含 M2 Debug EXE、get-mpv.ps1、check-https-network.ps1、prepare-mpv-msvc.ps1、README 与 18 项 UI-3 验收表；ZIP SHA-256 `e99b545240aa1a5304d46b338792067757b5d2fa38d556273daafb421b81e18a`。README 明确标注为 provisional manual-validation build：应用代码对应 84537517，最新 35a8ab4 仅修复 PowerShell smoke 编码；M1 修复后的重跑尚待完成。不可称为 release sign-off。
- 当前可先让用户在独立目录解压 Debug kit 并开始 UI-3 视觉/功能手动验收，但必须说明 M1 CI smoke 重跑尚在运行；不要覆盖 `%LOCALAPPDATA%\\HongGuoRust`。首次运行需 `Set-ExecutionPolicy -Scope Process Bypass`，再执行 `.\get-mpv.ps1 -Destination .` 和 `.\hongguorust.exe`；要求 WebView2 Runtime，且 EXE 同目录需有经校验的 `libmpv-2.dll`。
- 下一步：用户再次通知 CI 编译完成后，只核对最新 HEAD `35a8ab4` 的 Windows M1/M2 一次。若双绿，更新本交接、将当前临时包标为 CI-green validation kit（仍非 release），由用户按 18 项清单实机验收；若 M1 仍失败，读取日志定位下一处 smoke 兼容问题。PR #36 保持 Draft，禁止自动合并。
### 2026-11-22 UI-3 PowerShell smoke 修复补充与调试包更新

- 复核 PR #36 最新 HEAD：`f21a18f30d7e557e22ebab00ab262becb4a7cc25`，分支 `feat/ui-3-netflix-home`，仍 open / Draft / unmerged。修复后的脚本 blob SHA `9e13e60852d2d5e746c61dd67d34bf7434c74ec9`；脚本已确认不含非 ASCII 源字符，并且 `$playerLabel` 声明与两处引用存在。
- 需要保留修复过程细节：提交 `35a8ab4` 将 PowerShell 文件中的中文字面量替换为 `$playerLabel`，但没有加入变量声明；后续提交 `f21a18f` 补上 `$playerLabel = -join ([char[]](0x64AD, 0x653E, 0x5668))`。最终 HEAD 的脚本状态已静态核验正确。不要把 `35a8ab4` 单独作为最终修复。
- 对最终 HEAD `f21a18f` 的 `fetch_commit_workflow_runs` 本次未返回运行记录；因此当前 CI 状态是 **尚未观察到新 run / 未验证**，不能推断通过或失败，也不要持续轮询。用户再次通知编译完成后再查一次精确 HEAD；若仍无 run，先核对 PR checks / workflow 触发条件，而不是重复查询。
- 已重新打包临时 Windows UI-3 Debug kit：`HongGuoRust-UI3-Windows-Debug-84537517.zip`，ZIP SHA-256 `151ca5f07856fcacaf3621773be29436e2f813e0b8691e59cb466ab52ef4def9`；EXE SHA-256 `630f9f0f1f99687a37c284e353d7f8ad73406479a1e5e53a58a584f80796aabf`。内含 `hongguorust.exe`、固定哈希校验的 `get-mpv.ps1`、可选 HTTPS 网络诊断、MSVC runtime 准备脚本、README、BUILD_INFO、SHA256SUMS 与 18 项 UI-3 实机验收表。
- Debug EXE 来自 M2 run `45038401115` / artifact `14760343547`（app commit `84537517`，M2 成功）；M1 run `45038401090` 在 smoke parser 失败前已完成应用构建、Rust 测试与 artifact staging。随后 `35a8ab4`、`f21a18f` 仅修改 smoke PowerShell，不改应用代码，因此该 EXE 仍适合**先行 UI 手动验收**，但 kit 必须标注 provisional，不能称作最新 HEAD CI-green build 或 release。
- 当前 kit 文件路径仅存在于生成它的对话附件：`/mnt/data/HongGuoRust-UI3-Windows-Debug-84537517.zip`；新对话不可猜测复用该 sandbox 路径，需通过本对话附件或重新下载 GitHub artifact 获取。
- 用户可在独立目录解压并开始 UI-3 实机验收；首次运行需 PowerShell 执行策略仅对当前进程放行，然后运行 `get-mpv.ps1` 下载并校验 `libmpv-2.dll`，再启动 EXE。需要 WebView2 Runtime；不要覆盖 `%LOCALAPPDATA%\\HongGuoRust`。
### 2026-11-22 UI-3 第二轮 Windows CI 失败诊断与修复提交

- 用户再次通知“编译好了”后，按 AGENTS.md 仅对当时精确 HEAD `f21a18f30d7e557e22ebab00ab262becb4a7cc25` 核对一次。PR #36 open / Draft / unmerged；M1 run `45040279927` failure，M2 run `45040279911` failure，M5 NSIS skipped。
- M1 的前端构建、Rust fmt、Rust 55 项测试、Windows no-bundle build、artifact staging/upload 均成功；失败仍在 `scripts/smoke-windows.ps1`，但已不是编码问题。Windows PowerShell 自动变量 `$HOME` 大小写不敏感且只读；脚本用 `$home` 赋值导致 `Cannot overwrite variable HOME because it is read-only or constant`。
- M2 的前端构建、npm audit（含 production）、Rust fmt 均通过；Rust 57 项测试中 55 passed / 2 failed。失败测试 `add_deduplicate_restart_remove_preserves_original` 与 `favorites_survive_restart_and_do_not_clear_playback_history` 的 `test_dir()` 生成了相同目录 `hgr-library-512-1795362619669997300`。并发测试共享 `library.json` 与 `library.json.pending`，导致 `MoveFileExW` 偶发 error 2，且一个测试读取到另一个测试的 `favorite.mp4` 路径而非中文文件名。这是测试夹具目录碰撞，不是生产本地媒体目录业务逻辑问题。
- 已直接提交两项修复到 `feat/ui-3-netflix-home`：`ede5ea321455876999e7bb337a5ce5d2861fdc8b`（PowerShell `$home` 改为 `$homeLabel`）和 `d85f131e6fa4e2c4c481cc6148bd9ecd7e9d4224`（Rust `test_dir()` 加入 `AtomicU64` + `Ordering::Relaxed` 进程内序号，避免并行测试目录名碰撞）。通过重新读取分支文件确认两个修改均在最新 HEAD，且 PowerShell 脚本仍无非 ASCII 源字符。
- 当前 PR #36 HEAD 为 `d85f131e6fa4e2c4c481cc6148bd9ecd7e9d4224`，base main，仍 Draft / open / unmerged。两次修复提交已推送；遵守不轮询约定，不查询该 HEAD 的新 M1/M2 CI，直到用户再次通知“编译好了”。
- 不要把之前 `84537517` / `f21a18f` 的旧 M2 artifact 当作最新绿色包：新 Rust 测试夹具修复需要由精确 HEAD 的 CI 重新验证；在新 HEAD 的 M1/M2 双绿前不发布新的 Windows Debug kit。
- 下一步：用户在 Windows 10/11 x64 上解压 `HongGuoRust-UI3-Windows-Debug-d85f131.zip` 到独立目录，执行 README 中的 `get-mpv.ps1` 和 EXE 启动步骤，并按 `ui-3-acceptance.md` 逐项完成 18 项实机验收。回报通过/失败项、截图和日志后再继续修复。PR #36 保持 Draft，禁止合并。
### 2026-11-22 UI-3 精确 HEAD CI 双绿与 Windows Debug 验收包交付

- 用户通知“已经编译完成”后，重新读取 main 的 `AGENTS.md` 与本交接文档，并通过 GitHub 连接器核对 PR #36。PR 仍为 `feat/ui-3-netflix-home` → `main`，open / Draft / unmerged；精确 HEAD 为 `d85f131e6fa4e2c4c481cc6148bd9ecd7e9d4224`。
- 精确 HEAD 的 Windows M1 run `45041004835`（run number 306）completed / success；Windows M2 playback run `45041004830`（run number 277）completed / success。M5 NSIS candidate run `45041004863` skipped；本次交付为 Debug 手动验收包，不是 M5 安装器/正式发布。没有合并 PR。
- M2 Debug artifact：`HongGuoRust-M2-debug-x64`，artifact ID `14761752581`，来源 run `45041004830`，大小 4,536,659 bytes，GitHub digest / 下载 ZIP SHA-256 均为 `f311bb74f8201dc00696f1db0a27e111e98b7b1d7c582485f08b5926ae9e74f4`。其中 `hongguorust.exe` 为 PE32+ x86-64，导入 `libmpv-2.dll`，EXE SHA-256 `bbd92a782452cf8381fd3dd61d8917ff8def695159183da29dfda1843c41c724`。
- 同一 HEAD 的 M1 validation artifact：ID `14762135825`，GitHub digest / 下载 ZIP SHA-256 `867a3f04968335fc3df43af1e0c09909fff18f21ed4c3f65740e618e9ae9aab5`。只从此包取 `get-mpv.ps1`、`check-https-network.ps1`、`prepare-mpv-msvc.ps1`，未使用 M1 包中的 EXE；Debug EXE 确认来自 M2 artifact。
- 已制作独立目录验收包 `HongGuoRust-UI3-Windows-Debug-d85f131.zip`，包含 M2 Debug EXE、三个 M1 helper scripts、Windows 运行说明、BUILD_INFO、SHA256SUMS 和 18 项 `ui-3-acceptance.md`。验收包 SHA-256：`5b77bf934a99b670d72e94b7d9814c6407a46ad765e46589f2838ea7c59368b7`。包内 `SHA256SUMS.txt` 全部校验通过，ZIP integrity test 通过。当前会话容器路径 `/mnt/data/HongGuoRust-UI3-Windows-Debug-d85f131.zip`；新会话不可假设 sandbox 路径仍存在，应重新从 GitHub artifact 下载或让用户提供本次会话附件。
- Windows 首次启动：解压到独立目录，在 PowerShell 执行 `Set-ExecutionPolicy -Scope Process Bypass`、` .\\get-mpv.ps1 -Destination .`、` .\\hongguorust.exe`（命令中不需要前导空格）。`get-mpv.ps1` 固定下载源并校验 mpv archive SHA-256 与 `libmpv-2.dll` SHA-256。需要 Windows 10/11 x64 和 WebView2 Runtime。网络诊断可选执行 ` .\\check-https-network.ps1`。不要覆盖/删除 `%LOCALAPPDATA%\\HongGuoRust`。
- 18 项验收重点覆盖：默认 Home、浏览态 HWND 隐藏、导航/Provider、真实 Hero/封面、搜索和筛选竞态、Continue Watching、横向分类货架、hover/键盘可访问性、Community browse-only、允许播放与返回浏览、本地 MP4/收藏/历史/续播、480×320 响应式、播放器控制回归和重启数据持久性。
- 当前状态：精确 HEAD Windows CI 双绿；Windows 实机验收仍 pending，不能因 CI 双绿标记 UI-3 完成或允许合并。下一步由用户运行本包并反馈 18 项验收结果；根据失败项继续修复。PR #36 继续 Draft / unmerged，未经用户明确授权不合并。