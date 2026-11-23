# HG-2A / HG-3A：直接复用开源红果 SDK，并由桌面应用管理

> **当前实现状态（HG-3A）**：旧版 HG-2A 的 PowerShell 显式启动方式已被 Tauri 管理的 Sidecar 生命周期取代。桌面运行时默认注册 `hgcommunity` 真实内容源；不再将演示 Provider 或“红果待授权”占位源放入用户界面。`start-hg2a-community.ps1` 仅保留为历史开发工具。HG-3A 当前默认只浏览真实元数据与封面；媒体播放仍需显式配置允许的主机，并且只支持未受保护的直接 HTTPS MP4。

## 选择结论（2026-11-22 仓库源码核对）

用户提出优先研究实际可用的 GitHub 非官方 PC 客户端。HG-2A 因此调整为：**直接使用开源 `openhongguo` 的 Node.js SDK 获取真实目录、搜索和分集**；保留 Mock 仅作为离线 CI 回归，绝不将虚构素材替代为真实检索结果。

核对过的上游源码：

- [beilunyang/openhongguo](https://github.com/beilunyang/openhongguo)（[Node README](https://github.com/beilunyang/openhongguo/blob/main/bindings/node/README.md)、[Go 搜索代码](https://github.com/beilunyang/openhongguo/blob/main/sdk/catalog.go)、[播放代码](https://github.com/beilunyang/openhongguo/blob/main/sdk/media.go)）：有实际的 `discover/search/getSeries/listEpisodes/resolvePlayback` 接口。Node 包是 Go→WASM 绑定。SDK 的匿名设备、签名、请求限流与恢复**由上游维护**，我们不另行编写签名/设备轮换或媒体解密代码。
- [kyrqon/hongguo-web](https://github.com/kyrqon/hongguo-web)（[server/provider.mjs](https://github.com/kyrqon/hongguo-web/blob/main/server/provider.mjs)）：直接通过上游 SDK 做后端，并将结果映射为作品、选集、播放模型；适合参考 Rust Provider 的中间数据契约，不直接复制其应用账户系统或媒体处理链路。
- [zfdang/hongguo-open-player](https://github.com/zfdang/hongguo-open-player)（[本地 server/index.js](https://github.com/zfdang/hongguo-open-player/blob/main/src/server/index.js)）：Electron + Node/Express + Vue 独立桌面应用；本地 HTTP 服务为界面提供目录、搜索、详情、播放操作。可参考服务边界和可观察性；视频解密与原生客户端模块不直接搬入本项目。
- [yangbin1322/hongguo-player](https://github.com/yangbin1322/hongguo-player)：仓库 README 明确声明仅发安装包、不公开源码，不适合作为可审计的接入实现。

**授权边界**：以上都不是红果官方第三方桌面 API。上游开放源代码的许可证不等于平台内容、私有 API 或短剧播放/再分发许可证。openhongguo package.json 指明 Apache-2.0，Node README 另写了学习/非商用声明；将其纳入产品分发前需逐项复核许可证文件、README 附加条款、平台接入协议和内容权利。HG-2A 仅用于个人非商业学习；不代表官方接口或内容授权。

## HG-3A：桌面应用托管 Sidecar（当前）

- `src-tauri/src/catalog/sidecar.rs` 在 Tauri 启动前定位随包提供的 `hg2a-sdk/server.mjs` 与 Node.js 22+ 运行时，启动 Node 子进程并等待 `HG3_READY` 握手。
- Node 使用 `crypto.randomBytes(32)` 生成每次启动独立的 256-bit token，通过子进程 stdout 私有管道交给 Rust；token 不写日志、不落盘。Sidecar 仍只绑定 `127.0.0.1:19091`，Rust 仅在握手成功后持有该 token。
- 关闭窗口时先等待 libmpv worker 退出，再停止并回收 Node 子进程。Sidecar 缺失、启动失败或端口被占用时，Provider 保持可见但明确报错；绝不回退到 Mock/演示数据，也不复用未知进程。
- `CatalogService::new_for_desktop` 只注册 `hgcommunity` 和用户自有 `manifest`；演示源、旧的 `hongguo` 待授权占位源及单独的 local Provider 不进入正式剧库来源。用户本地视频仍由独立媒体库管理。
- Windows M2 调试产物新增 `HongGuoRust-HG3A-Windows-Debug`，包含 `hongguorust.exe`、`node.exe`、完整 `hg2a-sdk` 依赖目录及 `libmpv-2.dll`；直接运行 EXE，不再手动运行 PowerShell 启动器。
- 默认未设置 `HONGGUORUST_HG3_MEDIA_HOSTS`，因此真实目录可浏览，但播放能力关闭。只有用户明确提供精确媒体域名 allowlist 后，才允许符合现有 SDK 与 Rust 校验的未保护 HTTPS MP4；不解密受保护媒体。

## 实际实现（HG-2A/HG-2B 基础）

`scripts/hg2a-sdk/package.json`（固定上游版本 0.4.0）和 `scripts/hg2a-sdk/server.mjs`：

- 通过上游 Node SDK 的 `discover/search/getSeries/listEpisodes` 请求真实短剧目录/搜索/选集。
- 从现有 SDK 实际类型映射到本项目的固定字段 `{schemaVersion:1,items:[{id,title,description,genre,episodes}]}` 与 `{detail:{series,episodes}}`。
- 保留异常响应：限流、签名失败、匿名设备受限、需要登录/验证等返回有限错误码；**绝不回退为演示视频**。
- SDK 用仅内存的匿名设备档案，`deviceResets:0`、`retryAttempts:0`、`timeoutMs:8000`；不自行批量注册设备，不复制签名算法。SDK 实际仍可能发起匿名设备注册，这是上游协议的一部分。
- 单集取址路径仅接受 **SDK 明确 `protected=false`、非 HLS/DASH、可直接访问的 HTTPS MP4**。如果受保护、没有直链或接口失败，直接返回明确错误；不使用上游 `contentKey`，不做 CENC/DRM 解密、付费墙或登录绕过。
- HTTP 服务只绑定 `127.0.0.1:19091`，所有调用需进程随机 48+ 位十六进制会话令牌（HTTP 自定义 Header），拒绝 Origin、非 GET、非法路径，不开放 CORS，不保存密钥或目录，结果不包含 SDK 原生设备信息/内容密钥。
- `src-tauri/src/catalog/hgcommunity.rs`：Rust 用 `std::net::TcpStream` 访问固定 Loopback，不新增 HTTP crate，限定连接/总时限、响应体及 Header 上限，校验 HTTP 状态、JSON schema、ID/集数、URL 和显式提供的媒体域名列表。源 ID 是 `hgcommunity`，严格和 HG-1 的 `hongguo`（待授权/全禁播）分离。
- 历史 HG-2A 仅在 Rust debug build 且显式环境变量有效时注册；HG-3A 的桌面运行时改为通过 Tauri 托管 Sidecar 握手后注册真实源。媒体播放额外要求显式设置 `HONGGUORUST_HG2A_MEDIA_HOSTS` 域名列表，未设时只可浏览真实元数据。
- `src/catalog/CatalogView.tsx` 默认显示真实红果来源并标注「非官方」；演示与待授权占位来源不再显示。
- `tests/hg2a-community-gateway.test.mjs` 对上述 Node API 使用 Fake SDK 测试（不联网），包括真实 SDK 结构、Loopback 会话令牌、异常码脱敏、保护媒体拒绝及无效 ID。
- 之前 `scripts/hg2a-mock-server.mjs` **仅保留为离线故障测试工具**，不是 HG-2A 产品的默认数据来源。

### 后续仍需要

1. **还未在用户 Windows 上验证真实接口**。SDK 可能因为上游匿名请求限制、签名失效、IP 地区或 CDN 变动而无法返回作品、选集或直链；必须保留失败结果、不能虚报正常。
2. 当前桥接仅支持 SDK 非加密直接 HTTPS MP4。大部分真实片源如果是 CENC 保护流、HLS、DASH、ByteVC2 或需要特定请求 Headers，将返回可诊断的不支持状态，不承诺完整播放。
3. 该桥接目前是**实验开发链路**。正式发布需要补齐许可、用户确认、供应链安全审核、SDK 来源锁定和不同来源的隐私与访问边界；HG-1 官方待授权来源不能被社区实现覆盖。
4. 尽管桥接固定使用 127.0.0.1、令牌和媒体主机白名单，**域名字符串检查不等于完整 DNS 重绑定/SSRF 防护**；在公开发布和任意 CDN 取址前还需对解析 IP、重定向和原生播放器网络权限进行进一步限制。


### HG-2B HEIC 封面格式兼容

实机 WebView2 Network 已观察到社区封面返回 HTTP 200、image/heic；应用 img onError 与 DevTools Preview 破损图标表明问题位于图片解码层。HG-2B 因此在本机 Node sidecar 增加受限的图片中继与格式归一化：
- 原始 CDN signed URL 只存于进程内存；只接受 fqnovelpic.com 根域及子域，不提供任意 URL 参数入口。
- 向前端返回 128-bit 随机 opaque ID 的固定 Loopback 图片 URL，Tauri CSP 仅对 img-src 增加 http://127.0.0.1:19091。
- Node 使用 HTTPS_PROXY / ALL_PROXY 等环境代理下载，拒绝 allowlist 外重定向，不回退直连；设置大小、像素、并发、缓存和 ID TTL 限制。
- HEIC/HEIF 使用 libheif-js WASM 解码并由 jpeg-js 输出 JPEG；其他常见静态图片 MIME 经过 allowlist 后原样返回。图片只驻留内存，不落盘，不记录签名 URL。
- 新依赖固定为 libheif-js@1.23.5（LGPL-3.0）、jpeg-js@0.4.4（BSD-3-Clause）、https-proxy-agent@9.1.0（MIT）；安装命令仍是 npm install --prefix scripts/hg2a-sdk --ignore-scripts。Windows M1/M2 workflow 会单独安装此可选 sidecar 依赖。

## Windows 手动运行（开发试验）

在已克隆本仓库、安装 Node.js 22+/24 的 Windows 机器中，先在用户主动允许第三方非官方 SDK 的前提下安装独立依赖：

```powershell
npm install --prefix .\scripts\hg2a-sdk --ignore-scripts
```

上面只安装专门的 Node SDK 包，不改变主项目的 `package.json` 或 Tauri 客户端依赖。需要先等 GitHub M1/M2 CI 编译通过、按旧流程部署 HG-2A Debug EXE 到 `%LOCALAPPDATA%\HongGuoRust-HG2A-Test`。

然后（从仓库根目录）显式启动实验环境：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\start-hg2a-community.ps1
```

启动器为本次实验生成临时令牌、以相同环境启动 Node SDK 后端及 HG-2A Debug 客户端，退出客户端即停止后端，脚本不会自动安装依赖或修改原有软件数据。

默认不传媒体主机列表，所以**只启用真实目录/搜索/选集**；任何单集播放路径仍被 Rust 禁止。后续只有在校验来源主机、视频授权和直接 MP4 支持后，才考虑通过 `-AllowedMediaHosts` 显式允许合法的测试媒体域名。

完整真实测试项：[HG-2A Windows 验收](../acceptance/hg2a.md)。未完成真实 Windows 请求和实际视频验收前，不写成“红果全站已接通”。
