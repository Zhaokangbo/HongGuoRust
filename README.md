# HongGuoRust

面向 Windows 10/11 x64 的开源桌面媒体客户端。React 处理展示与交互，Rust 通过 Tauri 2 IPC 提供播放状态、控制和资源生命周期。项目采用 GPL-3.0-only。

## 开发状态与路线图

| 阶段 | 交付 |
| --- | --- |
| M0 | 仓库、许可证和架构基线 |
| M1 | 单窗口桌面应用，读取 Rust Idle 快照 |
| M2 | libmpv 本地 MP4、首帧及关闭验证 |
| M3 | 公开测试视频和播放控制 |
| M4 | 播放状态、可靠性测试和日志 |
| M5 | Windows x64 安装包验收 |

当前主分支为 M0；M1 在 `feat/m1-scaffold` 开发。[第一阶段架构](docs/architecture/HongGuoRust-Phase1-Plan.md) · [许可证评估](docs/architecture/licensing.md)

## Windows 开发构建

M1 分支使用 Rust stable MSVC x64、VS 2022 C++ 桌面开发工具、Windows SDK、Node.js 22 LTS（22.12 以上）及 WebView2 Runtime。进入包含工程代码的分支，在项目根目录执行：

```powershell
npm ci
npm run tauri dev
```

构建并运行桌面可执行文件：

```powershell
npm run tauri build -- --no-bundle
.\src-tauri\target\release\hongguorust.exe
```

## 非官方声明

HongGuoRust 为独立开发的非官方项目，与红果及相关平台无隶属或合作关系。输入范围为用户选择的本地媒体和公开测试视频。仓库不分发第三方媒体。
