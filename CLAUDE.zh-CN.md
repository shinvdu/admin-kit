# CLAUDE.zh-CN.md — admin-kit

[English](CLAUDE.md) | **简体中文**

编辑前先读 [`AI-PROMPT.zh-CN.md`](AI-PROMPT.zh-CN.md) 和
[`AGENTS.zh-CN.md`](AGENTS.zh-CN.md)。

## 项目

Soybean Admin（Vue 3）+ Rust/axum Lambda + DynamoDB 单表 + CloudFront/S3。
唯一完整参考功能是用户管理 + 认证。新增页面必须遵循
[`docs/EXTENDING-ADMIN-PAGES.md`](docs/EXTENDING-ADMIN-PAGES.md)。

## 命令

```bash
cargo test --workspace
cargo build -p admin-kit-api
cd web && corepack pnpm typecheck && corepack pnpm lint && corepack pnpm build
```

## 规则

- 一个页面 = 专用 Rust API + 明确的 DynamoDB 访问模式。
- 禁止通用查询接口、filter/sort DSL、Scan、Count、offset 分页。
- 只使用游标分页；游标包含所有 ExclusiveStartKey 键。
- 过滤/排序是白名单枚举；非法值 → 400。
- 服务端 RBAC；前端隐藏不是安全。
- 生产构建无密钥/demo/mock/遥测；无 source map。
- 经过 CloudFront 的非 GET 请求需要 `x-amz-content-sha256`；
  已由 `web/src/service/request/index.ts` 全局注入。
- 不提交 `web/node_modules`、`web/dist`、`infra/params.json` 或凭证。

## 新增页面前

先填 `docs/EXTENDING-ADMIN-PAGES.md` §2 的 Access Pattern Sheet。
参考 `api/src/admin.rs` 与 `web/src/views/users/`。

如果需求需要任意查询/BI/JOIN/搜索、一个实体第 4 条 GSI、认证/会话变更
或破坏性迁移，停下来先问人。