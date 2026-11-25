# Admin Kit (`web`)

**English** | [简体中文](README.zh-CN.md)

Operations console built on [Soybean Admin](https://github.com/soybeanjs/soybean-admin)
`v2.3.0` (MIT). It is a **standalone** Vite/Vue app; it does not share build
output or dependencies with the product frontend.

See `UPSTREAM.md` for the pinned commit and the complete list of local changes.

## Stack

- Vue 3, Vite, TypeScript, Naive UI, UnoCSS, Pinia, Vue Router (hash mode)
- Cookie session (`admin_session`, HttpOnly), no token in localStorage
- API: same-origin `/api` in production, Vite proxy in development

## Requirements

- Node `>= 20.19`
- pnpm via corepack (`corepack pnpm ...`); do not rely on a global install

## Local development

1. Start the Rust API on port `8090` (or change `.env.test` (`VITE_SERVICE_BASE_URL`)).
2. `corepack pnpm install --frozen-lockfile`
3. `corepack pnpm dev`
4. Open <http://127.0.0.1:9527>. Requests to `/proxy-default/*` are rewritten to
   `http://127.0.0.1:8090/*`; cookies are same-origin from the browser's point
   of view.

Local cookies must be issued without `Secure` (plain HTTP). Production uses
`Secure` + `SameSite=Lax`.

## Environment

| File | Purpose |
|---|---|
| `.env` | base path `/`, hash router, static routes, `owner` super role, `ADMIN_` storage prefix, source maps off |
| `.env.test` | `VITE_SERVICE_BASE_URL=http://127.0.0.1:8090`, proxy on |
| `.env.prod` | `VITE_SERVICE_BASE_URL=/api`, proxy off |

These files contain no secrets and are committed. Do not put credentials in
them.

## Build

```bash
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm build          # output: dist/
```

`dist/` contains `index.html` and hashed assets. Serve `index.html` with
`no-cache`; hashed `assets/*` may be cached immutably. Because routing is hash
based (`/#/users`), the server needs no history fallback.

If you deploy under a sub path such as `/admin/`, set `VITE_BASE_URL=/admin/`
and rebuild.

## API contract (frontend side)

All URLs are defined in `src/service/api/*.ts`:

- `POST /auth/login` `{ email, password }` → 200 `{ user }`, sets cookie
- `GET /auth/me` → `{ user }`, 401 when signed out
- `POST /auth/logout` → 204
- `POST /auth/password` `{ current_password, new_password }` → 204
- `GET /admin/users?limit=&cursor=&q=&role=&status=` →
  `{ items, next_cursor }` (epoch-ms timestamps converted to ISO in `users.ts`)
- `POST /admin/users` `{ email, password, role }` → 201; 409 `EMAIL_TAKEN`
- `PATCH /admin/users/{id}` `{ role?, disabled?, password? }` → 200;
  400 `SELF_ACTION` / `LAST_ADMIN`
- `GET /admin/stats` → `{ total_users, active_users, disabled_users }`
  (404 hides the Dashboard cards)

Every mutating request carries `x-amz-content-sha256` computed from the exact
JSON body (empty-body constant for no body), so CloudFront → Lambda Function
URL OAC accepts it.

## Pages

| Route | Page | Access |
|---|---|---|
| `/#/login` | Email + password login | public |
| `/#/home` | Dashboard: stats (optional), recent users | signed in |
| `/#/users` | User list, filters, cursor paging, create/role/disable/reset | `owner`, `admin` |
| `/#/403`, `/#/404`, `/#/500` | Exception pages | public |

## Security notes

- No access/refresh token, password, API key, or AWS credential in the browser
  storage. The session is the HttpOnly cookie only.
- Role checks in menus/buttons are display-only; the Rust Admin API remains the
  security boundary.
- No `v-html`; all user data is rendered with template interpolation.
- No third-party analytics/CDN/font scripts.
- 401 redirects are single-flight to avoid loops; network errors do not show
  backend internals (generic message).

## Tests

```bash
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm build
```

No admin-specific unit tests are included yet (the upstream template does not
ship a Vitest setup in this release). Type checking, linting and the production
build are the required gates; see the task report for remaining TODOs.
