# Admin Kit（`web`）

[English](README.md) | **简体中文**

基于 [Soybean Admin](https://github.com/soybeanjs/soybean-admin) `v2.3.0`（MIT）
的后台控制台。它是**独立**的 Vite/Vue 应用，不与产品前端共享构建产物或依赖。

固定 commit 与完整本地改动见 `UPSTREAM.md`。

## 技术栈

- Vue 3、Vite、TypeScript、Naive UI、UnoCSS、Pinia、Vue Router（hash 模式）
- Cookie 会话（`admin_session`，HttpOnly），localStorage 不存 token
- API：生产环境同源 `/api`；开发环境使用 Vite proxy

## 要求

- Node `>= 20.19`
- pnpm 通过 corepack（`corepack pnpm ...`）；不要依赖全局安装

## 本地开发

1. 在 `8090` 启动 Rust API（或改 `.env.test` 的 `VITE_SERVICE_BASE_URL`）。
2. `corepack pnpm install --frozen-lockfile`
3. `corepack pnpm dev`
4. 打开 <http://127.0.0.1:9527>。`/proxy-default/*` 会重写到
   `http://127.0.0.1:8090/*`；浏览器视角仍是同源 Cookie。

本地 HTTP 下 Cookie 不能带 `Secure`；生产使用 `Secure` + `SameSite=Lax`。

## 环境变量

| 文件 | 用途 |
|---|---|
| `.env` | base `/`、hash 路由、静态路由、超级角色 `owner`、`ADMIN_` 存储前缀、关闭 source map |
| `.env.test` | `VITE_SERVICE_BASE_URL=http://127.0.0.1:8090`，开启 proxy |
| `.env.prod` | `VITE_SERVICE_BASE_URL=/api`，关闭 proxy |

这些文件不含密钥，需要入库。不要把凭证写进去。

## 构建

```bash
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm build          # 产物：dist/
```

`dist/` 包含 `index.html` 与带 hash 的 assets。`index.html` 必须
`no-cache`；带 hash 的 `assets/*` 可长缓存。使用 hash 路由（`/#/users`），
服务端不需要 history fallback。

如果部署在 `/admin/` 这类子路径，设置 `VITE_BASE_URL=/admin/` 后重建。

## API 契约（前端侧）

所有 URL 定义在 `src/service/api/*.ts`：

- `POST /auth/login` `{ email, password }` → 200 `{ user }`，并设置 Cookie
- `GET /auth/me` → `{ user }`；未登录 401
- `POST /auth/logout` → 204
- `POST /auth/password` `{ current_password, new_password }` → 204
- `GET /admin/users?limit=&cursor=&q=&role=&status=` →
  `{ items, next_cursor }`（epoch ms 在 `users.ts` 转 ISO）
- `POST /admin/users` `{ email, password, role }` → 201；409 `EMAIL_TAKEN`
- `PATCH /admin/users/{id}` `{ role?, disabled?, password? }` → 200；
  400 `SELF_ACTION` / `LAST_ADMIN`
- `GET /admin/stats` → `{ total_users, active_users, disabled_users }`
  （404 时隐藏 Dashboard 卡片）

所有写请求都携带按精确 JSON body 计算的 `x-amz-content-sha256`
（无 body 用固定空串哈希），否则 CloudFront → Lambda Function URL OAC 会拒绝。

## 页面

| 路由 | 页面 | 权限 |
|---|---|---|
| `/#/login` | 邮箱 + 密码登录 | 公开 |
| `/#/home` | Dashboard：统计（可选）、最近用户 | 已登录 |
| `/#/users` | 用户列表、筛选、游标分页、创建/改角色/禁用/重置密码 | `owner`、`admin` |
| `/#/403`、`/#/404`、`/#/500` | 异常页 | 公开 |

## 安全说明

- 浏览器存储中没有 access/refresh token、密码、API Key 或 AWS 凭证；
  会话只存在 HttpOnly Cookie。
- 菜单/按钮里的角色检查只是展示；Rust Admin API 才是安全边界。
- 没有 `v-html`；所有用户数据用模板插值。
- 没有第三方分析/CDN/字体脚本。
- 401 跳转做了单飞，避免循环；网络错误不暴露后端内部信息（统一文案）。

## 测试

```bash
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm build
```

当前没有针对后台页面的单元测试（上游该版本未带 Vitest 配置）。
typecheck、lint、生产构建是必须门禁；其余 TODO 见任务报告。