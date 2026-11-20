# GPL-3.0-only 选型记录

结论：项目原创代码采用 GPL-3.0-only。Tauri 的 MIT / Apache-2.0、React 和 Vite 的 MIT、TypeScript 的 Apache-2.0 允许按相应条件与 GPLv3 项目组合。保留第三方版权及许可声明；分发时核对锁定版本的传递依赖和所需 notices。

M2 再决定 libmpv 集成路径。MPL-2.0 插件与 GPLv3 代码组合时，核对每个采用的文件是否被标记为 Incompatible With Secondary Licenses 及其历史，并满足 MPL 3.3 的额外分发条件。保留原文件声明。LGPL-2.1 绑定和 wrapper 可按 LGPL 链接规则组合，或对符合条件的库副本按第 3 节选择 GPLv3；结合实际链接与分发方式审查对应源码及重新链接义务。

mpv 默认 GPL-2.0-or-later，可选择与 GPLv3 兼容的版本。LGPL 构建须核对 mpv、FFmpeg 与打入 DLL 的全部组件及配置。含 GPL-2.0-only 且不能取得兼容授权的组件不能纳入本项目的组合作品。M1 只引入桌面框架和前端依赖，未引入播放器二进制。

依据：[GNU 许可证兼容表](https://www.gnu.org/licenses/gpl-faq.html#AllCompatibility)、[Mozilla MPL FAQ Q14/Q25](https://www.mozilla.org/en-US/MPL/2.0/FAQ/)、[mpv Copyright](https://github.com/mpv-player/mpv/blob/master/Copyright)、[Tauri](https://github.com/tauri-apps/tauri)、[React](https://github.com/facebook/react)、[Vite](https://github.com/vitejs/vite)、[TypeScript](https://github.com/microsoft/TypeScript)。
