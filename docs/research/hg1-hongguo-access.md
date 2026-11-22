# HG-1｜HongGuoRust 红果短剧内容源接入：事实核查与工程契约

> 状态：**HG-1 接口调研与未授权适配入口开发中**。本文只描述已公开验证的资料与本仓库实现，不代表红果官方合作、拥有片源、具备 API 调用权限或可读取红果全站剧集。核查基线：2026-11-21～22。

## 一、已经核实的接口与边界

1. 抖音开放平台的**微短剧行业解决方案**面向短剧商家，围绕抖音小程序的剧集接入、审核、播放、授权分销及商业变现展开。要求相关行业准入/资质，**不等于**向普通 Windows 桌面软件开放所有红果短剧搜索、目录和播放。
   - 官方：https://developer.open-douyin.com/docs/resource/zh-CN/mini-app/open-capacity/solutions/vertical/micro-drama
2. 抖音云**短剧媒资管理**明确说明，指定行业的小程序需提交工单并开通能力，且仅限相关抖音云环境；在具备授权及权限的场景，可以通过 OpenAPI 生成**小程序外**的播放链接。
   - 官方：https://developer.open-douyin.com/docs/resource/zh-CN/developer/tools/cloud/industry-solutions/short-drama-media-management
3. 官方**媒资管理 OpenAPI**具有 `/api/dyc_voc/get_video_list` 与 `/api/dyc_voc/get_video_by_vid` 等接口，要求相关 `cloud.playlet` 权限及 `access-token`，指定视频可返回播放 URL 与过期时间。根据其使用限制，这些是**获准小程序/云端的媒资接口**，不能推定为红果全站节目列表的公开接口。
   - 官方：https://developer.open-douyin.com/docs/resource/zh-CN/developer/tools/cloud/api-reference/short-drama-media-api
4. **重要差异**：文档示例视频地址可能使用 HTTP、包含过期参数；现有播放器要求直连的**公开 HTTPS 且路径以 .mp4 结尾**。所以即使获得官方 API，也不能未经审查就将原始临时链接塞入 libmpv。必须先审计 HTTPS/URL/访问权限/格式/到期刷新/是否 DRM，并决定是否需授权的服务端媒介层。
5. 现阶段**没有验证到**允许独立第三方 Windows 客户端匿名调用的红果全站短剧搜索 + 选集 + 合法播放 API。第三方“红果 API”页面、聚合网站、爬虫仓库的存在不等于取得版权或平台 API 使用许可。不能用私有端点、绕过登录/付费限制、提取用户 Cookie、逆向 DRM 或批量抓取来替代正式准入。

## 二、HG-1 已提交的实际代码

- Rust `src-tauri/src/catalog/hongguo.rs`：注册 `hongguo` 命名空间，Provider 名称为 **「红果接入（待授权）」**。生产构建默认没有剧集数据，`supportsSearch/Detail/Play = false`，任何选集解析都返回 `hg_not_authorized`，**无网络、无授权密钥、无播放源**。
- Rust `src-tauri/src/catalog/registry.rs`：现有 Provider registry 支持每种来源独立声明能力；原 demo/local/manifest 仍为 true，红果未授权来源均为 false。搜索、详情、取消、分页、跨来源唯一 ID 的既有契约保持不变。聚合搜索中红果不会伪造剧集。
- Rust `hongguo.rs` 内部 `schemaVersion=1` **元数据归一化契约（本项目自行设计，并非官方响应结构）**：`series[{id,title,genre,episodes[{id,title,number}]}]`，限制字节数、ID、文本、剧集数量和序号，拒绝未知字段/播放 URL/鉴权参数、重复 ID。使用**合成测试数据**验证搜索、详情、选集排序及**不可播放**边界。
- React 内容剧库加入「红果接入」入口、下拉来源及明确的“尚未授权”提示。不允许将合成测试剧冒充真实红果短剧，也不展示虚构排名/封面/片源。
- 以上属于**真实可编译的适配框架与安全前置门槛**，不等于 HG-2/HG-3 真实内容服务接通。

## 三、接入真实红果内容必须满足的条件

- **接口与主体**：确认可申请的官方或明确授权的服务商 API，业务主体、访问凭据、请求配额、限流、连接/响应超时、错误码与取消行为。
- **目录权利**：确认授权范围覆盖内容元数据、封面、剧集列表、具体剧目/集数、地域及缓存。没有授权的作品不能直接展示为可播放。
- **媒体权利**：确认 Windows 桌面端播放、来源域名、DRM、需否官方 SDK、转码/临时播放链接及过期刷新和跳转策略。HTTP / HLS / DASH / DRM 不属于当前直接 HTTPS MP4 放行规则。
- **客户端凭据策略**：长期 `client_secret`/合作方 access-token 不应硬编码进入 GitHub 或客户端 DLL/EXE。若官方协议允许，应由用户自己控制的服务端管理授权、签名、短时令牌及最小权限。禁止把登录 Cookie 或密钥写入日志、历史、收藏、诊断报告。
- **验收**：一旦取得书面或其他可核实的授权，可在单独 HG-2 PR 将受控、授权的元数据服务映射到本地 `Series/SeriesDetail`；HG-3 才实施真实链接解析和播放，并以可核实的权利/协议为合并门槛。

## 四、下一阶段选择（不阻塞代码开发）

**HG-2A（现在就能开发）**：受控模拟服务 + Provider 搜索、详情、分页、取消、授权态与故障回退；可用纯合成数据，确保架构稳定，未接入真实片源时绝不显示为官方数据。

**HG-2B（待授权）**：实际合作接口和服务端令牌管理；针对准确文档编写专用转换器和 SDK/HTTP 客户端，不猜测接口字段、不假设视频地址长期有效。

**HG-3（待授权且经实机验证）**：真实剧目首页、搜索、详情、选集与播放，继承 M7–M12 的断点续播/收藏/错误恢复；不绕过官方 DRM、鉴权或付费机制。

## 五、当前结论

HongGuoRust 的底层播放和 Provider 框架已足以承接合法的受控内容源。**阻碍真实红果剧集播放的主要是获取满足 Windows 平台播放权利的内容/API 授权，而不是缺少播放器功能**。因此下一阶段应优先研发授权 API 对接能力，而非继续扩展 M12 之后的媒体底层。