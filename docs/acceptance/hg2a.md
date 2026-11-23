# HG-2A — 非官方开源 SDK 真实接口实验 Windows 验收

## 状态与约束

- 基线：HG-1 PR #29 HEAD `a036d65e6212bb324325423e3550ef952395d05d`，Windows M1/M2 全绿，用户明确反馈 Windows 10/10 实机全部通过。
- **HG-2A 不再把 Mock 当作主要目标**：使用开源 `openhongguo` Node.js SDK 接入真实红果目录/搜索/选集；Mock 只用于离线 CI。
- 这并非官方授权客户端；没有红果版权分发/公开 API 授权。若 SDK 遇到实名、付费、验证、限流或 CENC 保护媒体，则按实际状态报错，不绕过。
- 真实外部 API 请求只由用户自己在 Windows 中主动安装 SDK、启动开发脚本后发生。普通调试 EXE 和 Release 在默认情况下不会出现 `hgcommunity`；原来的 `hongguo` 待授权占位仍不可播放。
- 受保护的分集不能因 `openhongguo` 返回播放模型而算作本机已经可播放。允许验证无保护公开 HTTPS MP4，但必须明确允许其媒体主机；实际媒体来源权利仍应自行确认。

## 自动化门槛

- [ ] 同一最终 SHA Windows M1：TS/Node 全量测试（含 `tests/hg2a-community-gateway.test.mjs`）+ Rust fmt/tests + Tauri 编译全绿。
- [ ] 同一最终 SHA Windows M2：原生视频/音频与干净退出探针全绿。
- [ ] CI 用模拟的 SDK 对象验证桥接协议、错误码、令牌防护、非法 ID、受保护媒体禁播；**CI 不请求真实红果接口**。
- [ ] 真机 Windows 10 项测试均有结果记录，失败项不可标记通过。

## 实机 10 项（请逐项反馈）

- [ ] 1. 不设置任何 HG-2A 环境变量直接启动：无「社区接口实验」来源；HG-1「红果接入（待授权）」依然显示空结果、不可播放。
- [ ] 2. 安装专属 SDK 依赖 `npm install --prefix .\scripts\hg2a-sdk --ignore-scripts`，未修改根应用 `package.json` 或历史/收藏数据；来源包为 `openhongguo@0.4.0`。
- [ ] 3. 安装本阶段 Windows Debug EXE，运行 `scripts/start-hg2a-community.ps1`，启动器生成新随机 token、Node 服务只绑定 127.0.0.1:19091，UI 显示单独的「社区接口实验」源，并保持非官方说明。
- [ ] 4. 实际执行空关键词目录加载：若上游成功返回，出现**真实 SDK 目录元数据**；若上游拒绝/限流/超时，则显示明确错误，不自动展示虚假测试剧集。记录实际结果，不伪造成功。
- [ ] 5. 用你有权查询的公开剧名实际搜索，点击真实详情：真实分集 ID、集数、排序和剧名匹配，未知作品有合理空结果；搜索取消/快速刷新不复用旧结果。
- [ ] 6. 默认未设置 `-AllowedMediaHosts` 时，真实分集**不得播放**；Rust 原生门禁拒绝 `hgcommunity` 播放目标，原生 `hongguo` 仍然始终禁播。
- [ ] 7. 对有合法使用权、**未加密且为直接 HTTPS MP4** 的测试分集，明确指定其允许的真实 CDN 主机后，可验证播放地址解析；遇到 DRM/HLS/DASH/需登录/请求头限制时必须拒绝，不试图解密或强行重试。
- [ ] 8. 上游出现匿名设备限制、签名变动、HTTP 429 或断网，UI 有错误状态并可手动恢复；不会产生无限重试/设备轮换、不会泄漏设备标识、内容密钥、Cookie 或带签名 URL 的原始异常文本。
- [ ] 9. 关闭 HG-2A 客户端后 Node 子进程同步退出；普通启动无任何 HG-2A 后台请求，用户历史文件与 M7–M12-C 观看数据不受影响。
- [ ] 10. 演示/本地/自有 JSON、收藏历史、选集控制、进度条缩略图、音轨字幕、M12 诊断及损坏媒体恢复均无回归；“全部来源”若碰到社区源不可用，不得静默把错误当真实播放数据。

## Windows 运行步骤

在用户已克隆的仓库根目录，且 Node.js 22+/24 与 HG-2A Debug EXE 均准备好后：

```powershell
npm install --prefix .\scripts\hg2a-sdk --ignore-scripts
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\start-hg2a-community.ps1
```

脚本默认寻找 `%LOCALAPPDATA%\HongGuoRust-HG2A-Test\hongguorust.exe`；如你把调试 EXE 放在其他独立目录，用 `-ExePath "绝对路径"` 指定。脚本自动生成本机连接 token，子进程不会写入用户真实红果账号；需要主动关闭播放器才能关闭实验服务。

**真实播放不是本阶段一律通过的门槛**：如果没有确认可以直接播放的无保护 MP4，记录“来源返回受保护或非直链，HG-3 待授权”，不应强迫采用第三方解密实现或给出虚假 PASS。真实内容访问受上游服务条款和实际许可约束。

## 发布与合并

PR #29 HG-1 用户已通过验收，但仍 Draft/未合并；本 HG-2A 仍是新的 Draft PR，与前置 #20–#29 的依赖和明确授权要求一致。Issue #7 许可证与正式分发仍未解除。开发调试版本不是可公开发行的红果客户端。

更多参考：[HG-2A 开源实现调研与适配方案](../architecture/hg2a-community-sdk-integration.md)。
