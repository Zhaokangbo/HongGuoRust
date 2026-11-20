# 参与开发

从 main 创建功能分支。M1 使用 feat/m1-scaffold，M2 使用 feat/m2-player。按独立改动提交，采用英文 Conventional Commits，例如 `feat: expose the initial player snapshot`。

提交前执行与改动相关的检查。Rust 改动运行格式检查与单测，前端改动运行类型检查与 Vite 构建。Pull Request 应列出变更、实测命令、结果及尚未验证的条件。Windows CI 通过并达到里程碑验收条件后方可合并。M1 标签为 phase1-m1。

仅提交有权公开的源码和文档。提交前检查暂存差异、依赖来源和许可证。凭据、Cookie、本机配置、媒体和构建产物须留在仓库外。
