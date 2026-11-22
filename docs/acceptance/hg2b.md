# HG-2B：真实短剧封面、详情与选集状态验收

## 阶段目标与基线

本阶段从 HG-2A Draft PR #31 的 `50ff5f07018e84bae93def02bea54543b840d7e7` 派生。Windows 用户已在 HG-2A 实验源中**实际搜索到剧名与集数**，并明确指出所有封面都是 `artwork-studio` 渐变占位。HG-2A 已确认“搜索元数据返回”，但尚**未**确认完整 Windows 10/10、远端视频是否实际可播放或具有独立内容授权。

HG-2B 专注于**真实数据展示**，不尝试绕过 DRM、签名、会员权限或任何访问限制；不把真实搜索等同于视频可播放。原 HG-1 正式 `hongguo` 待授权源仍保留空结果/禁播；唯一开放数据的路径是用户明确启动的非官方 `hgcommunity` 实验源。所有 PR 依然 Draft/未合并。

## 已实现的数据链

- `openhongguo` Node SDK 原生作品元数据含 `cover_url`。`scripts/hg2a-sdk/server.mjs` 使用 `publicCoverUrl` 校验并转发到本机 HTTPS 字段 `coverUrl`，按原样保留 CDN 签名查询参数，不存储媒体密钥，不将作品改写成模拟数据。
- Rust `Series` 新增可选 `cover_url: Option<String>`（序列化为 camelCase `coverUrl`，没有时省略）；所有演示、本地、导入媒体默认 None；`HgCommunityProvider` 限定非内部 HTTPS 封面域名并保留不存在/非法地址时的渐变回退。
- React `CoverImage` 懒加载实际封面，`onError` 回退原来的风格渐变；应用在搜索结果卡片、作品详情、首页焦点和首页普通卡片。CSS `object-fit: cover`、对比渐变及文字叠加；组件用空 alt 表示装饰性图片、`referrerPolicy=no-referrer` 不发送应用页面地址。
- Tauri CSP 仅在 `img-src` 增加 `https:`，**没有**开放任意 `connect-src`、HTTP 图像、WebView JS API 或任意原生媒体地址。远端 CDN 会在实验源用户加载封面时获得其客户端网络地址；不应误解为私密/匿名访问。URL 的语法验证并不能充分防范 DNS 重绑定、重定向及远端跟踪；公开发行或生产授权源需补精确允许域名及下载代理/缓存策略。
- Node 现在保留 SDK 返回的全部已知分集，包括 `playable=false`，在页面标为“上游标记不可播放”。Rust `Episode` 增加 `playable: bool`（其他内置来源默认 true），并在 `HgCommunityProvider::resolve_episode` 中再次校验真值；即使绕过前端禁用按钮也不能解析已知不可播放分集。
- `hgcommunity` 默认未配置允许媒体域名时页面提示“仅浏览、不开放播放”，防止出现“能点任意集数却不断报错”的误导。即使 `playable=true`，也只是 SDK 可选状态，不代表用户/客户端具备播放授权、更不代表流没有保护。
- `tests/hg2a-community-gateway.test.mjs` 更新为 Mock SDK 返回真实封面字段及可播放状态，验证不可播放分集详情仍列出但 /play 路由拒绝；`tests/hg2b-cover.test.mjs` 双向验证 Node 和前端 URL 拒绝异常协议、私网 IP、用户信息、未允许端口、fragment、超长/控制字符，允许合法 HTTPS signed cover 并原样保留查询字符串。测试只使用虚构域名，不连接外部红果服务。

## HG-2B 实机封面缺失复测（追加）

用户首次部署 HG-2B 后的两张 Windows 实机截图显示：在「社区接口实验」搜索“重生”返回 6 部作品、清空关键词返回 8 部作品，**全部仍为蓝绿色 CSS 渐变，占位封面验收未通过**。这不是播放状态异常，真实作品标题/集数检索仍可用。

已针对上游 `openhongguo` SDK 源码核查：其 `internal/protocol/parse.go` 中 `ParseSearchTab` 在直接 `video_detail` 搜索结果分支构造 `SeriesSummary` 时仅赋值 ID、Title、Description、EpisodeCount，遗漏 `CoverURL`；SDK `GetSeriesBatch` 则会返回 `{series:{id,cover_url,...},episodes:[...]}`，可取得详情里的封面。

本分支增加 `enrichCatalogCovers`：仅针对原搜索结果中缺失/无效的封面，从最多 10 个已有作品 ID 发起**一次** SDK 原生 `getSeriesBatch` 只读元数据查询；若 SDK 限流/拒绝/返回无封面，保留搜索原结果，不用合成封面、不能隐蔽重复尝试。Node 只打印不含作品名称/URL/签名/Cookie 的数字统计，例如：

```text
HG2B_COVER_DIAG results=6 direct=0 batch=5 missing=1 lookup=ok
```

UI 对实验来源分别显示「SDK 未获取到封面」（数据无 URL 或被过滤）与「封面图片加载失败」（已拿到有效 HTTPS URL、WebView/CDN 加载失败）；普通 Demo/本地不显示这些诊断字样。此方式能区分 SDK 数据缺失与 Tauri CSP / CDN 网络问题。

**复测需要精确 SHA 的 Windows M1/M2 再次完成 success，重新部署对应新 EXE 并更新仓库中的 Node SDK bridge 脚本**。仅更换 EXE 不更新 `scripts/hg2a-sdk/server.mjs` 会使旧桥接继续返回无封面。复测搜索“重生”后查看带前缀 `HG2B_COVER_DIAG` 的终端统计，同时查看列表封面。诊断只需要分享该统计行和截图，禁止复制完整带签名的 CDN URL、会话 token 或请求 Header。

## HG-2B 封面加载缓慢：第二轮 Windows 实测与前端补丁

用户在 2026-11-22 Windows 实测中多次刷新，发现**偶尔只有一张封面显示且速度很慢**，原 HG-2B Debug EXE 搭配最新 Node 网关的无敏感统计为：

```text
HG2B_COVER_DIAG results=8 direct=8 batch=0 missing=0 lookup=not-needed
HG2B_COVER_DIAG results=6 direct=0 batch=6 missing=0 lookup=ok
```

其他样本同样 `missing=0`。**能够确认 SDK→Node 已有符合安全 URL 校验的封面字段，但不能单凭该计数判断图片 CDN 的 HTTP 状态、内容类型、网络代理或是否成功渲染。** 图像显示问题仍待确认；旧 EXE 不显示新诊断文字属于混合版本预期。

针对首屏迟迟不发起图片请求的可能性，本修订让目录第一页前 8 张封面与首页前 6 张、详情和 Hero 使用 `loading=eager`；只有前 2 张及焦点/详情使用高下载优先级，其他条目保留 lazy，避免整个分页瞬间抢占网络。在实验社区来源中，如果图片下载超过 8 秒，显示“封面请求超过 8 秒”；图像实际加载成功会消除提示，`onError` 仍回退渐变并显示“封面图片加载失败”。没有新增图片网络请求或重试循环，没有打印封面 URL/签名，也没有更改任何 DRM/媒体权限。

**该补丁是启动时机优化及可观察性增强，尚不能宣称解决 CDN 或 WebView2 网络问题。** 如果更新后的 EXE 仍长时间超时或失败，需检查 Windows WebView2 DevTools Network 里的状态（可只反馈状态码、耗时、失败原因，勿贴 signed URL、Cookie、Token）并与系统代理配置比较。只在新 HEAD Windows M1/M2 都成功后部署调试包并复测。

## HG-2B Windows 实机：详情 HTTP 503 与代理排障

用户 2026-11-22 现场截图证明：至少一张真实社区封面已加载，但点击该作品后显示 `hgcommunity_http_503`。用户系统代理为 `127.0.0.1:7897`（HTTP/HTTPS 环境变量与 HKCU 用户代理一致），WinHTTP 是直连；**WinHTTP 直连不能单独判定 WebView2 不走代理**。真实原因仍未确认。

源码定位：原 `/v1/series/:id` 同时发起 `getSeries` 和 `listEpisodes`，任一报错时 Node 网关的异常处理仅返回 503，Rust 再丢弃 JSON error code，UI 无法判断限流、网络、鉴权和内容访问限制。

本次修订：优先通过 SDK 已确认存在的 `getSeriesBatch([id])` 单次获取作品公开详情和分集，严格校验返回 ID，保留不存在该方法时的旧调用路径；不增加自动重试、不会调用播放或解密。Node 仅返回预定义错误码（如 `TRANSPORT`、`RATE_LIMITED`、`CONTENT_UNAVAILABLE`），Rust 严格白名单解析，再由前端显示用户可读提示。Node 控制台新增 `HG2B_DETAIL_DIAG code=... status=...`，不会打印作品 ID、URL、签名参数、token 或异常原文。

**验收**：新的 CI 同一 HEAD 通过后使用新调试 EXE + 新 sidecar；打开封面正常的作品，观察是否能显示剧集与不可播放状态。如失败，提供 `HG2B_DETAIL_DIAG` 计数/错误码和截图。若 `TRANSPORT` 检查 WebView2/Node 的网络路径；若 `RATE_LIMITED` 暂停频繁刷新；若 `CONTENT_UNAVAILABLE` 则不伪造授权或触发播放。不能仅以新修订代码存在就判定已修复。

## Windows 验收流程

首先确认最新 HG-2B HEAD 的 Windows M1/M2 同 SHA 均全绿（其中新增 Node + TypeScript、Rust 类型/原生测试）。在 **HG-2B Debug EXE** 部署到独立目录后，仍沿用 HG-2A 的显式本机 SDK 启动方式。用户先关闭普通模式播放器，再从仓库 HG-2B 分支启动开发脚本：

```powershell
cd D:\Download\HongGuoRust-HG2A-Source
git fetch origin
git checkout feat/hg2b-real-covers-and-episode-status
npm.cmd install --prefix .\scripts\hg2a-sdk --ignore-scripts
powershell.exe -NoProfile -ExecutionPolicy Bypass -File ".\scripts\start-hg2a-community.ps1" -ExePath "$env:LOCALAPPDATA\HongGuoRust-HG2B-Test\hongguorust.exe"
```

注意 `git checkout` 必须保证工作树没有未提交本地更改。实验服务固定占用本机 127.0.0.1:19091；若 EADDRINUSE，先核实端口和 PID 后决定是否手工关闭**已确认是旧 HG-2A Node** 进程，脚本不会自动 kill。

## 实机验收 10 项

- [ ] 1. 在没有 HG-2A 实验环境变量的普通启动中，不出现 `hgcommunity`，HG-1 红果待授权源仍禁播。
- [ ] 2. 显式启动社区实验服务，在非官方「社区接口实验」搜索已确认的真实剧名（如“重生”）；搜索结果剧名和集数仍来自 SDK 实时返回，而非测试占位数据。
- [ ] 3. SDK 返回 `cover_url` 的作品在列表显示实际不同的图片，不再全部统一渐变；网络加载失败/已过期签名的作品保留渐变兜底，不出现损坏图片标志。
- [ ] 4. 打开真实作品详情，显示该剧的真实封面与简介；返回列表、再次进入详情和搜索切换仍正常。
- [ ] 5. 首页「全部」或「社区 SDK」来源下的焦点和普通卡片显示真实封面；演示/本地/自有清单的原占位图不回归。
- [ ] 6. 真实分集列表显示集数与标题；上游 `playable=false` 条目仍显示但按钮禁用并明确标“上游标记不可播放”，不会因过滤后条目缺失而误报集数。
- [ ] 7. 未提供 `-AllowedMediaHosts` 时，社区内容详情标明“仅浏览、不开放播放”，所有分集按钮不可点；原生播放解析仍不会因为修改 UI 而解锁。
- [ ] 8. 测试无效/超长/非 HTTPS 封面链接只能走占位；错误不会泄露 URL 签名 query 到普通 UI/日志，不能发起 HTTP/内网图片请求。若远端图片反复 404，确认无死循环。
- [ ] 9. 搜索快速修改、刷新、取消、切换来源和多次打开详情后没有旧结果覆盖、闪退、空白/重叠封面；图片懒加载不卡住主界面。
- [ ] 10. M7–HG-2A 的本地 MP4、演示、自有 JSON、收藏/历史/续看、M11 音轨字幕、M12 诊断/损坏媒体回归正常。**实际红果视频播放仍需另立已获许可的媒体验收项**。

## 质量门槛

- [ ] 最新准确 SHA Windows M1：TS 类型检查、Node HG-2B URL/SDK 结构测试、Rust fmt/tests、Tauri 编译通过。
- [ ] 最新准确 SHA Windows M2：正常原生 MP4 音画与退出探针通过。
- [ ] 用户 Windows 10/10 实际验收完成；无内容授权/未验证的媒体播放不标记通过。
- [ ] PR #20–#29、#31、HG-2B Draft 状态与逐项合并授权保持；Issue #7 发布许可证门槛独立存在。

更多原有接口条件和上游风险见 `docs/architecture/hg2a-community-sdk-integration.md`。
