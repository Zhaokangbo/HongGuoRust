# HongGuoRust — AI 多对话开发交接与持续维护记录

> **长期事实来源（跨对话使用）**。请每次接手开发时，先读取本文件，再核对 GitHub 上的 PR、分支 HEAD、CI 与相关源码。**不要仅凭本文件或聊天记忆假定 CI/合并已完成**。
>
> 更新基准：2026-11-22 UTC。维护方式：该文件直接保存在 GitHub `main` 的 `docs/` 下，独立于尚未合并的功能 PR，因此新对话可以始终从稳定 URL 访问。开发阶段变化后应及时更新此文件。

## 0. HG-2B Windows 新版封面请求失败：已进入 WebView2 网络层诊断（2026-11-22 UTC）

- **PR #32 版本/CI 不变**：Draft/open/未合并；分支 `feat/hg2b-real-covers-and-episode-status` HEAD `7b8735de8cc67dc00d3e5cd68b44cbd39f0e44ac`。同 SHA 的 Windows M1 #45011433042 与 M2 #45011433033 均 completed/success。M2 artifact #14749403837 已校验并打包为 `HongGuoRust-HG2B-7b8735d-Windows-test-kit.zip`。未合并，不允许将 CI success 当作实机验收。
- **用户已经确认本机 checkout 与 EXE**：`git rev-parse HEAD` 输出与上述 SHA 完全一致；启动新版 `%LOCALAPPDATA%\\HongGuoRust-HG2B-7b8735d-Test\\hongguorust.exe` 与同 SHA sidecar 成功，Node 正常输出多轮 `HG2B_COVER_DIAG`，例 `results=8 direct=8 batch=0 missing=0 lookup=not-needed` 以及 `results=6 direct=0 batch=6 missing=0 lookup=ok`。
- **最新 Windows 实机失败证据（截图 2026-11-22）**：8 张社区作品卡片均显示前端标签“封面图片加载失败”。该文案来自 `CoverImage.tsx` 的 `img onError`，不是 8 秒 slow timer；因此 WebView2 的图片资源请求或解码确实失败。SDK 元数据 `missing=0` 只表示网关有符合 URL 语法校验的字符串，不代表 CDN HTTP 200、图片 MIME、网络代理、浏览器下载或图片解码成功。
- **当前代码核对**：`src-tauri/tauri.conf.json` CSP 为 `img-src 'self' data: https:`，语法上已允许 HTTPS 远程图片；`CoverImage.tsx` 使用原始安全 HTTPS URL、`referrerPolicy="no-referrer"`、`onError`。尚未获得 DevTools 的实际 Network 错误码，因此不能武断认定是 CSP、代理、Referer、CDN 403/429/503、DNS、TLS 或 MIME/ORB。用户之前代理为 HTTP(S)/ALL_PROXY + HKCU `127.0.0.1:7897`，WinHTTP direct；仍不构成根因证明。
- **本轮远程调试尝试尚未成功附加到 WebView2**：用户执行 `Invoke-RestMethod http://127.0.0.1:9222/json | Select-Object title,type,devtoolsFrontendUrl | Format-List` 仅得到空字段；Edge 打开 `http://127.0.0.1:9222` 是白页，Edge 自己的 DevTools Console 只显示该空白页面的 `/favicon.ico 404`。这不是应用封面请求；之前“打开 9222 根地址并按 F12”的指导不正确，已向用户纠正。用户应用内刷新时 Node 侧仍新增多行 `HG2B_COVER_DIAG`，表明刷新触发了 SDK 搜索，但当前 Edge Network 面板没有附加到 Tauri WebView2。
- **WebView2 CDP target 已成功确认（用户 2026-11-22 最新回报）**：`/json/version` HTTP 200，Browser `Edg/157.0.4341.60`；`/json/list` 与 `/json` 均 HTTP 200，返回唯一 target `title=HongGuoRust`、`type=page`、`url=http://tauri.localhost/`、page id `0975CDE5A2CA575D412A3CC4CA398B46` 和 `devtoolsFrontendUrl` 指向 Edge inspector。9222 listener PID 34360 为 `msedgewebview2`，监听 `127.0.0.1:9222`。**远程调试已打开且真实 Tauri page target 已枚举；还未确认 DevTools frontend 已 attach**。此前 Edge 打开 9222 根地址看到白页属预期，不是应用页面；`favicon.ico 404` 无关。
- **CDP target 存在，但 `devtoolsFrontendUrl` 直接启动会跳转 Bing**：用户执行 `Start-Process $target.devtoolsFrontendUrl` 后 Edge 将 `aka.ms/docs-landing-page/serve_rev/.../inspector.html` 重定向到 `cn.bing.com`，没有打开 DevTools；这属于微软 Edge DevTools 前端短链接/跳转问题，不代表 CDP 端口或 WebView2 target 故障。不要继续重试这个短链接，也不要在 `http://127.0.0.1:9222` 根页按 F12。
- **下一步正确附加方式**：保留应用和 sidecar，直接在 Edge 地址栏打开 `edge://inspect/#devices`。在 `Discover network targets` 旁点 `Configure...`，添加 `localhost:9222`（若已存在则保留），关闭配置弹窗；在 `Remote Target`/目标列表中找到 `HongGuoRust`（`http://tauri.localhost/`），点击该条目旁的 **inspect** 链接。确认新 DevTools 窗口顶部对应 `HongGuoRust`，再选 Network → Img，勾选 Preserve log/Disable cache（可选），回应用内点击“刷新内容”；选择失败图片记录 Status 或 `net::ERR_...`、Response Headers 的 Content-Type、Console CSP/解码错误。不要复制完整图片 URL/query、Cookie、Authorization、token 或签名参数。若 `edge://inspect` 不显示 target，先确认 `localhost:9222` 已配置，再复核 `/json/list` 仍返回该 page target；不需要重启或重新编译。
- **诊断后修复决策**：CSP 报错→最小化调整 img-src 的 HTTPS/loopback 限制；`ERR_TUNNEL_CONNECTION_FAILED`/代理错误→确认 WebView2 代理与系统代理，不改全局注册表；HTTP 403/404/429/5xx→按 CDN referer/签名/限流/上游状态定位，禁止自动重试；TLS/DNS/MIME/ORB→对应修正。只有取得证据后才决定是否实现受限的 loopback image relay，避免先引入 SSRF、代理绕过或无意记录签名 URL。
- **Node 未跟踪文件**：用户工作区有 `?? scripts/hg2a-sdk/package-lock.json`，远程 PR HEAD 不含该文件。不要删、reset 或 stash；它是用户本地未跟踪依赖锁文件。当前用户已正确 checkout HEAD。
- **下一步**：先获取 WebView2 失败码，后续若修改代码再触发 CI；遵循用户规则，不持续轮询。PR #32 保持 Draft；不合并、不绕过 DRM/会员/版权访问控制。

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
| HG-2B 真实封面及分集状态 | [Draft PR #32](https://github.com/Zhaokangbo/HongGuoRust/pull/32), `feat/hg2b-real-covers-and-episode-status`, **HEAD `7b8735de8cc67dc00d3e5cd68b44cbd39f0e44ac`** | CI M1/M2 全绿；用户 Windows 已运行精确 EXE/sidecar，但 8 张卡片均触发 `img onError` 显示“封面图片加载失败”；Node `direct/batch` 有数据。CSP 当前允许 HTTPS，**根因未定**，需 DevTools Network/Console 错误码 | 先采集 WebView2 状态码/MIME/CSP/ERR 证据，再定向修复；不盲加代理/重试、不合并 |
| HG-1 原并行前端/禁播方案 | [PR #30](https://github.com/Zhaokangbo/HongGuoRust/pull/30), `feat/hg1-hongguo-provider-gate`, SHA `9e9f68b9e5f30483fc17bc043dc7666682cc4575` | **已关闭（closed, NOT merged）**：其独特的 React tab、Provider capability 及 native 禁播能力已移植到 #29，避免重复维护；没有任何 PR 合并到 main | 后续以 **#29 为唯一 HG-1 活跃主线**，不要尝试独立合并 #30 |

**验收口径**：用户已明确确认 M11-A 10/10、M11-B 12/12、M11-C1 12/12、M11-C2 12/12、M12-A 10/10、M12-B 10/10、M12-C 10/10、HG-1 10/10 Windows 实机通过。HG-2A 截图只证明真实目录搜索元数据有效，未完整验收或验证真实视频播放。**HG-2B 旧 `7eb8253c5697ccd52b51bac37f4f4dc9a18dcba8` 的 M1/M2 CI 虽全绿，但用户提供的两张截图表明真实封面实机验收 FAILED：搜索“重生”6 条和清空搜索 8 条都只有渐变占位**。现针对 upstream SDK ParseSearchTab 漏 CoverURL 在新 `779dbdb10b5addf233993cb8e66b6b43d1c68d1d` 补有界详情封面回填，实际 CDN 图片修复尚未验收。#20/#21 部分验收未确认。

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

1. **当前阶段 HG-2B 实机封面缺失排障**：[Draft PR #32](https://github.com/Zhaokangbo/HongGuoRust/pull/32)，`feat/hg2b-real-covers-and-episode-status`，最新 SHA `779dbdb10b5addf233993cb8e66b6b43d1c68d1d`。用户已交付实机截图，旧 `7eb8253c5697ccd52b51bac37f4f4dc9a18dcba8` 的真实剧名/集数搜索可用但海报 6/8 张均为渐变占位。查明上游 openhongguo `ParseSearchTab` 在部分主搜索分支没有设置 `CoverURL`，Node 现以 SDK 自带 `getSeriesBatch` 最多补 10 张封面，失败保留原结果；前端标明缺失/加载失败；详细见第 0 节。旧 Draft #20–#29/#31/#32 均未合并，重复 #30 已关闭，禁止擅自合并。
2. **核心改动已提交**：`scripts/hg2a-sdk/server.mjs` 现在从开源 `openhongguo@0.4.0` 返回的 `cover_url` 转发安全合法的 `coverUrl`，保留签名查询参数；`src-tauri/src/catalog/mod.rs` 的 `Series.cover_url: Option<String>`，`src-tauri/src/catalog/hgcommunity.rs` 独立 URL 校验；`src/catalog/coverUrl.ts` 和 `CoverImage.tsx` 懒加载，错误回退旧渐变封面；页面卡片、首页焦点/普通卡片和作品详情使用真海报。Tauri CSP 仅放行 `img-src https:`，`connect-src` 不变，图片发往 CDN 仍有 IP/远端可观测性问题；字符串校验不是完整 SSRF/DNS/redirect 安全方案。
3. **分集状态**：Node SDK 保留 `playable=false` 条目而不删掉，Rust `Episode.playable: bool` 各现有 Provider 默认 true，HG community 从上游真实字段读取；界面保留禁播集数并提示上游不可播放，native resolve 再次强制检验；未明确允许媒体主机时社区源详情只能浏览、选集按钮不可点。真实元数据可见不等于 DRM/会员/播放授权，禁止绕过。
4. **测试与文档**：`tests/hg2b-cover.test.mjs` 覆盖 Node 前端双向 URL 拒绝 HTTP、内网 IP、带密码 URL、非标准端口、控制字符、过长 URL，合法 HTTPS 带签名 query 保留；`tests/hg2a-community-gateway.test.mjs` 模拟真实 `cover_url` 和不可播放选集，Node 测试链加入 `npm run build`。Rust 增加 `valid_cover_url` 单测。[HG-2B Windows 10 项实机验收](https://github.com/Zhaokangbo/HongGuoRust/blob/feat/hg2b-real-covers-and-episode-status/docs/acceptance/hg2b.md) 要求看到实际不同封面/详情、失败回退、`playable=false` 禁播及 M7–HG2A 回归。
5. **HG-2B 新封面补全修复 CI 进行中**：旧 `7eb8253c5697ccd52b51bac37f4f4dc9a18dcba8` [M1 #45008194544](https://github.com/Zhaokangbo/HongGuoRust/actions/runs/45008194544)/[M2 #45008194542](https://github.com/Zhaokangbo/HongGuoRust/actions/runs/45008194542) success，旧 Artifact `14748394183` 的 ZIP SHA256 `9c939183ea3089bee2cdf8fd088e3c8624f2f8245fcc3bc62267950eb4a57375` 和 EXE SHA256 `a39f3652d4f250bb1c9ed822198b4d7ee6a0feaabd77a4725095849c53728707` **不适用于本次新修复**。用户实机搜索“重生”6 条和默认 8 条均无封面，已在新 `779dbdb10b5addf233993cb8e66b6b43d1c68d1d` 增加 Node `getSeriesBatch` 缺失封面补全、`HG2B_COVER_DIAG` 无 URL 计数日志、React 区分「SDK 未获取到封面」和「封面图片加载失败」、离线 batch/限流/ID 边界测试，验收文档已更新。新 [Windows M1 #45010167452](https://github.com/Zhaokangbo/HongGuoRust/actions/runs/45010167452) / [M2 #45010167460](https://github.com/Zhaokangbo/HongGuoRust/actions/runs/45010167460) 首次 in_progress，**还未完成/核实成功**。不持续轮询；等用户确认本轮编译完成后检查准确 HEAD，失败修复，全绿才提供新 SHA M2 ZIP、双 SHA256、隔离 Windows 部署脚本。**复测必须更新用户 Windows 本地代码仓库中的 `scripts/hg2a-sdk/server.mjs`，不仅更换 EXE**，再运行社区 SDK，检查终端 `HG2B_COVER_DIAG` 和卡片状态，避免输出原始签名 URL。受保护媒体/分发许可仍未获得授权，不处理解密。
6. **下一步方向**：HG-2B 验收通过后再做 HG-2C/3 合法媒体能力与格式兼容调查，实际判断 openhongguo SDK 各作品源 `protected`、HLS/DASH、直链 MP4 和请求头需求；无已获授权的 DRM/付费资源仍不可解密、跳过权限或作为公开视频分发。Issue #7 第三方 DLL/SDK/版权发布许可独立阻断。仍沿用显式 Node SDK opt-in：`npm.cmd install --prefix .\\scripts\\hg2a-sdk --ignore-scripts`，`powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\\scripts\\start-hg2a-community.ps1 -ExePath ...`。

## 6. 构建/交付标准与协作手册

- Windows 工作流：`Windows M1` 和 `Windows M2 playback`，同一最新 HEAD 的 `status=completed` + `conclusion=success` 才算 CI 通过。GitHub Actions 页面：`https://github.com/Zhaokangbo/HongGuoRust/actions`。
- M2 调试产物通常名 `HongGuoRust-M2-debug-x64`，ZIP 内是 `hongguorust.exe`；`libmpv-2.dll`、`vulkan-1.dll` 需复用用户旧版已验证的私有 runtime，部署到新测试目录（例：`%LOCALAPPDATA%\HongGuoRust-M11B-Test`）。
- **不要覆盖、删除** `%LOCALAPPDATA%\HongGuoRust` 数据目录（`library.json`、`provider-progress.json`）。第三方二进制发布合规议题为 Issue #7；未经处理不得做公开发行。
- 实际 Windows 验收由用户报告，不能用 Github runner 的 CLI probe 替代。
- 修改源文件时：Windows 路径大小写不敏感，**不要创建仅大小写不同的 TypeScript 模块名**；此前 `recentActivity.ts` 与 `RecentActivity.tsx` 导致 CI 失败，后改成 `ContinueWatchingShelf.tsx`。
- git SHA、artifact ID、运行状态等是会变化的；永远从 GitHub 校验，而非直接复述文件中可能过期的快照。

## 7. 跨对话接力约定

**新对话可直接发送下面这段话：**

> 继续开发我的 GitHub 仓库 `Zhaokangbo/HongGuoRust`。请先用 GitHub 连接器读取 main 上的 `docs/AI_DEVELOPMENT_HANDOFF.md`，再核对当前所有 Draft PR 的实际 HEAD、验收门槛、相关源文件和 GitHub Actions。遵循文档中的协作方式：直接修改代码、维护 Draft PR、我负责 Windows 真机验收、我说 CI 编译完成后再查看结果、未明确授权不要合并。请从文档的“当前下一步操作”继续，并在每阶段结束时同步更新此交接文件。

**交接文件更新纪律：**每个阶段至少维护：当前阶段、PR/branch/HEAD、构建状态+artifact ID、已经明确通过的验收条目、尚待验证的条目、遇到的问题/修复、下一步动作。**禁止把未运行/未通过状态写成通过。**优先将本文件更新到 main，确保下一段对话不会从旧功能分支获取过期状态；如果 main 提交权限不可用，才更新工作分支并明确告知用户路径。

## 8. 参考入口

- 仓库主文档：`README.md`、`docs/roadmap.md`。
- 既有验收：`docs/acceptance/m10-c1.md`、`m10-c2.md`、`m11-a.md`、`m11-b.md`（**部分只在对应功能分支**）。
- 相关 PR：[#20](https://github.com/Zhaokangbo/HongGuoRust/pull/20)、[#21](https://github.com/Zhaokangbo/HongGuoRust/pull/21)、[#22](https://github.com/Zhaokangbo/HongGuoRust/pull/22)、[#23](https://github.com/Zhaokangbo/HongGuoRust/pull/23)、[#24](https://github.com/Zhaokangbo/HongGuoRust/pull/24)、[#25](https://github.com/Zhaokangbo/HongGuoRust/pull/25)、[#26](https://github.com/Zhaokangbo/HongGuoRust/pull/26)、[#27](https://github.com/Zhaokangbo/HongGuoRust/pull/27)、[#28](https://github.com/Zhaokangbo/HongGuoRust/pull/28)、[#29](https://github.com/Zhaokangbo/HongGuoRust/pull/29)、[#30](https://github.com/Zhaokangbo/HongGuoRust/pull/30)、[#31](https://github.com/Zhaokangbo/HongGuoRust/pull/31)、[#32](https://github.com/Zhaokangbo/HongGuoRust/pull/32)。
