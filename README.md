# admin-kit

**English** | [简体中文](README.zh-CN.md)

[![CI](https://github.com/shinvdu/admin-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/shinvdu/admin-kit/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> ## 🤖 AI agents: start here
>
> **Read [`AI-PROMPT.md`](AI-PROMPT.md) first, then [`AGENTS.md`](AGENTS.md) and [`docs/EXTENDING-ADMIN-PAGES.md`](docs/EXTENDING-ADMIN-PAGES.md).**
>
> Copy this prompt into your agent:
>
> ```text
> You are an AI coding agent working in admin-kit — a Soybean Admin + Rust Lambda
> + DynamoDB + CloudFront admin starter.
>
> Before writing code:
> 1. Read AI-PROMPT.md, AGENTS.md, docs/AI-USAGE.md, docs/EXTENDING-ADMIN-PAGES.md.
> 2. For any new page, fill the Access Pattern Sheet first (page / query / key /
>    index / sort / whitelisted filters / pagination / writes).
> 3. Use api/src/admin.rs and web/src/views/users/ as the reference.
>
> Hard rules:
> - One page = dedicated Rust API + explicit DynamoDB access pattern.
> - NEVER add a generic "query any table/column" endpoint, filter DSL, or Scan.
> - Lists use opaque cursor pagination; no page numbers or total counts.
> - Sort is fixed or enum-backed; every sort maps to a known key/GSI.
> - Auth/RBAC is enforced server-side; UI hiding is not security.
> - Mutating requests through CloudFront must carry x-amz-content-sha256.
> - No secrets, demo accounts, mock data, or third-party telemetry.
>
> Deliverables: Access Pattern Sheet, Rust handler/store, Soybean page + API client,
> tests, CFN GSI changes (if any), and a smoke/verification report.
> ```

**A deployable, AI-friendly admin starter** — Soybean Admin on the frontend, Rust (axum) Lambda on the backend, DynamoDB single-table storage, S3 + CloudFront delivery, HttpOnly Cookie sessions.

It is intentionally narrow: **user management + authentication** is the complete reference implementation. New admin pages are added with dedicated APIs and explicit DynamoDB access patterns — not a generic “database client”.

---

## Why this exists

Most admin templates assume a relational database and a generic query layer (`filter[anything]`, `sort=any`, `page=N`, `COUNT(*)`). DynamoDB does not work that way.

admin-kit demonstrates a different, production-shaped pattern:

- **Dedicated endpoint per screen** (`/api/admin/users`), not `/api/admin/query?table=...`;
- **Explicit key/index design** (base table Query, or one GSI per access pattern);
- **Opaque cursor pagination**; no fake page numbers or total counts;
- **Server-side RBAC**; the frontend menu is only presentation;
- **Immediate session revocation** via `session_version` on disable/password/role changes;
- **AI-first documentation**: a prompt, an agent rulebook, and a page-extension playbook.

If you need arbitrary ad-hoc queries, BI, or cross-entity JOINs, add a relational read model (Aurora/Postgres) or use DynamoDB Export → S3 → Athena. Do not contort DynamoDB into a SQL database.

---

## Features

- **Frontend**: Soybean Admin `v2.3.0` (Vue 3 + Vite + TypeScript + Naive UI + UnoCSS)
- **Backend**: Rust + axum + AWS Lambda Web Adapter (one Lambda)
- **Data**: DynamoDB single table (`ACCOUNT#` / `EMAIL#`, GSI for admin listing)
- **Delivery**: CloudFront → S3 static frontend + Lambda Function URL (OAC)
- **Auth**: email + password, argon2id, HttpOnly Cookie, 30-day session
- **Admin**: dashboard, cursor-paginated user list, create user, change role, disable/enable, reset password
- **Immediate revocation**: disable/password/role change invalidates existing sessions on the next request
- **Bootstrap admin**: SSM `admin_email` + `admin_password` creates the first admin on login
- **CloudFront-safe writes**: frontend automatically adds `x-amz-content-sha256`
- **Light/dark**, responsive layout, `zh-CN` / `en-US`
- **Infrastructure as code**: DynamoDB, S3, Lambda, Function URL, CloudFront, SSM, IAM
- **End-to-end smoke script**: login → me → list → create → disable → revocation → logout

---

## Architecture

```text
Browser
  │  HTTPS (CloudFront default domain)
  ▼
CloudFront
  ├─ /api/*    → Lambda Function URL (OAC, SigV4)
  ├─ /health   → Lambda Function URL
  ├─ /assets/* → S3 (long cache)
  └─ default    → S3 index.html (no-cache)

Rust Lambda (admin-kit-api)
  │
  ▼
DynamoDB single table
  ACCOUNT#user:<uuid> / META
  EMAIL#<normalized_email> / META
  gsi1pk = "USERS", gsi1sk = "{created_ms:013}#user:<uuid>"
```

- The frontend uses **hash routing** (`/#/users`), so no SPA fallback is required.
- `/api/*` is same-origin; no CORS or cross-site Cookie configuration.
- Non-GET requests must include `x-amz-content-sha256`; the frontend injects it globally.
- HTML is always `no-cache`; hashed assets are immutable.

More detail: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

---

## Repository layout

```text
.
├── api/                     # Rust crate: admin-kit-api
│   ├── Cargo.toml
│   └── src/
├── crates/
│   ├── core/                # config, JWT, AppError, axum/Lambda bootstrap
│   └── dynamo/              # thin DynamoDB wrapper (no ORM)
├── web/                     # Soybean Admin v2.3.0 (independent pnpm app)
│   ├── UPSTREAM.md
│   └── src/views/users/     # reference admin page
├── infra/
│   ├── template.yaml        # DynamoDB + S3 + Lambda + Function URL + CloudFront
│   ├── deploy.sh            # end-to-end deploy
│   ├── smoke.sh             # online smoke test
│   └── params.example.json
├── docs/
│   ├── AI-USAGE.md          # how AI agents should use this repo
│   ├── EXTENDING-ADMIN-PAGES.md
│   ├── ARCHITECTURE.md
│   ├── API.md
│   └── DEPLOY.md
├── AI-PROMPT.md             # copy-paste prompt for AI agents
├── AGENTS.md                # agent rulebook
└── README.md
```

---

## Quick start (local)

### Prerequisites

| Dependency | Requirement |
|---|---|
| Rust | stable + `x86_64-unknown-linux-musl` (for deploy) |
| Node.js | `>=20.19.0` |
| pnpm | `corepack pnpm ...` if not installed |
| AWS CLI | only for deploy / SSM |
| Tools | `zip`, `openssl`, `python3` |

### Backend (offline memory store)

```bash
AWS_EC2_METADATA_DISABLED=true AWS_REGION=us-west-2 \
ADMIN_KIT_PROJECT=admin-kit ADMIN_KIT_ENV=dev \
ADMIN_KIT_DEV_JWT_SEED="$(openssl rand -base64 32)" \
ADMIN_KIT_DEV_ADMIN_EMAIL=admin@example.com \
ADMIN_KIT_DEV_ADMIN_PASSWORD=devpassword123 \
ADMIN_KIT_COOKIE_SECURE=false ADMIN_KIT_STORE=memory PORT=8090 \
cargo run -p admin-kit-api
```

### Frontend

```bash
cd web
corepack pnpm install --frozen-lockfile
corepack pnpm dev
```

Open <http://localhost:9527>, log in with the dev admin credentials above. `.env.test` proxies `/api` to `http://127.0.0.1:8090`.

### Tests

```bash
cargo test --workspace
cd web && corepack pnpm typecheck && corepack pnpm build
```

---

## Deploy to AWS

```bash
source ~/proxy.sh                       # if your network needs a proxy
cp infra/params.example.json infra/params.json
# edit AdminEmail (default admin@example.com), Project, Environment, region, layer ARN
./infra/deploy.sh
```

The script:

1. creates SSM secrets (`jwt_seed`, `admin_email`, `admin_password`; password is never printed);
2. builds the musl Lambda binary and uploads `bootstrap.zip`;
3. deploys the CloudFormation stack;
4. builds the Soybean frontend and syncs it to S3;
5. invalidates CloudFront and polls `/health`.

Read the admin password:

```bash
aws ssm get-parameter --name /admin-kit/dev/admin_password \
  --with-decryption --query Parameter.Value --output text
```

First login with that email/password creates the `admin` account. If the email already exists as a normal user, it is **not** auto-promoted.

Full deployment notes: [`docs/DEPLOY.md`](docs/DEPLOY.md).

---

## API

| Method | Path | Description |
|---|---|---|
| GET | `/health` | health check |
| GET | `/api/config` | feature flags |
| POST | `/api/auth/login` | `{email,password}` → user + Cookie |
| GET | `/api/auth/me` | current user |
| POST | `/api/auth/logout` | clear session |
| POST | `/api/auth/password` | change own password |
| GET | `/api/admin/users` | cursor-paginated users |
| POST | `/api/admin/users` | create user |
| PATCH | `/api/admin/users/{id}` | role / disabled / password |
| GET | `/api/admin/stats` | user counts |

Errors use HTTP status + JSON. See [`docs/API.md`](docs/API.md).

---

## AI usage

- **Prompt**: [`AI-PROMPT.md`](AI-PROMPT.md) / [`AI-PROMPT.zh-CN.md`](AI-PROMPT.zh-CN.md)
- **Agent rules**: [`AGENTS.md`](AGENTS.md) / [`AGENTS.zh-CN.md`](AGENTS.zh-CN.md)
- **Detailed guide**: [`docs/AI-USAGE.md`](docs/AI-USAGE.md)
- **Adding pages**: [`docs/EXTENDING-ADMIN-PAGES.md`](docs/EXTENDING-ADMIN-PAGES.md)
- **Copilot**: [`.github/copilot-instructions.md`](.github/copilot-instructions.md)
- **Claude Code**: [`CLAUDE.md`](CLAUDE.md)

The intended AI workflow is:

```text
read prompt → inspect users reference → fill Access Pattern Sheet
→ Rust API/store → Soybean page/API client → tests → CFN GSI (if needed) → smoke
```

---

## Security notes

- No passwords or tokens in localStorage; the session is an HttpOnly Cookie.
- The frontend never sees DynamoDB or AWS credentials.
- Server-side role checks are authoritative; hiding a menu is not authorization.
- No telemetry, no third-party CDN/fonts, no mock data in production builds.
- Production builds disable source maps.
- The current version does not implement login rate limiting; put WAF/rate limits in front if exposed publicly. See [`SECURITY.md`](SECURITY.md).

---

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). The short version:

- New admin pages require an Access Pattern Sheet before code.
- No generic query endpoints, no `Scan`, no arbitrary sort/filter.
- Tests must be offline; `cargo test --workspace` and frontend typecheck/build must pass.
- Do not commit secrets, `.env` files with credentials, `params.json`, or build artifacts.

---

## License

MIT — see [`LICENSE`](LICENSE). Soybean Admin and other third-party notices are listed in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
