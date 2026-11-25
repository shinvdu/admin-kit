# Contributing

Thanks for helping improve admin-kit.

## Design principles

1. **One page = dedicated Rust API + explicit DynamoDB access pattern.**
2. No generic “query any table/column” endpoints, filter DSLs, or arbitrary sort.
3. No `Scan`, request-time `Count`, offset/page-number pagination, or full-table export.
4. Lists use opaque cursors; responses are `{ items, next_cursor }`.
5. Server-side RBAC is authoritative; frontend hiding is not authorization.
6. No secrets, demo accounts, mock data, or telemetry in production builds.
7. Every new page must fill the Access Pattern Sheet in
   [`docs/EXTENDING-ADMIN-PAGES.md`](docs/EXTENDING-ADMIN-PAGES.md) §2 **before** code.

If a requirement needs arbitrary query/BI/JOIN/full-text search, propose a
relational read model (Aurora/Postgres), DynamoDB Export → S3 → Athena, or
OpenSearch instead of extending DynamoDB into a SQL database.

## Setup

```bash
git clone https://github.com/<owner>/admin-kit.git
cd admin-kit

# backend
cargo test --workspace

# frontend
cd web
corepack pnpm install --frozen-lockfile
corepack pnpm typecheck
corepack pnpm lint
corepack pnpm build
```

Local backend (memory store, no AWS):

```bash
AWS_EC2_METADATA_DISABLED=true AWS_REGION=us-west-2 \
ADMIN_KIT_PROJECT=admin-kit ADMIN_KIT_ENV=dev \
ADMIN_KIT_DEV_JWT_SEED="$(openssl rand -base64 32)" \
ADMIN_KIT_DEV_ADMIN_EMAIL=admin@example.com \
ADMIN_KIT_DEV_ADMIN_PASSWORD=devpassword123 \
ADMIN_KIT_COOKIE_SECURE=false ADMIN_KIT_STORE=memory PORT=8090 \
cargo run -p admin-kit-api
```

## Adding an admin page

Follow [`docs/EXTENDING-ADMIN-PAGES.md`](docs/EXTENDING-ADMIN-PAGES.md):

1. Fill the Access Pattern Sheet.
2. Design keys + cursor.
3. Add Rust store port, memory adapter, Dynamo adapter, `AdminAuth` handler.
4. Add frontend API client, typings, Soybean page, route, i18n.
5. Add CFN GSI/attributes if needed; wait for backfill.
6. Add tests + extend `infra/smoke.sh`.
7. Update docs/API/README if the contract changes.

Reference implementation: `api/src/admin.rs`, `api/src/dynamo_account_store.rs`,
`web/src/views/users/`, `web/src/service/api/users.ts`.

## Tests

Must run offline (no AWS/SSM/network):

```bash
cargo test --workspace
cargo build -p admin-kit-api
cd web && corepack pnpm typecheck && corepack pnpm exec eslint . && corepack pnpm build
```

If you change an access pattern, add tests for:

- limit clamp / invalid cursor;
- filter whitelist and empty results;
- role checks (401/403);
- create conflict (409) and validation (400);
- session revocation where relevant.

## Pull requests

Include:

- Access Pattern Sheet;
- API contract/error codes;
- GSI/infra changes + backfill/cost notes;
- test/build output;
- smoke evidence (if deployed);
- known limitations.

Do not commit:

- credentials, `.env` with secrets, `infra/params.json`;
- `web/node_modules`, `web/dist`, `target/`;
- demo/mock data or third-party telemetry;
- generated files containing secrets or source maps.

## Code style

- Rust: `cargo fmt --all`; no ORM/query DSL; thin handlers, store adapters.
- Vue/TS: TypeScript strict; Naive UI + UnoCSS; all UI text via i18n.
- Logs: ids/actions/status only; never emails, passwords, Cookies, tokens.
- Errors: HTTP status + stable `code` + user-safe message.

## Commits

Conventional commits are appreciated:

```text
feat(users): add role filter
fix(auth): revoke sessions on role change
docs(ai): add page extension prompt
```

By contributing you agree your changes are licensed under the MIT License.