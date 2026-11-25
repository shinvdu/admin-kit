# admin-kit

[English](README.md) | **简体中文**

[![CI](https://github.com/shinvdu/admin-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/shinvdu/admin-kit/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> ## 🤖 AI Agent 从这里开始
>
> **先读 [`AI-PROMPT.zh-CN.md`](AI-PROMPT.zh-CN.md)，再读 [`AGENTS.zh-CN.md`](AGENTS.zh-CN.md) 和 [`docs/EXTENDING-ADMIN-PAGES.md`](docs/EXTENDING-ADMIN-PAGES.md)。**
>
> 把下面这段提示词复制给 Agent：
>
> ```text
> 你是一个 AI 编码 Agent，正在 admin-kit 仓库工作：
> Soybean Admin + Rust Lambda + DynamoDB + CloudFront 后台底座。
>
> 写代码前：
> 1. 阅读 AI-PROMPT.zh-CN.md、AGENTS.zh-CN.md、docs/AI-USAGE.md、
>    docs/EXTENDING-ADMIN-PAGES.md。
> 2. 新增任何页面前，先填写 Access Pattern Sheet（页面/查询/Key/索引/排序/
>    白名单过滤/分页/写入）。
> 3. 参考 api/src/admin.rs 与 web/src/views/users/。
>
> 硬性规则：
> - 一个页面 = 专用 Rust API + 明确的 DynamoDB 访问模式。
> - 禁止通用“任意表/任意列查询”接口、filter DSL 或 Scan。
> - 列表使用不透明游标分页；不要页码，不要总数。
> - 排序固定或枚举化；每种排序必须对应已知 Key/GSI。
> - 鉴权/RBAC 必须在服务端；前端隐藏不是安全。
> - 经过 CloudFront 的写请求必须带 x-amz-content-sha256。
> - 不要有密钥、demo 账号、mock 数据或第三方遥测。
>
> 交付物：Access Pattern Sheet、Rust handler/store、Soybean 页面 + API client、
> 测试、CFN GSI 变更（如需要），以及 smoke/验证报告。
> ```

**一个可部署、对 AI 友好的后台底座** —— 前端 Soybean Admin，后端 Rust (axum) Lambda，DynamoDB 单表，S3 + CloudFront 交付，HttpOnly Cookie 会话。

它刻意保持范围狭窄：**用户管理 + 认证**就是完整的参考实现。新增后台页面时，使用专用 API + 明确的 DynamoDB 访问模式，而不是一个“通用数据库客户端”。

---

## 为什么做这个项目

大多数后台模板假设底层是关系型数据库，并提供通用查询层（`filter[anything]`、`sort=any`、`page=N`、`COUNT(*)`）。DynamoDB 不是这样工作的。

admin-kit 演示的是一套更接近生产实践的方案：

- **每个页面一组专用接口**（`/api/admin/users`），而不是 `/api/admin/query?table=...`；
- **显式 Key/索引设计**（基表 Query，或每个访问模式一条 GSI）；
- **不透明游标分页**；不伪造页码，不返回总数；
- **服务端 RBAC**；前端菜单只是展示；
- **会话即时撤销**：禁用/改密码/改角色时递增 `session_version`；
- **AI-first 文档**：提示词、Agent 规则、页面扩展手册。

如果你需要任意即席查询、BI 或跨实体 JOIN，请增加关系型读模型（Aurora/Postgres），或使用 DynamoDB Export → S3 → Athena。不要把 DynamoDB 硬掰成 SQL 数据库。

---

## 功能

- **前端**：Soybean Admin `v2.3.0`（Vue 3 + Vite + TypeScript + Naive UI + UnoCSS）
- **后端**：Rust + axum + AWS Lambda Web Adapter（单 Lambda）
- **数据**：DynamoDB 单表（`ACCOUNT#` / `EMAIL#`，后台列表用 GSI）
- **交付**：CloudFront → S3 静态前端 + Lambda Function URL（OAC）
- **认证**：邮箱 + 密码，argon2id，HttpOnly Cookie，30 天会话
- **后台**：Dashboard、游标分页用户列表、创建用户、改角色、禁用/启用、重置密码
- **即时撤销**：禁用/改密/改角色后，下一次请求即让旧会话失效
- **管理员引导**：SSM `admin_email` + `admin_password`，首次登录创建管理员
- **CloudFront 安全写请求**：前端自动附加 `x-amz-content-sha256`
- **明亮/暗色主题**、响应式布局、`zh-CN` / `en-US`
- **基础设施即代码**：DynamoDB、S3、Lambda、Function URL、CloudFront、SSM、IAM
- **端到端冒烟脚本**：登录 → me → 列表 → 创建 → 禁用 → 撤销 → 登出

---

## 架构

```text
浏览器
  │  HTTPS（CloudFront 默认域名）
  ▼
CloudFront
  ├─ /api/*    → Lambda Function URL（OAC，SigV4）
  ├─ /health   → Lambda Function URL
  ├─ /assets/* → S3（长缓存）
  └─ 默认       → S3 index.html（no-cache）

Rust Lambda（admin-kit-api）
  │
  ▼
DynamoDB 单表
  ACCOUNT#user:<uuid> / META
  EMAIL#<normalized_email> / META
  gsi1pk = "USERS", gsi1sk = "{created_ms:013}#user:<uuid>"
```

- 前端使用 **hash 路由**（`/#/users`），因此不需要 SPA fallback；
- `/api/*` 与前端同源，不需要 CORS，也没有跨站 Cookie 配置；
- 非 GET 请求必须带 `x-amz-content-sha256`，前端请求层已全局注入；
- HTML 始终 `no-cache`；带 hash 的 assets 使用不可变长缓存。

更多细节：[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)。

---

## 仓库结构

```text
.
├── api/                     # Rust crate: admin-kit-api
│   ├── Cargo.toml
│   └── src/
├── crates/
│   ├── core/                # 配置、JWT、AppError、axum/Lambda 启动
│   └── dynamo/              # DynamoDB 薄封装（无 ORM）
├── web/                     # Soybean Admin v2.3.0（独立 pnpm 应用）
│   ├── UPSTREAM.md
│   └── src/views/users/     # 参考后台页面
├── infra/
│   ├── template.yaml        # DynamoDB + S3 + Lambda + Function URL + CloudFront
│   ├── deploy.sh            # 端到端部署
│   ├── smoke.sh             # 线上冒烟测试
│   └── params.example.json
├── docs/
│   ├── AI-USAGE.md          # AI 如何使用本项目
│   ├── EXTENDING-ADMIN-PAGES.md
│   ├── ARCHITECTURE.md
│   ├── API.md
│   └── DEPLOY.md
├── AI-PROMPT.md             # 可复制给 AI 的英文提示词
├── AI-PROMPT.zh-CN.md       # 中文提示词
├── AGENTS.md                # Agent 规则（英文）
├── AGENTS.zh-CN.md          # Agent 规则（中文）
└── README.md
```

---

## 本地快速开始

### 前置条件

| 依赖 | 要求 |
|---|---|
| Rust | stable + `x86_64-unknown-linux-musl`（部署用） |
| Node.js | `>=20.19.0` |
| pnpm | 未安装时用 `corepack pnpm ...` |
| AWS CLI | 仅部署 / SSM 需要 |
| 工具 | `zip`、`openssl`、`python3` |

### 后端（离线内存存储）

```bash
AWS_EC2_METADATA_DISABLED=true AWS_REGION=us-west-2 \
ADMIN_KIT_PROJECT=admin-kit ADMIN_KIT_ENV=dev \
ADMIN_KIT_DEV_JWT_SEED="$(openssl rand -base64 32)" \
ADMIN_KIT_DEV_ADMIN_EMAIL=admin@example.com \
ADMIN_KIT_DEV_ADMIN_PASSWORD=devpassword123 \
ADMIN_KIT_COOKIE_SECURE=false ADMIN_KIT_STORE=memory PORT=8090 \
cargo run -p admin-kit-api
```

### 前端

```bash
cd web
corepack pnpm install --frozen-lockfile
corepack pnpm dev
```

打开 <http://localhost:9527>，用上面的开发管理员账号登录。`.env.test` 已把 `/api` 代理到 `http://127.0.0.1:8090`。

### 测试

```bash
cargo test --workspace
cd web && corepack pnpm typecheck && corepack pnpm build
```

---

## 部署到 AWS

```bash
source ~/proxy.sh                       # 如果网络需要代理
cp infra/params.example.json infra/params.json
# 修改 AdminEmail（默认 admin@example.com）、Project、Environment、区域、Layer ARN
./infra/deploy.sh
```

脚本会：

1. 创建 SSM 密钥（`jwt_seed`、`admin_email`、`admin_password`；密码不会打印）；
2. 构建 musl Lambda 二进制并上传 `bootstrap.zip`；
3. 部署 CloudFormation 栈；
4. 构建 Soybean 前端并同步到 S3；
5. 失效 CloudFront 并轮询 `/health`。

读取管理员密码：

```bash
aws ssm get-parameter --name /admin-kit/dev/admin_password \
  --with-decryption --query Parameter.Value --output text
```

首次用该邮箱/密码登录会创建 `admin` 账号。如果该邮箱已经是普通用户，**不会**自动提权。

完整部署说明：[`docs/DEPLOY.md`](docs/DEPLOY.md)。

---

## API

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/health` | 健康检查 |
| GET | `/api/config` | 功能开关 |
| POST | `/api/auth/login` | `{email,password}` → 用户 + Cookie |
| GET | `/api/auth/me` | 当前用户 |
| POST | `/api/auth/logout` | 清除会话 |
| POST | `/api/auth/password` | 修改自己的密码 |
| GET | `/api/admin/users` | 游标分页用户列表 |
| POST | `/api/admin/users` | 创建用户 |
| PATCH | `/api/admin/users/{id}` | 角色 / 禁用 / 密码 |
| GET | `/api/admin/stats` | 用户统计 |

错误使用 HTTP 状态码 + JSON。详见 [`docs/API.md`](docs/API.md)。

---

## AI 使用

- **提示词**：[`AI-PROMPT.zh-CN.md`](AI-PROMPT.zh-CN.md)
- **Agent 规则**：[`AGENTS.zh-CN.md`](AGENTS.zh-CN.md)
- **详细说明**：[`docs/AI-USAGE.md`](docs/AI-USAGE.md)
- **新增页面**：[`docs/EXTENDING-ADMIN-PAGES.md`](docs/EXTENDING-ADMIN-PAGES.md)
- **Copilot**：[`.github/copilot-instructions.md`](.github/copilot-instructions.md)
- **Claude Code**：[`CLAUDE.md`](CLAUDE.md)

推荐的 AI 工作流：

```text
读提示词 → 查看 users 参考实现 → 填 Access Pattern Sheet
→ Rust API/store → Soybean 页面/API client → 测试 → CFN GSI（如需要）→ smoke
```

---

## 安全说明

- 密码/token 不进入 localStorage；会话是 HttpOnly Cookie。
- 前端永远看不到 DynamoDB 或 AWS 凭证。
- 服务端角色校验才是权威；隐藏菜单不是授权。
- 没有遥测、没有第三方 CDN/字体、生产构建没有 mock 数据。
- 生产构建关闭 source map。
- 当前版本没有登录限流；公网暴露时请在前面加 WAF/限流。见 [`SECURITY.md`](SECURITY.md)。

---

## 贡献

见 [`CONTRIBUTING.zh-CN.md`](CONTRIBUTING.zh-CN.md)。简短版：

- 新增后台页面必须先填 Access Pattern Sheet；
- 不允许通用查询接口、`Scan`、任意排序/过滤；
- 测试必须离线；`cargo test --workspace` 与前端 typecheck/build 必须通过；
- 不要提交密钥、含凭证的 `.env`、`params.json` 或构建产物。

---

## License

MIT —— 见 [`LICENSE`](LICENSE)。Soybean Admin 及其他第三方声明见 [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)。