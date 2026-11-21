# M9-A：统一 Content Provider 与本地媒体源

**来源**：主分支 M8-B 已验收合并 `4f5e653`。本阶段只升级内容接入结构；
不改变 libmpv 引擎、M7 观看历史/收藏、M8 自动连播状态机或 M5 安装器。

## 交付接口

- `catalog_sources` 返回可用 Provider 及能力；初期包含 demo 和 local。
- `catalog_search_page` 接收 `sourceId`、`query`、`offset`、`limit`；
  搜索最长 128 字节，分页大小 1–50，错误返回明确代码前缀。
- `catalog_detail` 和 `catalog_play_episode` 继续沿用原有 IPC 命令名，
  `Series.id` 升级为 Provider 命名空间 ID（demo:.../local:...）。
  旧的未带前缀演示 ID 继续兼容。
- Rust Content Provider 只描述内容，并返回 `MediaTarget`；真正播放依然
  由现有 Rust `PlayerService` 独占原生 libmpv。外部 URL 再次通过安全验证。
- 本地适配器只读取用户已选择的媒体条目，不扫描或复制其他磁盘文件；
  本地 `catalog_play_episode` 最终调用现有 `open_library`，保留 M7 断点续播。
- 同名内容不同 Provider ID 不能互相解析，未授权媒体 URL 不接受。
- 内容剧库增加可见的来源筛选、分页/加载更多与刷新入口；
  之前的“我的视频”“最近播放”“我的收藏”功能完全保留。

## CI
- [ ] Windows M1：TypeScript、Rust 格式/单元测试、桌面构建全部通过。
- [ ] Windows M2：真实视频/音频、原生窗口关闭/资源回归全部通过。
- [ ] Rust 测试：Provider 命名空间碰撞、搜索分页/边界、无效来源、
      本地媒体路由、既有 demo 语义。

## Windows 实机验收
- [ ] 剧库默认显示演示内容；搜索、详情及播放演示 HTTPS MP4 正常。
- [ ] 切换为“本地媒体”，显示此前在“我的视频”导入的 MP4，不出现未导入的文件。
- [ ] 本地媒体搜索标题/文件名、进入详情和单集播放正常。
- [ ] 已观看本地视频从既有进度继续；收藏与最近播放仍然保留。
- [ ] 选择“全部内容源”可以同时看到 demo 和 local，不混淆同名内容。
- [ ] 删除或移动原始 MP4 后，有明确不可用提示，不使其他内容源崩溃。
- [ ] “刷新内容”展示新导入/移除的媒体；无结果提示正确。
- [ ] M8 自动连播以及 M7 播放历史/收藏不回归。

## 非目标和安全边界
- 本阶段**不**接入红果等第三方平台，不进行抓取、绕过鉴权或 DRM，
  也不提供缺少授权证据的媒体索引。
- 本阶段不做多窗口、跨进程数据库同步、插件动态加载、WebView 向本地文件的路径泄露。
- 不发布公共安装器；Issue #7 的 GPL/LGPL/FFmpeg 对应源码与干净 Windows
  环境验收仍是独立阻断项。
