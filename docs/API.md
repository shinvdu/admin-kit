# API

Base path: `/api`（同源部署）。

错误格式（前端兼容 nested 结构）：

```json
{ "error": { "code": "INVALID_CREDENTIALS", "message": "invalid credentials" } }
```

HTTP 状态码与 code 一一对应；`401` 由前端统一清理会话并跳登录。

---

## Auth

### `POST /api/auth/login`

```json
{ "email": "admin@example.com", "password": "********" }
```

`200` + `Set-Cookie: admin_session=...; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=2592000`

```json
{ "user": { "id": "user_...", "email": "admin@example.com", "role": "admin", "disabled": false } }
```

失败：

| Status | code |
|---|---|
| 400 | `BAD_REQUEST` / `VALIDATION_ERROR` |
| 401 | `INVALID_CREDENTIALS`（不区分账号不存在/密码错/禁用） |
| 429 | `RATE_LIMITED`（若启用限流） |

### `GET /api/auth/me`

`200`：`{ "user": { id, email, role, disabled } }`；未登录 `401 UNAUTHORIZED`。

### `POST /api/auth/logout`

`204`，清 Cookie。

### `POST /api/auth/password`

```json
{ "current_password": "...", "new_password": "..." }
```

`204`；`400` 校验失败；`401` 当前密码错误。成功后 `session_version + 1` 并重新签发 Cookie。

---

## Admin users

所有接口仅接受 Cookie 会话，role ∈ {owner, admin}。

### `GET /api/admin/users?limit=20&cursor=&q=&role=&status=`

| 参数 | 说明 |
|---|---|
| `limit` | 1–100，默认 20 |
| `cursor` | 不透明游标，原样回传 |
| `q` | 邮箱包含匹配（服务端实现） |
| `role` | `owner|admin|user` |
| `status` | `active|disabled` |

`200`：

```json
{
  "items": [
    {
      "id": "user_...",
      "email": "alice@example.com",
      "role": "admin",
      "disabled": false,
      "created_at": 1795578933180,
      "updated_at": 1795578933180
    }
  ],
  "next_cursor": "eyJ..."
}
```

- `next_cursor` 为 `null` 表示没有下一页；
- 不返回总数/总页数；
- 时间字段为 epoch ms（前端转 ISO）。

### `POST /api/admin/users`

```json
{ "email": "bob@example.com", "password": "********", "role": "user" }
```

`201` 返回用户；`400` 校验失败；`409 EMAIL_TAKEN`。

### `PATCH /api/admin/users/{id}`

```json
{ "role": "admin", "disabled": true, "password": "********" }
```

字段均可选（至少一个）。`200` 返回更新后的用户。

| Status | code | 场景 |
|---|---|---|
| 400 | `SELF_ACTION` | 禁用/降级自己，或改自己的密码（应走 `/auth/password`） |
| 400 | `LAST_ADMIN` | 移除最后一个 active admin |
| 404 | `NOT_FOUND` | 账号不存在 |

任何实际变更都会递增 `session_version`，目标用户的旧会话立即失效。

### `GET /api/admin/stats`

```json
{ "total_users": 128, "active_users": 120, "disabled_users": 8 }
```

小规模直接由 GSI 统计；大规模应改为计数项/Streams。

---

## Public

### `GET /health`

```json
{ "status": "ok", "service": "admin-kit-api" }
```

### `GET /api/config`

```json
{ "features": { "emailAuth": true, "signup": false, "admin": true } }
```

---

## 前端请求约定

- 同源 `/api`；`withCredentials: true`；
- 非 GET/HEAD 请求带 `x-amz-content-sha256`（请求体 SHA-256 hex；空 body 固定哈希）；
- 401 → 单飞清理并跳 `/login?redirect=...`；
- 400/409 由页面处理；5xx 统一泛化提示。