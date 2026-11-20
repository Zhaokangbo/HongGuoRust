# HongGuoRust 第一阶段架构与验收计划

设计日期：2026-11-20。目标为 Windows 10/11 x64 单窗口、单播放会话客户端。本阶段输入为用户选择的本地 MP4 和公开 HTTPS 直链测试片。内容平台接入、受保护内容解密和媒体下载均在范围之外。

## 1. 开发环境与开源选型

D:\HongGuoRust 在检查时为空目录，无 Git 仓库。Windows x64，build 26200。已检测到 Node 23.9.0、npm 10.9.2、Git 2.49.0、CMake 3.31.7、WebView2 Runtime 157.0.4341.60。当前终端未找到 rustup、rustc、cargo、cl、link 或 cargo-tauri；发现 VS 12.0/14.0 目录及 Windows SDK 10.0.10150.0，尚未验证可用的现代 MSVC x64 工具链。未发现 mpv 命令，工作区没有 libmpv DLL。能否运行桌面程序、安装包均待实测。

M1 采用 Rust stable x86_64-pc-windows-msvc、VS 2022 C++ 桌面工作负载和相配套的 Windows SDK，保留已有 VS 版本。前端选受支持的 Node LTS、npm 和项目内 Tauri 2 CLI。以实际运行 Rust 链接、Vite 构建及 Tauri 启动作为判据。[Tauri Windows 环境说明](https://v2.tauri.app/start/prerequisites/)

| 候选 | 复用价值与质量证据 | Windows 与许可证 | 决策 |
| --- | --- | --- | --- |
| tauri-plugin-libmpv | 已发布 Rust crate，有嵌入示例、资源布局和透明窗口说明。公开示例以 JS 操作插件，Rust 侧可用接口及回调销毁顺序仍须验证 | 上游报告已测 Windows。插件 MPL-2.0，另需 LGPL-2.1 的 libmpv-wrapper.dll 和独立的 libmpv-2.dll | 优先试验其嵌入实现。只有 Rust 能接管命令、原始事件与退出顺序，才采用完整插件 |
| libmpv2 Rust 绑定 | 包含事件、渲染示例，多个发布版本，可直接将引擎封装在 Rust 适配器中 | crate 声明 LGPL-2.1。需验证 MSVC 导入库链接、DLL 装载和 HWND 嵌入 | 插件缺少必要 Rust 接口时，采用绑定和 Win32 视频宿主；沿用同一业务接口 |
| tauri-plugin-mpv | 有 Tauri UI 和窗口示例，作者已指向新插件 | MPL-2.0；使用外部 mpv 进程和 JSON IPC | 仅作为历史方案比较 |

依据：[Tauri 插件](https://github.com/nini22P/tauri-plugin-libmpv)、[wrapper](https://github.com/nini22P/libmpv-wrapper)、[libmpv2 清单](https://docs.rs/crate/libmpv2/latest/source/Cargo.toml)、[旧插件](https://github.com/nini22P/tauri-plugin-mpv)。

原生依赖使用固定版本、通用 x86_64 构建和 SHA-256。分发前逐项核对插件、Rust 绑定或 wrapper、mpv、FFmpeg 及其依赖的实际构建配置，备齐适用的许可证、对应源码和构建材料。mpv 默认 GPL，提供剔除 GPL-only 文件的构建选项；单凭文件名或开关无法确认最终二进制的授权。[mpv Copyright](https://github.com/mpv-player/mpv/blob/master/Copyright)

## 2. 目录与模块职责

下列是目标布局。各目录随对应里程碑引入。

| 路径 | 所有权和职责 |
| --- | --- |
| src/components/、src/player/ | React 控件、文件选择入口、显示快照、发送用户意图，维护拖动进度条等短暂 UI 状态 |
| src-tauri/src/commands.rs | Tauri 类型化 IPC、参数校验、调用应用服务、返回请求受理结果 |
| src-tauri/src/player/state.rs | 无 Tauri/libmpv 依赖的 Rust 状态归约、会话编号、快照、错误类型 |
| src-tauri/src/player/service.rs | 串行处理用户意图、关联异步回复与媒体事件、快照发布、关闭协调 |
| src-tauri/src/player/engine.rs | 适配选中的 libmpv 实现，独占原生句柄，翻译命令、属性和结束原因 |
| src-tauri/src/window.rs | UI 线程上的原生窗口操作，视频区域、DPI、焦点、全屏和关闭 |
| src-tauri/src/provider.rs | Provider trait 解析媒体标识并返回 MediaSource。首版支持本地路径、公开测试源，界面与状态机只依赖解析结果 |
| src-tauri/src/telemetry.rs | tracing 结构化诊断，首帧、缓冲区间、错误及退出日志 |
| src-tauri/tauri.conf.json、src-tauri/capabilities/ | 窗口、前端权限、运行时资源和 NSIS 配置 |
| scripts/、docs/ | Windows 环境检查、构建打包与验收说明；当前设计文档位于 docs/ |

## 3. Rust 控制流与资源生命周期

React 提交 open_local、open_test_source、set_paused、seek_to、set_rate、set_volume、set_fullscreen、stop。Rust 暴露 get_snapshot 和 player_state 事件。命令受理后由引擎回报结果。React 初始先订阅再取快照，按递增 revision 接受更新，避免晚到的旧快照覆盖新状态。状态变化即时推送，播放进度建议节流到每秒 4 次。

每次打开分配 session_id。Worker 串行操作 libmpv，引擎阻塞等待置于专用线程；Tokio 负责命令通道、取消协调及异步通知。适配器用原生 playlist_entry_id 或独立会话句柄识别事件归属。只在可确认归属时转发；无身份的属性通知在换片时重新订阅或重新读取。拖动进度条时由 UI 本地预览，松开提交绝对时间，Rust 校验有限数值、时长和 seekable。全屏走 Tauri 窗口，由窗口实际状态反馈 UI。

关闭时先阻止窗口立即销毁，停止接收新命令，使会话失效，唤醒事件线程并请求终止播放。工作线程解除回调，等待在途回调退出，释放引擎和渲染资源并退出。后台等待 join 完成，再交由 UI 线程销毁仍有效的窗口。关闭与 stop 均须可重复调用。网络加载或缓冲期间允许取消；网络超时和异步中断需要在原型中实测。超时记录卡住的阶段，禁止强行释放仍被使用的原生对象。

## 4. 状态与日志

| 状态 | 进入条件及重要边界 |
| --- | --- |
| Idle | 初始无媒体，或当前会话主动停止、取消后 |
| Loading | 接受新来源，等待当前会话具备播放条件 |
| Playing | 引擎确认播放已开始或恢复，且用户未暂停、引擎未等待缓存 |
| Paused | 用户暂停请求已确认。已加载后，用户暂停优先于等待缓存 |
| Buffering | 等待缓存已阻塞启动或播放，记录一次区间；缓存恢复后按当前暂停意图还原 |
| Ended | 当前会话自然 EOF，终态保留至下一次用户操作 |
| Error | 当前会话打开或播放失败，保留错误阶段和错误码，可打开下一片恢复 |

状态机还保存用户暂停意图、是否已启动、缓存等待及终态原因。Seek 期间记录 seeking 标志；缓存清空与 seek 完成分别处理。旧会话的 EOF、慢打开失败和属性通知均不能覆盖新会话。mpv 的 paused-for-cache 可用于检测因缓存不足暂停。[mpv 属性文档](https://mpv.io/manual/stable/#command-interface-paused-for-cache)

tracing 日志携带 session_id、来源类型和单调时钟耗时。首帧测量从 Rust 接受打开开始，到渲染端首次提交该会话有效视频帧，字段 first_frame_submit_ms，注明观测点。M2 先证明路径能观测到该信号；只有文件加载或播放恢复事件时，单列 startup_ready_ms，首帧指标保持待验。若原生嵌入路径无提交信号，验证 libmpv render API 路径；用已知首帧色块的短片在测试端核对首次可见画面。缓冲记录进入、退出、持续时间及因停止中断的区间；错误记录 open/decode/render 阶段与可取得的原生错误码。URL 日志省略查询串。

## 5. 技术风险、验证顺序及交付

| 里程碑 | 交付 | 验收标准 |
| --- | --- | --- |
| M1 工程骨架 | 环境检查脚本、React/Vite + Tauri 2、Rust 初始 Idle 快照、最小测试 | 能编译启动窗口；React 经 IPC 拿到 Rust 快照；关闭后进程退出 |
| M2 播放垂直切片 | 仅本地 MP4，验证 libmpv 接入、窗口嵌入、首帧观测和释放，记录选型结论 | 中文及空格路径的 H.264/AAC MP4 有画有声；100%/150% DPI 下缩放、遮挡恢复均正常；反复开关 20 次无崩溃和残留进程；记录实际首帧信号 |
| M3 播放控制 | 公开 HTTPS 测试片、播放/暂停、绝对 Seek、倍速、音量、全屏、取消 | 本地与在线均可操作；建议 0.5×/1×/1.5×/2×、音量 0–100、Esc 退出全屏。固定带时间码素材检查 Seek 后位置与播放速度；打开失败后可换片 |
| M4 状态及可靠性 | 七态、结构化日志、针对竞态和异常的测试 | Rust 单测覆盖七态、暂停与缓冲交叠、旧 EOF、取消与失败；使用本地受控限速/断流服务复现 Buffering 和 Error；快速 A→B、加载/缓冲时关闭均不挂起。缓冲有开始和结束，首帧每会话一次 |
| M5 Windows 安装包 | scripts/build-windows.ps1、依赖版本及校验值、许可证材料、NSIS x64 安装程序 | 一条脚本完成锁定依赖安装、测试、构建与打包；在无开发工具和 mpv 的 Win10、Win11 x64 干净环境分别安装、播放两种来源、关闭并卸载。记录 WebView2 已装及缺失时的结果 |

构建脚本目标为 npm ci、TypeScript 检查、Rust 单测、原生依赖校验、Tauri NSIS release 构建；Cargo.lock 和 npm 锁文件进入版本控制。安装程序从应用资源目录加载经校验的 DLL。拟先采用 WebView2 Evergreen 引导安装，缺少运行时的目标机首次安装需要联网。安装包验收使用实际安装后的文件路径。[Tauri Windows Installer](https://v2.tauri.app/distribute/windows-installer/)

本轮交付为架构计划；编码从 M1 开始。Windows 10 兼容性、插件后端控制能力、首帧观测及安装包运行性均尚待验证。
