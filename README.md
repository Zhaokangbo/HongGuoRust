<div align="center">

# HongGuoRust (`hongguo-rust`)

**红果短剧 Windows 电脑版开源桌面客户端 · Hongguo Short Drama Desktop Player**

基于 **Rust + Tauri 2 + libmpv** 原生构建 — 免安卓模拟器 · 1080P 高清 · 原站滚动弹幕 · 官方扫码双向同步 · WebDAV / 本地暗黑影院

[![Latest Release](https://img.shields.io/github/v/release/Zhaokangbo/hongguo-rust?style=flat-square&color=e50914&label=最新版本)](https://github.com/Zhaokangbo/hongguo-rust/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/Zhaokangbo/hongguo-rust/total?style=flat-square&color=2ea44f&label=累计下载)](https://github.com/Zhaokangbo/hongguo-rust/releases)
[![Stars](https://img.shields.io/github/stars/Zhaokangbo/hongguo-rust?style=flat-square&color=f59e0b&label=Stars)](https://github.com/Zhaokangbo/hongguo-rust/stargazers)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11%20x64-0078D6?style=flat-square)](https://github.com/Zhaokangbo/hongguo-rust/releases/latest)
[![Tech](https://img.shields.io/badge/Built%20with-Rust%20%2B%20Tauri%202%20%2B%20libmpv-orange?style=flat-square)](#技术架构与工程亮点)
[![License](https://img.shields.io/github/license/Zhaokangbo/hongguo-rust?style=flat-square&label=License)](LICENSE)

[**立即下载安装版 / 绿色版**](https://github.com/Zhaokangbo/hongguo-rust/releases/latest) · [**界面预览**](#界面预览) · [**核心功能**](#核心功能v032) · [**技术亮点**](#技术架构与工程亮点) · [**常见问题**](#常见问题-faq) · [**本地构建**](#windows-本地开发与构建)

</div>

---

## 为什么选择 HongGuoRust？

**HongGuoRust（hongguo-rust）** 是一款专为 **Windows 10/11 x64** 打造的开源红果短剧 PC 客户端与多源暗黑影院播放器。相比安卓模拟器或臃肿的 Electron 套壳应用，它直接以 **Rust + Tauri 2 + 原生 `libmpv`** 驱动：

- **免模拟器，原生轻量流畅**：直接运行于 Windows 桌面，原生 `libmpv` 硬解渲染，拒绝卡顿与高内存占用，安装或解压即用。
- **1080P 高清 + 原站实时弹幕**：自动优选最高兼容清晰度（支持 1080P / 720P / 540P / 480P / 360P 秒切续播），内置 8 轨道平滑滚动原站弹幕，支持开关与透明度调节。
- **官方红果 App 扫码登录，手机电脑无缝接力**：支持手机端「红果免费短剧」App 扫一扫登录，双向同步 **云端追剧收藏、观看历史进度与 App 预约片单**；登录态使用 Windows 原生 DPAPI 加密保护。
- **Netflix 暗黑影院界面**：原生 Windows 暗色标题栏、焦点大图（Hero Billboard）、横向海报轨道、进度条悬停实时缩略图预览、倍速/多音轨/字幕/右侧选集抽屉与完整快捷键。
- **不止看红果短剧**：同时原生接入 **WebDAV 媒体库（Alist / OpenList / 飞牛 NAS / 群晖 / Nextcloud / rclone）**、远程/本地 JSON 片单，以及 **本地视频右键「打开方式」** 直接播放。

---

## 立即下载与安装（Windows 10/11 x64）

前往 [**GitHub Releases**](https://github.com/Zhaokangbo/hongguo-rust/releases/latest) 下载由 GitHub Actions 自动编译发布的最新版本。安装包与绿色版**均已内置 `libmpv-2.dll` 与 `vulkan-1.dll` 运行时**，无需额外配置解码器：

| 版本类型 | 文件名格式 | 特点与适用场景 |
| :--- | :--- | :--- |
| **安装版（推荐）** | `HongGuoRust-*-windows-x64-setup.exe` | 双击一键安装，自动创建开始菜单快捷方式并注册视频文件右键「打开方式」关联 |
| **绿色便携版** | `HongGuoRust-*-windows-x64-portable.zip` | 免安装，解压后直接双击 `hongguorust.exe` 运行 |
| **校验文件** | `SHA256SUMS.txt` | 提供所有发布包的 SHA-256 哈希，便于核验文件完整性 |

> **提示**：应用首次运行时，若系统尚未安装 Microsoft Edge WebView2 Runtime，安装程序会自动引导补装。

---

## 界面预览

### 首页推荐、焦点大图与继续观看
![hongguo-rust 红果短剧电脑版首页截图](docs/screenshots/home-v0.2.jpg)

### 1080P 原生播放、原站滚动弹幕与清晰度切换
![hongguo-rust 1080P 播放与原站弹幕界面截图](docs/screenshots/player-v0.2.jpg)

### 剧集详情、全剧进度总览与分集检索
![hongguo-rust 剧集详情与选集界面截图](docs/screenshots/detail-v0.2.jpg)

---

## 核心功能（v0.3.2）

### 1. 红果短剧片库与高清播放
- **海量片库与全站搜索**：支持片库多页渐进加载（触底自动加载与「加载更多」，覆盖 34 页 / 800 部推荐短剧）以及关键词多页搜索与 App 接口兜底。
- **多档清晰度无缝切换**：默认自动选择最高可用兼容画质（如 1080P，同清晰度优先 H.264/AVC），播放器控制栏支持一键切换 1080P / 720P / 540P / 480P / 360P，切画质时保留当前播放秒数无缝续播。
- **原站时间轴滚动弹幕**：透明悬浮叠加层内置 8 轨道平滑滚动弹幕，自动同步播放进度、倍速、暂停与拖拽定位，支持一键开关与透明度调节。
- **稳定的断点续播**：剧集详情页规范展示分集标题（`剧名 · 第 N 集` 或上游单集标题），以稳定进度键持久化观看历史，不因 CDN 链接过期丢失续播进度。

### 2. 官方 App 扫码登录与双向云端同步
- **一键扫码登录**：右上角呼出扫码弹窗，使用手机端「红果免费短剧」App 扫一扫确认即可完成登录（严格限定仅允许 `https://hongguoduanju.com` 官方域名跳转）。
- **云端追剧收藏双向同步**：拉取手机端云端收藏列表（自动将签名 `.heic` 封面归一化为桌面端可直接展示的公开 JPEG），桌面端收藏/取消收藏实时回写云端。
- **观看历史与播放进度双向回传**：按时间戳与本地记录合并观看历史；播放中节流上报，暂停/切集/播完时立即后台同步至云端，手机电脑无缝接力。
- **App 预约片单**：在「我的收藏 → App 预约」中同步展示 **已上线预约（可直接点击播放）** 与 **待上线预约（显示上线预告）**；退出登录自动清空云端态且完整保留本地片库与离线收藏。

### 3. 暗黑影院交互与 `libmpv` 专业播控
- **沉浸式透明悬浮控制层**：视频铺满主窗口，控制条与弹幕通过透明从属叠加窗口浮于画面之上；播放时鼠标静止自动隐藏控制条与光标。
- **顺手的播控体验**：支持单击播放/暂停、双击全屏、悬停进度条实时缩略图预览、`0.5×–2.0×` 倍速、音量与静音记忆、清晰度/弹幕/多音轨/字幕切换、右侧滑出式选集抽屉及完整键盘快捷键。
- **智能网络与代理适配**：自动适配 Windows 系统代理并在代理异常重置时自动回退直连，局域网与本机回环流始终直连。

### 4. 多来源个人媒体库（WebDAV / 本地 / JSON）
- **WebDAV 媒体库**：兼容 **Alist / OpenList、群晖 / 飞牛 NAS、Nextcloud、rclone** 等标准 WebDAV 服务，支持 Basic 认证，自动聚合多集剧集并按自然集数排序。
- **本地视频库与右键直播**：管理本机视频，支持断点续播与收藏，支持 Windows 资源管理器右键「打开方式」直接播放 `.mp4`、`.mkv`、`.mov`、`.m4v`、`.webm`、`.avi`、`.flv`、`.ts`。
- **远程与本地 JSON 片单**：支持公开 HTTPS JSON 链接订阅与一键刷新，或导入本地 `.json` 清单（仓库附带开源电影示例 [open-cinema-manifest.json](docs/fixtures/open-cinema-manifest.json)）。
- **自动抽帧封面缓存**：无海报图的本地与 WebDAV 视频会在后台自动抽帧缓存于 `%LOCALAPPDATA%\HongGuoRust\thumbnails`，收藏与历史跨来源统一管理。

---

## 技术架构与工程亮点

| 层级 | 技术选型 | 核心职责 |
| :--- | :--- | :--- |
| **桌面框架** | Rust + Tauri 2 | 原生 Windows 暗色标题栏、安全 IPC、低内存常驻 |
| **视频播放** | 原生 `libmpv` + 透明叠加窗 | Rust 独占管理播放会话，视频铺满主窗，控件与弹幕独立渲染 |
| **前端界面** | React 19 + TypeScript + Vite | 暗黑影院 UI、分集管理、弹幕舞台与交互状态 |
| **协议与安全** | 纯 Rust 实现（HG-4/HG-5 零新增 Cargo 依赖） | 原生签名、在线 CENC 流解密代理、Windows DPAPI 凭据加密 |

<details>
<summary><strong>展开查看底层协议与安全实现细节（面向开发者）</strong></summary>

- **Windows 原生 DPAPI 加密持久化 & 凭据隔离（HG-5）**
  - 登录态通过 Windows 原生 `CryptProtectData / CryptUnprotectData` 加密原子保存至 `%LOCALAPPDATA%\HongGuoRust\account-session.bin`，重启自动免扫码恢复。
  - Cookie 与设备 ID 严格留在 Rust 进程内，不暴露给前端 WebView2、不落日志，也不会随视频播放、弹幕或 CDN 流请求发送。
- **纯 Rust 协议签名与匿名设备注册（HG-4）**
  - 原生实现 MD5 / SHA-512 / SM3 / AES-128（ECB/CBC/CTR）/ Gzip 与 Android v5（`v04.10.03`, `field 21 = 312`）/ v3 签名及匿名设备注册与冷却轮换，全程在内存中完成，无需登录、不落盘凭据。
  - 纯 Rust 移植官方分时弹幕签名（`X-Gorgon / X-Argus / X-Ladon`）与分段拉取。
- **多路径播放模型解析与编码协商**
  - 支持 `reading.snssdk.com/.../preload/v1` 主路径、`video_tab` 备用路径及多域名回退，自动过滤不兼容的 `bytevc2` 编码并支持 `fallback_api`（`codec_type=1/3`）兼容协商。
- **本机流式解密代理与 CDN 容灾**
  - 内置仅监听 `127.0.0.1` 随机端口的 HTTP/1.1 Range 流代理（256-bit `X-HGR-Stream-Token` 常量时间鉴权），支持 MP4 CENC（`spade_a` + `moov/sinf/saiz/saio`）任意字节偏移在线 AES-128-CTR 解密、原始编码还原与主/备 CDN 头校验容灾切换。

</details>

---

## 常见问题 (FAQ)

<details>
<summary><strong>Q1：使用官方 App 扫码登录安全吗？会泄露账号 Cookie 吗？</strong></summary>

登录过程仅与官方域名 `https://hongguoduanju.com` 通信，登录凭据使用 Windows 系统级原生 DPAPI（`CryptProtectData`）加密存储于当前用户的 `%LOCALAPPDATA%\HongGuoRust\account-session.bin` 中；Cookie 严格隔离在 Rust 后端进程内，不会传给前端页面、不会写入日志，也不会发送到视频 CDN。

</details>

<details>
<summary><strong>Q2：需要安装安卓模拟器、FFmpeg 或其他解码器吗？</strong></summary>

不需要。Windows 发布包已内置 `libmpv-2.dll` 与 `vulkan-1.dll`，下载安装版或解压绿色版后即可直接播放红果短剧与本地/WebDAV 视频。

</details>

<details>
<summary><strong>Q3：支持 macOS 或 Linux 吗？</strong></summary>

目前聚焦 **Windows 10/11 x64** 原生体验与稳定性（含 DPAPI 凭据加密与 Win32 叠加窗控制）。欢迎社区提交 PR 或在 [Issues](https://github.com/Zhaokangbo/hongguo-rust/issues) 中讨论跨平台适配需求。

</details>

<details>
<summary><strong>Q4：为什么下载时浏览器或 Windows Defender 可能提示风险？</strong></summary>

个人开源项目未购买商业 EV 代码签名证书，Windows SmartScreen 首次遇到新版本 EXE 可能会出现拦截提示。所有发布包均由本仓库公开的 [GitHub Actions 工作流](.github/workflows/release.yml) 从源码自动编译生成，并附带 `SHA256SUMS.txt` 供核验；你也可以按下方步骤自行从源码编译。

</details>

---

## Windows 本地开发与构建

环境要求：Visual Studio 2022（C++ 桌面开发工具与 Windows SDK）、Rust stable MSVC x64、Node.js 22.12+ / 24 LTS、WebView2 Runtime。

```powershell
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass
cargo install tauri-cli --version 2.11.4 --locked

# 1. 准备 libmpv 运行时与 MSVC 导入库
$native = Join-Path $env:TEMP 'HongGuoRust-mpv'
.\scripts\prepare-mpv-msvc.ps1 -Destination $native
$env:LIB = "$native;$env:LIB"
$env:PATH = "$native;$env:PATH"

# 2. 安装前端依赖并启动开发模式
npm.cmd ci
cargo tauri dev
```

编译独立桌面版可执行文件：

```powershell
npm.cmd install --prefix .\scripts\hg2a-sdk --ignore-scripts
cargo tauri build --no-bundle -- --locked
Copy-Item "$native\libmpv-2.dll" .\src-tauri\target\release\ -Force
.\src-tauri\target\release\hongguorust.exe
```

---

## 致谢与许可声明

- 协议研究与架构参考：[N3urda/hongguoTV](https://github.com/N3urda/hongguoTV)（`GPL-3.0-only`）。
- 本项目以 `GPL-3.0-only` 协议开源，为独立开发的非官方开源桌面客户端，与红果及相关平台无隶属或合作关系，仅用于技术学习与个人本地/合法媒体播放研究。

如果这个项目让你在电脑上追剧更顺手，欢迎点一个 **Star ⭐** 支持持续更新！
