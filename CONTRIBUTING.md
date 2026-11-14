# 参与贡献

感谢你愿意参与 ColdStart。无论是报告问题、补充文档、改进交互还是提交代码，都欢迎先通过 GitHub Issue 说明背景和预期。

## 报告问题

- 先搜索现有 Issue，避免重复。
- 提供 ColdStart 版本、Windows 版本、复现步骤、预期行为和实际行为。
- 日志、截图和配置片段必须先脱敏，不要上传真实 Token、密码、私钥、客户名称或业务数据。
- 安全漏洞不要公开提交，请按 [SECURITY.md](SECURITY.md) 的流程私下报告。

## 提交代码

1. Fork 仓库并从 `main` 创建短分支，例如 `fix/plan-lease-timeout`。
2. 保持改动聚焦，不混入无关格式化、依赖升级或功能重构。
3. 行为变更应先补测试，再实现；Bug 修复应包含可复现的回归测试。
4. 提交 PR 前运行相关验证，并在 PR 中写明验证命令与结果。
5. PR 描述说明动机、边界、风险和需要人工确认的事项。

## 开发环境

- Windows 10/11
- Rust 1.89 或更高版本
- Node.js 与 pnpm 11
- WebView2

```powershell
pnpm install
cargo build -p coldstart-mcp --release
pnpm --dir apps/desktop dev
```

完整开发与验证命令见 [README.md](README.md)。

## 项目约定

- 文档使用中文，代码标识符和协议字段使用英文。
- 第一方生产代码必须保留模块职责、边界和关键不变量的说明。
- 不提交真实凭据、客户信息、数据库、日志、构建产物或本地项目目录。
- 不绕过验证门禁、AI Review 或 MCP 的凭据隔离边界。
- 对外行为、数据迁移和安全边界变化必须同时更新文档与测试。

## Pull Request 检查清单

- [ ] 改动范围与 Issue 或需求一致
- [ ] 已补充或更新测试
- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 通过
- [ ] `cargo test --workspace --locked` 通过
- [ ] `pnpm --dir web typecheck`、`pnpm --dir web test`、`pnpm --dir web build` 通过
- [ ] 文档、截图和日志不含敏感信息

提交即表示你同意以仓库采用的 MIT License 授权你的贡献。
