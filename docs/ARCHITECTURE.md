# 架构

## 总览

```text
Browser
  │ HTTPS
  ▼
CloudFront
  ├─ /api/*    → Lambda Function URL (OAC, SigV4)
  ├─ /health   → Lambda Function URL
  ├─ /assets/* → S3 (immutable long cache)
  └─ default    → S3 index.html (no-cache)

Rust Lambda (admin-kit-api, axum + Web Adapter)
  │
  ├─ HttpOnly Cookie session (Ed25519 JWT)
  ├─ AdminAuth / SessionAuth (server-side RBAC)
  └─ DynamoDB single table
        ACCOUNT#user:<uuid> / META
        EMAIL#<normalized_email> / META
        gsi1pk = "USERS"
        gsi1sk = "{created_ms:013}#user:<uuid>"
```

## 为什么是单 Lambda

- 后台流量低，冷启动/并发压力小；
- 一个二进制同时支持本地 `PORT` 与 Lambda Web Adapter；
- 认证、用户管理、未来的运营页共享同一套 `AppCtx` 与错误/日志约定；
- 需要隔离时，可以拆出独立 admin Lambda，但不要先拆。

## 认证与会话

- 登录：`POST /api/auth/login`，邮箱 + argon2id 密码校验；
- 会话：Ed25519 签名的 JWT 存在 HttpOnly Cookie `admin_session`；
- 每次请求对 `user:` 会话做一次强一致账号读取：
  - 账号存在；
  - 未禁用；
  - `claims.sv == account.session_version`；
  - role/email 以数据库为准；
- 禁用、改密码、改角色都会 `session_version + 1`，旧 Cookie 在下一次请求立即 401；
- 管理员引导：SSM `admin_email` + `admin_password`，仅当邮箱不存在时创建 admin；
  已存在但 role=user 时**不会**自动提权。

## 数据模型

```text
ACCOUNT#user:<uuid> / META
  id, owner, email, password_hash (argon2id), role (admin|user),
  disabled, session_version, created_at(ms), updated_at(ms)
  gsi1pk = "USERS", gsi1sk = "{created_ms:013}#user:<uuid>"

EMAIL#<normalized_email> / META
  owner, created_at
```

- 创建账号：`TransactWriteItems` 同时写 `ACCOUNT#` + `EMAIL#`，条件 `attribute_not_exists(pk)`；
- 邮箱唯一性由 `EMAIL#` 项保证；
- 后台列表走 GSI1，倒序创建时间；
- 不保存明文密码；GSI 投影不包含 `password_hash`；
- 不做 `Scan`；不在请求路径做全量 `Count`。

## API 设计

- 每个页面一组专用接口；
- 列表：`limit` + 不透明 `cursor`；响应 `{ items, next_cursor }`；
- 过滤/排序白名单枚举；非法值 400；
- 写操作返回实体；冲突/权限使用明确 code；
- 错误：HTTP 状态码 + JSON（前端兼容 nested error）。

详见 [`API.md`](API.md)。

## 前端

- Soybean Admin `v2.3.0`，Vue 3 + Vite + TS + Naive UI + UnoCSS；
- hash 路由（`/#/users`），避免 CloudFront/S3 的 history fallback；
- Cookie 会话；请求层 `withCredentials: true`；
- 非 GET 请求自动注入 `x-amz-content-sha256`（CloudFront → Lambda 必需）；
- HTML no-cache，`/assets/*` 长缓存；
- 菜单权限只是展示，服务端 `AdminAuth` 才是边界。

## 基础设施

- DynamoDB：按需计费 + PITR + GSI1；
- S3：私有桶 + OAC + bucket policy；
- Lambda：provided.al2023 / x86_64 / 256MB / Timeout 55 / Web Adapter；
- Function URL：`AWS_IAM`，仅 CloudFront OAC 可调用；
- CloudFront：默认域名（无 ACM），两条 Lambda behavior + 两条 S3 behavior；
- SSM：`/admin-kit/{env}/jwt_seed`、`admin_email`、`admin_password`；
- IAM：最小权限，无 `dynamodb:Scan`。

## 关键决策

| 决策 | 选择 | 理由 |
|---|---|---|
| 后台与产品前端 | 分开构建/部署 | 依赖隔离、独立发布、避免模板污染主站 |
| 认证 | Cookie + JWT | 不把 token 放进 localStorage；同源最简单 |
| 权限 | 服务端 RBAC | 前端隐藏不是安全边界 |
| 列表 | 游标分页 | DynamoDB 不支持廉价 offset/总数 |
| 查询 | 专用 API + 明确 GSI | 避免 SQL 式通用查询把成本放大 |
| 报表 | Export → S3 → Athena | 不在 Lambda 内 Scan |
| 部署 | 独立栈 `admin-kit-{env}` | 不碰其他业务栈 |

## 已知限制

- i18n 目前 `zh-CN` / `en-US`；
- 未实现登录限流（公网暴露时需 WAF/限流）；
- 无自助注册/找回密码/邮件验证；
- 无自定义域名（默认 CloudFront 域名）；
- 仅用户管理参考页；新增页面按 [`EXTENDING-ADMIN-PAGES.md`](EXTENDING-ADMIN-PAGES.md) 扩展。