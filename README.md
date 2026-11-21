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

默认 Windows 客户端使用 Rust 服务和 libmpv 播放本地 MP4。支持选择文件、替换和停止。运行验证包请参阅[本机启动说明](docs/acceptance/run-windows.md)。[M2 验收记录](docs/acceptance/m2.md) · [M1 验收记录](docs/acceptance/m1.md) · [第一阶段架构](docs/architecture/HongGuoRust-Phase1-Plan.md) · [许可证评估](docs/architecture/licensing.md)

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
