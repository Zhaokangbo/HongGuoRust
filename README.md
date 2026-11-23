# HongGuoRust

面向 Windows 10/11 x64 的开源桌面媒体客户端。React 处理展示与交互，Rust 通过 Tauri 2 IPC 提供播放状态、控制和资源生命周期。项目采用 GPL-3.0-only。

## 开发状态与路线图

### HG-3A：红果真实内容默认接入（开发中）

- 桌面运行时默认注册真实 `hgcommunity` Provider；演示 Provider 和“红果接入（待授权）”占位 Provider 不进入用户界面。
- Tauri 自动启动/关闭本机 `openhongguo` Node Sidecar；Windows 调试包携带 `node.exe`、SDK 依赖及 `libmpv-2.dll`，无需手动运行 PowerShell 启动器。
- 目录、搜索、详情、分集和封面来自 SDK 实际响应；上游失败显示错误，不回退虚构内容。
- 默认仅浏览真实目录。媒体播放仍需明确配置媒体主机 allowlist；受保护媒体、DRM、付费内容及官方账号登录不在范围内。
- 该 SDK 仅用于个人非商业学习，不代表红果官方接口或内容授权；HG-3A 尚待 Windows CI 和实机验收。


| 阶段 | 交付 |
| --- | --- |
| M0 | 仓库、许可证和架构基线 |
| M1 | 单窗口桌面应用，读取 Rust Idle 快照 |
| M2 | libmpv 本地 MP4、首帧及关闭验证 |
| M3 | 公开测试视频和播放控制 |
| M4 | 播放状态、可靠性测试和日志 |
| M5 | Windows x64 本机安装包验收（源码已合入，公开分发仍受 Issue #7 约束） |
| M6 | Provider 演示目录与本地 MP4 媒体库（已合入） |
| M7 | 播放历史、断点续播、收藏夹与进度可视化（已合入） |
| M8–M14 | 搜索与列表 → 合法 Provider 扩展 → 短剧业务闭环 → 播放体验 → 稳定性 → 发布 → 长期扩展，详见 [产品路线图](docs/roadmap.md) |

Windows 客户端由 Rust 独占 libmpv 会话；本阶段将默认内容源切换为非官方 openhongguo SDK，Tauri 自动管理本机 Node Sidecar。真实目录、搜索、详情、选集和封面以 SDK 实际响应为准；SDK 不可用时显示错误，不回退到演示数据。播放仍只接受用户明确允许域名上的未受保护 HTTPS MP4；不登录官方账号、不解密受保护媒体、不绕过付费或访问限制。M5 源码及本机安装验收已完成，但**仍未完成干净 Windows 10/11 环境验收及第三方依赖对应源码/许可证合规，不发布公开安装器**；公开分发阻断项见 [Issue #7](https://github.com/Zhaokangbo/HongGuoRust/issues/7)。M6–M8 已合入本地媒体库、观看历史、断点续播、收藏、搜索排序及自动连播；M9 的 Provider、请求取消与用户自有 JSON 清单能力已完成；M10/UI 的此前阶段仍按各自 Draft PR 与实机验收记录管理。详见 [HG-3A 验收](docs/acceptance/hg3a.md)、[HG-2A/HG-2B 接入设计](docs/architecture/hg2a-community-sdk-integration.md)、[本机启动说明](docs/acceptance/run-windows.md) 和 [许可证评估](docs/architecture/licensing.md)。

## Windows 开发构建

准备 VS 2022 的 C++ 桌面开发工具及 Windows SDK、Rust stable MSVC x64、Node.js 24 LTS（也支持 22.12 以上的 22 LTS）和 WebView2 Runtime。项目已提交 npm 和 Cargo 锁文件。在项目根目录执行以下命令。

```powershell
cargo install tauri-cli --version 2.11.4 --locked
.\scripts\check-windows.ps1
$native = Join-Path $env:TEMP 'HongGuoRust-mpv'
.\scripts\prepare-mpv-msvc.ps1 -Destination $native
$env:LIB = "$native;$env:LIB"
$env:PATH = "$native;$env:PATH"
npm ci
npm run tauri -- dev
```

构建并运行桌面可执行文件：

```powershell
npm run tauri -- build --no-bundle -- --locked
.\src-tauri\target\release\hongguorust.exe
```

## 非官方声明

HongGuoRust 为独立开发的非官方项目，与红果及相关平台无隶属或合作关系。输入范围为用户选择的本地媒体和公开测试视频。仓库不分发第三方媒体。
