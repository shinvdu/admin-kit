# 上游说明

[English](UPSTREAM.md) | **简体中文**

- 仓库：<https://github.com/soybeanjs/soybean-admin>
- Tag：`v2.3.0`
- Commit：`ab40c08992807e303dd8aabb6ec43d7e2334b24f`
- 拉取日期：2026-11-25
- License：MIT（见 `LICENSE`）
- 包管理器：pnpm（保留 `pnpm-lock.yaml`）

## 本地改动（admin-kit 适配）

上游 `.git` 已删除。以下内容只属于 `web`，**不是**上游内容。

### 环境变量

- `.env`：base `/`、hash 路由、静态认证路由、超级角色 `owner`、
  存储前缀 `ADMIN_`、关闭 source map；token/refresh 相关变量为空。
- `.env.test`：本地 Rust API `http://127.0.0.1:8090`，开启 Vite proxy。
- `.env.prod`：同源 `/api`，关闭 proxy。
- 无 Apifox token、mock host、demo 账号或 demo API。

### 请求层

- `src/service/request/index.ts`：axios `withCredentials: true`；按 HTTP 状态
  判断成功（2xx）；无 Authorization/token refresh 逻辑。
- 错误处理兼容 `{ error: { code, message } }` 与 `{ code, message }`。
- 所有 `POST` / `PUT` / `PATCH` / `DELETE` 自动附加 `x-amz-content-sha256`
  （精确 JSON body 的 SHA-256；无 body 用固定空串哈希）。
  CloudFront → Lambda Function URL OAC 需要它。
- 401 清理 Cookie 会话并只跳转一次登录（单飞）；4xx 由页面处理；
  5xx/网络错误使用统一文案。

### 认证与路由

- `src/store/modules/auth/index.ts`：Cookie 会话状态机，包含 `user`、
  `initialized`、`loginLoading`、`ensureAuth()`、`login()`、`logout()`、
  `fetchMe()`、`handleUnauthorized()`。localStorage 不存 token。
- `src/router/guard/route.ts`：受保护路由进入前只调用一次 `ensureAuth()`；
  `/users` 设置 `meta.roles = ['owner', 'admin']`。
- 静态 elegant 路由由 `src/views` 生成；登录路由改为纯 `/login`。

### 页面

- `src/views/home/index.vue`：Dashboard（有 `/admin/stats` 时显示统计卡片、
  最近用户、快捷入口）。
- `src/views/users/index.vue` + `modules/user-form.vue` +
  `modules/reset-password.vue`：用户列表、服务端搜索/筛选、游标分页、
  创建、改角色、禁用/启用、重置密码。
- `src/layouts/modules/global-header/components/change-password.vue`：
  可选的用户自助改密。

### 删除的上游 demo

- 登录模块：验证码登录、注册、找回密码、微信登录，以及
  Super/Admin/User 快捷登录账号。
- `src/views/_builtin/iframe-page/`。
- `src/views/home/modules/` 下的 demo 区块。
- demo 路由 API（`src/service/api/route.ts`）、动态路由 store 分支、
  refresh-token 辅助。
- Apifox mock URL/token，以及 `src/` 内 Soybean 品牌可见文案。

### i18n

- 上游提供 `zh-CN` 与 `en-US`。新增后台 key 都定义在
  `App.I18n.Schema`（`src/typings/app.d.ts`），并实现于
  `src/locales/langs/zh-cn.ts` / `en-us.ts`。
- 新增 key 分组：`page.login.*`、`page.home.*`、`page.users.*`、
  `common.role.*`、`form.password.*`、`form.newPassword.*`、`form.role.*`、
  `request.serviceUnavailable`。
- 如果宿主项目要求完整 10 语言，按同一 key 结构补齐（typecheck 会强制对齐）。