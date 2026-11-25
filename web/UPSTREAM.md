# Upstream

- Repository: <https://github.com/soybeanjs/soybean-admin>
- Tag: `v2.3.0`
- Commit: `ab40c08992807e303dd8aabb6ec43d7e2334b24f`
- Fetched: 2026-11-25
- License: MIT (see `LICENSE`)
- Package manager: pnpm (lockfile kept as `pnpm-lock.yaml`)

## Local changes (admin-kit adaptation)

The upstream `.git` directory was removed. Everything below is local to
`web` and is **not** upstream.

### Environment

- `.env`: base `/`, hash router, static auth routes, super role `owner`,
  storage prefix `ADMIN_`, source maps off. Token/refresh-code variables are
  empty.
- `.env.test`: local Rust API at `http://127.0.0.1:8090`, Vite proxy enabled.
- `.env.prod`: same-origin `/api`, proxy disabled.
- No Apifox tokens, mock hosts, demo accounts, or demo API endpoints.

### Request layer

- `src/service/request/index.ts`: axios `withCredentials: true`; HTTP status
  based success (`2xx`); no Authorization/token refresh logic.
- Error handling supports both `{ error: { code, message } }` and
  `{ code, message }` bodies.
- Every `POST` / `PUT` / `PATCH` / `DELETE` request automatically receives
  `x-amz-content-sha256` (SHA-256 of the exact JSON body; fixed empty-body hash
  for no body). Required by CloudFront → Lambda Function URL OAC.
- 401 clears the cookie-session state and redirects to login once
  (single-flight). 4xx errors are handled by pages; 5xx/network errors use a
  generic message.

### Auth and routing

- `src/store/modules/auth/index.ts`: cookie-session state machine with
  `user`, `initialized`, `loginLoading`, `ensureAuth()`, `login()`,
  `logout()`, `fetchMe()`, `handleUnauthorized()`. No local storage token.
- `src/router/guard/route.ts`: calls `ensureAuth()` once before protected
  navigation; `/users` has `meta.roles = ['owner', 'admin']`.
- Static elegant routes are generated from `src/views`; the login route is
  now plain `/login` (no login modules).

### Pages

- `src/views/home/index.vue`: Dashboard (stats cards when
  `GET /admin/stats` exists, recent users, quick actions).
- `src/views/users/index.vue` + `modules/user-form.vue` +
  `modules/reset-password.vue`: user list with server-side search/filter,
  cursor pagination, create, role change, disable/enable, password reset.
- `src/layouts/modules/global-header/components/change-password.vue`: optional
  self-service password change.

### Removed upstream demo content

- Demo login modules: code login, register, reset password, WeChat, and the
  Super/Admin/User quick-login accounts.
- `src/views/_builtin/iframe-page/`.
- Demo blocks under `src/views/home/modules/`.
- Demo route API (`src/service/api/route.ts`), dynamic-route store branch, and
  refresh-token helpers.
- Apifox mock URLs/tokens and Soybean-branded visible strings inside `src/`.

### i18n

- Upstream ships `zh-CN` and `en-US`. New admin keys are all defined in
  `App.I18n.Schema` (`src/typings/app.d.ts`) and implemented in
  `src/locales/langs/zh-cn.ts` / `en-us.ts`.
- New key groups: `page.login.*`, `page.home.*`, `page.users.*`,
  `common.role.*`, `form.password.*`, `form.newPassword.*`, `form.role.*`,
  `request.serviceUnavailable`.
- If the host project requires the full 10-locale set, replicate the same
  key structure (the type checker enforces alignment).
