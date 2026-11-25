# 贡献指南

[English](CONTRIBUTING.md) | **简体中文**

感谢你帮助改进 admin-kit。

## 设计原则

1. **一个页面 = 一组专用 Rust API + 明确的 DynamoDB 访问模式。**
2. 禁止通用“任意表/任意列”查询接口、filter DSL、任意排序。
3. 禁止 `Scan`、请求内 `Count`、页码/offset 分页、全表导出。
4. 列表使用不透明游标；响应为 `{ items, next_cursor }`。
5. 服务端 RBAC 才是权威；前端隐藏不是授权。
6. 生产构建不允许密钥、demo 账号、mock 数据或遥测。
7. 新增页面必须在写代码前填写
   [`docs/EXTENDING-ADMIN-PAGES.md`](docs/EXTENDING-ADMIN-PAGES.md) §2 的
   Access Pattern Sheet。

如果需求需要任意查询/BI/JOIN/全文搜索，请提出关系型读模型
（Aurora/Postgres）、DynamoDB Export → S3 → Athena 或 OpenSearch 方案，
不要把 DynamoDB 硬掰成 SQL 数据库。

## 环境准备

```bash
git clone https://github.com/<owner>/admin-kit.git
cd admin-kit

# 后端
cargo test --workspace

# 前端
cd web
corepack pnpm install --frozen-lockfile
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm build
```

本地后端（内存存储，不依赖 AWS）：

```bash
AWS_EC2_METADATA_DISABLED=true AWS_REGION=us-west-2 \
ADMIN_KIT_PROJECT=admin-kit ADMIN_KIT_ENV=dev \
ADMIN_KIT_DEV_JWT_SEED="$(openssl rand -base64 32)" \
ADMIN_KIT_DEV_ADMIN_EMAIL=admin@example.com \
ADMIN_KIT_DEV_ADMIN_PASSWORD=devpassword123 \
ADMIN_KIT_COOKIE_SECURE=false ADMIN_KIT_STORE=memory PORT=8090 \
cargo run -p admin-kit-api
```

## 新增后台页面

按 [`docs/EXTENDING-ADMIN-PAGES.md`](docs/EXTENDING-ADMIN-PAGES.md) 执行：

1. 填 Access Pattern Sheet；
2. 设计 Key + 游标；
3. 增加 Rust store port、内存适配器、Dynamo 适配器、`AdminAuth` handler；
4. 增加前端 API client、类型、Soybean 页面、路由、i18n；
5. 如需要，增加 CFN GSI/属性；等待回填；
6. 增加测试 + 扩展 `infra/smoke.sh`；
7. 如果契约变化，更新 docs/API/README。

参考实现：`api/src/admin.rs`、`api/src/dynamo_account_store.rs`、
`web/src/views/users/`、`web/src/service/api/users.ts`。

## 测试

必须离线运行（不访问 AWS/SSM/网络）：

```bash
cargo test --workspace
cargo build -p admin-kit-api
cd web && corepack pnpm typecheck && corepack pnpm exec eslint . && corepack pnpm build
```

如果修改访问模式，至少增加：

- limit 钳制 / 非法游标；
- 过滤白名单与空结果；
- 角色校验（401/403）；
- 创建冲突（409）与校验失败（400）；
- 相关会话撤销。

## Pull Request

请包含：

- Access Pattern Sheet；
- API 契约/错误码；
- GSI/基础设施变更 + 回填/成本说明；
- 测试/构建输出；
- 部署后的 smoke 证据；
- 已知限制。

不要提交：

- 凭证、含密钥的 `.env`、`infra/params.json`；
- `web/node_modules`、`web/dist`、`target/`；
- demo/mock 数据或第三方遥测；
- 含密钥或 source map 的生成文件。

## 代码风格

- Rust：`cargo fmt --all`；不引入 ORM/查询 DSL；handler 薄，store 适配器负责数据。
- Vue/TS：严格 TypeScript；Naive UI + UnoCSS；所有 UI 文案走 i18n。
- 日志：只记录 id/动作/状态；绝不记录邮箱、密码、Cookie、token。
- 错误：HTTP 状态 + 稳定 `code` + 用户安全文案。

## 提交信息

建议使用 Conventional Commits：

```text
feat(users): add role filter
fix(auth): revoke sessions on role change
docs(ai): add page extension prompt
```

提交即表示你同意以 MIT License 授权你的改动。