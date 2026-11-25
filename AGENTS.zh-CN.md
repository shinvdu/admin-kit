# AGENTS.zh-CN.md — admin-kit

[English](AGENTS.md) | **简体中文**

本文件面向 AI 编码 Agent（Claude Code、Codex、Cursor、Copilot 等）。
先读 [`AI-PROMPT.zh-CN.md`](AI-PROMPT.zh-CN.md)（正式提示词与硬性规则），
本文件是简明的操作参考。

---

## 这个仓库是什么

admin-kit 是一个范围狭窄、接近生产实践的后台底座：

- **前端**：Soybean Admin `v2.3.0`（Vue 3 + Vite + TypeScript + Naive UI + UnoCSS）
- **后端**：Rust + axum + AWS Lambda Web Adapter（单 Lambda）
- **数据**：DynamoDB 单表 + 一条用于用户列表的 GSI
- **交付**：CloudFront + S3 + Lambda Function URL（OAC）
- **会话**：HttpOnly Cookie + `session_version` 即时撤销

唯一完整功能是**用户管理 + 认证**。把它当作参考模式，不要当作通用表浏览器。

---

## 仓库地图

| 路径 | 作用 |
|---|---|
| `api/src/main.rs` | 路由装配、store 选择、Lambda/本地启动 |
| `api/src/handlers.rs` | 登录 / me / logout / 改密 |
| `api/src/admin.rs` | 用户列表/创建/更新/统计（参考页面） |
| `api/src/auth.rs` | `SessionAuth` / `AdminAuth`，每次请求校验账号 |
| `api/src/session.rs` | Cookie 设置/清除 |
| `api/src/dynamo_account_store.rs` | DynamoDB 适配器、游标、条件更新 |
| `api/src/memory_account_store.rs` | 离线测试适配器 |
| `crates/core/` | 配置、Ed25519 JWT、AppError、axum/Lambda 启动 |
| `crates/dynamo/` | DynamoDB 薄封装（serde、分页、事务） |
| `web/src/views/users/` | 参考 Soybean 页面（列表 + 创建 + 操作） |
| `web/src/service/api/` | API client（唯一放置 URL 的地方） |
| `web/src/service/request/index.ts` | axios 适配器 + `x-amz-content-sha256` |
| `web/src/store/modules/auth/index.ts` | Cookie 会话状态机 |
| `infra/template.yaml` | CloudFormation：DynamoDB、S3、Lambda、CloudFront、IAM |
| `infra/deploy.sh` | 构建 + 部署 + 前端同步 + 健康检查 |
| `infra/smoke.sh` | 线上端到端冒烟测试 |
| `docs/EXTENDING-ADMIN-PAGES.md` | 新增页面的强制手册 |

---

## 常用命令

```bash
# 后端
cargo test --workspace
cargo build -p admin-kit-api

# 前端
cd web
corepack pnpm install --frozen-lockfile
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm build

# 本地后端（离线内存存储）
AWS_EC2_METADATA_DISABLED=true AWS_REGION=us-west-2 \
ADMIN_KIT_PROJECT=admin-kit ADMIN_KIT_ENV=dev \
ADMIN_KIT_DEV_JWT_SEED="$(openssl rand -base64 32)" \
ADMIN_KIT_DEV_ADMIN_EMAIL=admin@example.com \
ADMIN_KIT_DEV_ADMIN_PASSWORD=devpassword123 \
ADMIN_KIT_COOKIE_SECURE=false ADMIN_KIT_STORE=memory PORT=8090 \
cargo run -p admin-kit-api

# 本地前端（后端在 :8090 后）
cd web && corepack pnpm dev

# 部署（需要 .env 中的 AWS 凭证；会创建真实资源）
cp infra/params.example.json infra/params.json
./infra/deploy.sh

# 线上冒烟（需要已部署栈）
./infra/smoke.sh
```

---

## 硬性规则

1. **禁止通用查询 API。** 不要 `/api/admin/query?table=...`、任意 filter/sort DSL、SQL 仿真。
2. **禁止 Scan、请求内 Count、offset/页码分页。** 列表只能 GSI/基表 Query + 不透明游标。
3. **一个页面 = 专用接口 + 明确访问模式。** 写代码前填 Access Pattern Sheet
   （`docs/EXTENDING-ADMIN-PAGES.md` §2）。
4. **过滤参数是白名单枚举。** 非法值返回 400。非索引过滤必须标注“页内过滤”。
5. **排序固定或枚举化。** 每种排序对应确定的 SK/GSI SK。
6. **游标包含所有 `ExclusiveStartKey` 键**（pk、sk、gsi1pk、gsi1sk）。
7. **GSI 投影最小化。** 绝不投影密码哈希或大字段。
8. **服务端鉴权。** 前端菜单可见性不是授权。
9. **浏览器、localStorage、日志、仓库里都不放密钥。**
   不要记录邮箱、密码、Cookie 或 token。
10. **CloudFront 写请求需要 `x-amz-content-sha256`。**
    已由 `web/src/service/request/index.ts` 全局注入；组件里不要重复实现。
11. **生产构建无 demo/mock/Apifox；无第三方遥测。**
12. **HTML no-cache，带 hash 的 assets 长缓存。** hash 路由避免 SPA fallback。

---

## 新增页面的工作流

```text
1. 填 Access Pattern Sheet（页面/动作、Key、索引、排序、过滤、分页、写入）
2. 设计游标 + Key 布局
3. Rust：store port + 内存适配器 + Dynamo 适配器 + AdminAuth handler
4. 前端：API client + 类型 + Soybean 页面 + 路由 + i18n
5. 基础设施：如需要加 GSI，等待回填，IAM 只加 Query
6. 测试：cargo test --workspace；typecheck/lint/build；smoke
7. 报告：访问模式、文件、API、GSI 成本、测试证据、限制
```

参考实现：

- 后端：`api/src/admin.rs` + `api/src/dynamo_account_store.rs`
- 前端：`web/src/views/users/` + `web/src/service/api/users.ts`
- 基础设施：`infra/template.yaml`（`gsi1`）
- 冒烟：`infra/smoke.sh`

---

## 必须停下来问人的情况

- 需要任意查询/BI/JOIN/全文搜索；
- 一个实体即将增加第 4 条 GSI；
- 需要改认证/会话语义、表 Key 或租户隔离；
- 需要破坏性迁移或删除生产数据；
- 部署账号/域名/区域不明确。

---

## 完成定义

- Access Pattern Sheet 已填写并评审；
- Rust 测试 + 构建通过；
- 前端 typecheck/lint/build 通过；
- 没有引入 Scan/Count/通用查询；
- GSI/基础设施变更记录回填与成本影响；
- smoke.sh 覆盖新页面并清理测试数据；
- `web/dist` 无密钥/demo/mock；
- 最终报告包含已知限制。

如果需求与这些规则冲突，停下来解释取舍，不要偷偷在 DynamoDB 上实现 SQL 式查询层。