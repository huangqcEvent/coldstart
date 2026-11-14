## 改动说明

<!-- 说明动机、用户可见变化和实现边界。 -->

## 关联 Issue

<!-- 例如 Closes #123；没有 Issue 时说明背景。 -->

## 验证

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] `pnpm --dir web typecheck`
- [ ] `pnpm --dir web test`
- [ ] `pnpm --dir web build`

补充验证命令与结果：

```text

```

## 风险与检查

- [ ] 改动聚焦，未混入无关重构或格式化
- [ ] 已补充或更新测试
- [ ] 已更新相关文档
- [ ] 未提交真实凭据、客户信息、数据库、日志或构建产物
- [ ] 数据迁移、安全边界、MCP 契约或破坏性变化已在描述中说明
