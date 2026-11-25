# GitHub Copilot 指令（中文）

[English](copilot-instructions.md) | **简体中文**

这是一个 Soybean Admin + Rust Lambda + DynamoDB + CloudFront 后台底座。

## 先读

- `AI-PROMPT.zh-CN.md`（正式 Agent 提示词）
- `AGENTS.zh-CN.md`（操作规则）
- `docs/EXTENDING-ADMIN-PAGES.md`（新增页面强制手册）
- `docs/ARCHITECTURE.md`

## 不可协商规则

- 一个页面 = 专用 Rust 接口 + 明确的 DynamoDB 访问模式。
- 不要新增通用表/列查询接口、filter DSL、任意排序。
- 禁止 DynamoDB `Scan`、请求内 `Count`、页码/offset 分页。
- 列表使用不透明游标；响应 `{ items, next_cursor }`（无总数）。
- 过滤参数是白名单枚举；非法值 400。
- GSI 投影最小化；绝不投影密钥。
- 服务端 RBAC 才是权威；隐藏 UI 不是授权。
- 非 GET 请求必须带 `x-amz-content-sha256`；前端请求层已全局注入。
- 生产构建无密钥、demo、mock 或遥测。

## 参考文件

- 后端：`api/src/admin.rs`、`api/src/account_store.rs`、
  `api/src/dynamo_account_store.rs`、`api/src/auth.rs`
- 前端：`web/src/views/users/index.vue`、`web/src/service/api/users.ts`、
  `web/src/store/modules/auth/index.ts`
- 基础设施：`infra/template.yaml`、`infra/deploy.sh`、`infra/smoke.sh`

## 验证

```bash
cargo test --workspace
cargo build -p admin-kit-api
cd web && corepack pnpm typecheck && corepack pnpm lint && corepack pnpm build
```

如果需求需要任意查询/BI/JOIN/全文搜索，停下来提出关系型读模型
（Aurora/Postgres）、Athena 或 OpenSearch，不要把 DynamoDB 扩成 SQL 数据库。