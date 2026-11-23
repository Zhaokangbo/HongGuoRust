# HG-3A Windows 实机验收

## 测试包

从本 PR 对应的 GitHub Actions 下载 artifact：HongGuoRust-HG3A-Windows-Debug。解压后确认以下文件在同一目录结构中：

- hongguorust.exe
- node.exe
- libmpv-2.dll
- hg2a-sdk/server.mjs
- hg2a-sdk/node_modules/openhongguo
- hg2a-sdk/node_modules/libheif-js
- hg2a-sdk/node_modules/jpeg-js
- hg2a-sdk/node_modules/https-proxy-agent

直接双击 hongguorust.exe。不运行旧的 start-hg2a-community.ps1，不需要额外安装 Node.js。

## 必测项目

- [ ] 应用可直接启动，首页默认进入真实红果社区 SDK 内容源。
- [ ] 任务管理器中可观察到应用管理的 Node 子进程；不需要手动打开 PowerShell。
- [ ] 首页不出现“林间光影”“城市片刻”“播放器实验室”“演示内容”或“红果接入（待授权）”占位源。
- [ ] 首页内容来自 SDK 实际返回；不把本地清单或测试视频伪装成红果内容。
- [ ] 真实剧名、集数和封面可加载；点击作品能进入详情并查看实际返回的剧集。
- [ ] 上游离线、限流、签名变化或匿名访问受限时显示可理解的错误和重试入口，不回退到 Mock。
- [ ] 默认显示“仅浏览模式”；不因剧集标记可播放就绕过媒体域名 allowlist。
- [ ] 受保护媒体不播放，不请求或使用内容密钥，不尝试 DRM/CENC 解密。
- [ ] 播放器不再显示 SampleLib 测试 MP4 直链输入框；生产构建隐藏开发诊断按钮。
- [ ] 关闭窗口后 Node 子进程退出；重新打开应用可再次正常启动。
- [ ] 本地视频、已有收藏、观看历史和续播进度保持不变。
- [ ] 原有本地 MP4 播放、历史、收藏、详情页和播放器基本控制没有回归。

## 失败处理

- 如果 Sidecar 没有启动，检查应用目录是否同时存在 node.exe 与完整 hg2a-sdk 目录。
- 如果 127.0.0.1:19091 被占用，应用必须报错而不是复用未知进程；确认占用 PID 后再处理。
- 如果红果上游返回限流、验证或签名错误，记录错误码与时间，不要连续高频刷新，也不要尝试绕过。
- 此阶段尚不验收任何受保护片源播放。真实目录与海报通过，不代表视频播放权限或平台授权已通过。

