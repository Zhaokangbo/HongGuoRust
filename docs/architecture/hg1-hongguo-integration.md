# HG-1：红果短剧接入可行性与授权 Provider 架构评估

## 项目目标

HongGuoRust 是独立 Windows Rust/Tauri + libmpv 客户端，当前支持 demo、local、manifest 三个内容源，以及首页、搜索、详情、选集、历史/续播和播放器功能。

本阶段从 M12-C **已通过用户 Windows 10/10 实机验收**的 PR #28 HEAD `8067f881a12f624413eedd8895b851d1bb05a551` 开始，优先解决“何时真正接入红果”这个目标，而不是继续增加播放器功能。

**本阶段 HG-1 的正确交付是：研究真实 API 及授权边界、落地可编译/可测试的独立 Provider 合约和模拟链路。不是声称已经取得红果用户剧集数据或播放授权。**

## 截至 2026-11-21 的公开资料结论

1. **抖音开放平台的微短剧行业方案存在。** 它面向微短剧商家，服务抖音小程序内的短剧内容接入、审核、备案、播放、授权分销等。这不等于任何第三方 Windows 客户端有权获取红果全站剧集。
   - 官方：https://developer.open-douyin.com/docs/resource/zh-CN/mini-app/open-capacity/solutions/vertical/micro-drama
   - 微短剧类目可涉及特定资质、主体申请、审核与 ICP 备案，不应假设个人开发项目自动具备。
2. **官方抖音云短剧媒资 OpenAPI 包含指定 VID 获取播放链接。** 官方限制：小程序开通抖音云和短剧解决方案、已完成短剧域名绑定；需要 `cloud.playlet` 权限及有效的服务端访问令牌。此接口属于**满足权限条件的云端媒资**，不能据此推断可检索或播放红果平台上的任意版权剧集。
   - 官方：https://developer.open-douyin.com/docs/resource/zh-CN/developer/tools/cloud/api-reference/short-drama-media-api
3. **官方短剧媒资 FAQ 说明：托管在抖音云的视频支持在其他 App/小程序外端播放，须通过 OpenAPI 生成播放链接。** 对“在自己的 Windows 客户端播放自己有权使用的媒资”是可进一步咨询的技术方向，仍需核实具体授权条款、资源归属、客户端调用模式及鉴权架构。
   - 官方：https://developer.open-douyin.com/docs/resource/zh-CN/developer/tools/cloud/faq/short-drama-faq
4. **社区第三方红果客户端并非授权证明。** 开源仓库公开的搜索/播放实现可能依赖非官方、动态变化的设备或签名接口。其源码许可证通常不授予电视剧作品版权、平台 API 调用权或 DRM 绕过权，也不能保证持续可用。仅把社区项目作为 UX/架构风险参考，不纳入未经验证的请求代码、设备签名或 token 提取。
   - 非官方示例：https://github.com/kyrqon/hongguo-web
   - 非官方示例：https://github.com/HankLiy/guoapp
5. **当前确认状态**：没有验证到允许普通第三方 Windows 桌面应用直接获取“红果全站剧集检索/详情/播放流”的通用官方接口与授权条款。对该能力标记为 **未确认/未接入**；不使用名称暗示官方合作。

## HG-1 已落地的架构边界

- 新增 `src-tauri/src/catalog/hongguo.rs`：`ApprovedCatalogTransport` 抽象搜索、详情和按剧集 ID 解析播放地址；`HongGuoAuthorizedProvider` 实现现有 `ContentProvider` 接口。另加 `HongGuoUnavailableProvider` 供生产端只显示明确的待授权入口，两种 Provider 不能同时被生产注册。
- 采用 **注册不可播放占位来源，但不注册获授权 Transport** 的策略：当前生产 `CatalogService::new` 在 `demo/local/manifest` 外新增 `hongguo` 占位 Provider，名称为 **「红果接入（待授权）」**，其 `supportsSearch/Detail/Play` 均为 `false`、内容集合为空、解析播放请求必然拒绝。**`HongGuoAuthorizedProvider` 及其模拟 `ApprovedCatalogTransport` 仅用于 Rust 离线测试，不注册生产，不产生真实片源请求。**
- 新增输入与输出校验：ID 限定 ASCII 字母数字/下划线/横线（避免路径、冒号命名空间冲突）；搜索字节长度上限；剧集数量上限 200、结果条数上限 50；剧集编号/标题/数量一致性、重复 ID 拒绝。
- `allowed_media_url` 先沿用项目的 `public_https_url` 验证直接 HTTPS MP4，再校验完整主机名必须精确匹配合作方书面确认的媒体域名。不会接受用户可随意提供的 URL 作为集数 ID、HTTP、用户信息、内网 IP、伪域名和错误扩展名。
- 错误对外只有预定义错误码/说明，不暴露上游异常文本、令牌、带签名的播放 URL 或本地用户路径。
- 新增 Provider 能力声明及 Rust `CatalogService::resolve_episode` 强制禁播门控：不能只靠前端禁用按钮保证权限。前端提供「红果接入」入口、待授权状态和安全空结果，不展示虚构真实作品。**尚未实现 HTTP transport、真正登录、OAuth、票据交换、内容授权审核或使用真实红果内容。** 为避免安全错觉，未来网络实现必须另行覆盖 DNS 重绑定/重定向/私网 IP 防护、连接/读取超时、请求取消、凭证只放服务端和 token 轮换；本阶段没有假装已满足这些要求。
- `resolve_episode` 返回已有 `MediaTarget::Https`，最后仍交给现有 Rust `CatalogService` 和 `PlayerService/libmpv` 处理，Provider 无权直接控制 native 播放器。
- 此实现只兼容直接 HTTPS MP4。如果未来授权源只提供 HLS/m3u8、DRM 或过期签名 URL，则必须作为新需求单独设计，不能将不同格式 URL 直接塞进当前 MP4 校验链路。

## 需要向官方/合作方明确的接入信息

1. 是否向**独立第三方 Windows 桌面客户端**提供可获许可的红果短剧检索、详情和选集 API？还是只面向抖音小程序及商家自有媒资？
2. 若支持，书面的 API/内容许可范围、可访问剧目、用户地域/有效期限制、非商用测试条件、对外分发规定是什么？
3. 认证应采用服务端 OAuth/client token 还是用户授权码？Windows 桌面程序不能内置 `client_secret`；是否有可使用的授权测试环境/沙盒？
4. 允许的媒体格式、CDN 主机名、播放地址过期时间、跳转行为、签名 URL 日志处理、并发/频率限制、DRM 播放要求是什么？
5. 提供沙盒账号与合成/经授权剧集的搜索/详情/选集/播放响应、错误码、分页协议吗？

**只有获得接口及可用内容许可，才会从 HG-1 过渡到 HG-2（协议适配/实机契约测试）与 HG-3（受控真实内容播放）。**

## 建议后续子阶段

- **HG-1（本阶段）**：公开事实调研、接入条件清单、Rust Provider 安全适配骨架、合成 Mock 全链路单测、仅注册默认禁用的红果入口，不注册真实红果内容服务。
- **HG-2**：若合作/授权接口可用，创建具备明确许可的远端 `ApprovedCatalogTransport`，受控 HTTP 客户端及服务端凭证代理，完成真机数据检索/详情/选集的测试；若缺少授权，先建设与相同 contract 一致的本地 Mock/自有云内容源，但清晰标注“非红果真实内容”。
- **HG-3**：在播放权和真实媒体目标格式确认后，安全接入 `MediaTarget`，处理 token 有效期、过期重试、域名重定向校验、禁止 DRM/付费墙绕过，完成合法剧目实际播放验收。
- **HG-4**：整合真实可用剧目至首页、分类、历史续看、收藏与跨重启；补齐 Provider 作用域与版权来源标注。
- **M13**：保持原有第三方 DLL 开源许可证及公开发行合规 Issue #7 阻断，实机功能正常不代表可公开分发。

## HG-1 验收条件

- [ ] 同一 HEAD Windows M1（Rust fmt/tests、Tauri 构建、TS/Node）通过；
- [ ] 同一 HEAD Windows M2（真实原生播放/正常退出探针）通过；
- [ ] Rust 模拟适配单测通过：合成作品搜索、详情选集排序、已知 ID 解析、非法 ID、恶意媒体 URL、不同域名/端口、异常数据拒绝；
- [ ] 只新增 `hongguo` **空占位来源**，生产不注册获授权的 Mock Transport，不联网、不存储真实账号/token、不展示真实剧集、不回归 demo/local/manifest 与播放链路；
- [ ] 负责人 Windows 适配回归与产品流程验收完成，按后续 HG-1 对应验收项记录；
- [ ] 前置 PR #20–#28 未合并前仍需原有独立门槛与明确授权。

**此文件所列官方链接是公开技术资料，不是向本项目发放的接口凭证、许可证或内容分销授权。**
