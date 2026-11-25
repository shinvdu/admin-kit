# 部署

## 前置条件

| 依赖 | 要求 |
|---|---|
| AWS | 可创建 IAM / Lambda / DynamoDB / S3 / CloudFront / SSM |
| 区域 | 默认 `us-west-2`；Lambda Web Adapter Layer ARN 与区域绑定 |
| Rust | stable + `x86_64-unknown-linux-musl` |
| Node | `>=20.19.0`；pnpm 可用 `corepack pnpm` |
| 工具 | `aws` CLI、`python3`、`zip`、`openssl` |

不需要 ACM 证书、DNS 或自定义域名。

## 凭证

项目根 `.env`（不入库）：

```dotenv
AWS_ACCESS_KEY_ID=...
AWS_SECRET_ACCESS_KEY=...
AWS_DEFAULT_REGION=us-west-2
```

脚本不会打印凭证或 SSM 密钥值。

## 参数

```bash
cp infra/params.example.json infra/params.json
```

可改：

```json
{
  "Project": "admin-kit",
  "Environment": "dev",
  "LambdaArchitecture": "x86_64",
  "LambdaMemoryMB": "256",
  "WebAdapterLayerArn": "arn:aws:lambda:us-west-2:753240598075:layer:LambdaAdapterLayerX86:28",
  "AdminEmail": "admin@example.com"
}
```

- `Project` / `Environment` 决定资源名与栈名（`{Project}-{Environment}`）。
- `AdminEmail` 是首次管理员引导邮箱；建议部署前改成你自己的邮箱。
- 换区域时必须换 `WebAdapterLayerArn`（Layer 与区域/架构绑定）。

环境变量覆盖：

```bash
PROJECT=my-admin ENV=prod ADMIN_KIT_ADMIN_EMAIL=me@example.com ./infra/deploy.sh
```

## 部署

```bash
./infra/deploy.sh
```

流程：

1. 幂等创建 SSM：`jwt_seed`、`admin_email`、`admin_password`；
2. musl 编译 `admin-kit-api`，打包 `bootstrap.zip`；
3. 创建部署桶并上传；
4. `aws cloudformation deploy`（DynamoDB / S3 / Lambda / Function URL / CloudFront）；
5. 构建 `web/` 并同步到前端桶（HTML no-cache、assets immutable）；
6. CloudFront invalidation；
7. 轮询 `/health` 并输出站点 URL。

首次 CloudFront 创建通常需要 5–15 分钟。

## 登录

```bash
aws ssm get-parameter --name /admin-kit/dev/admin_password \
  --with-decryption --query Parameter.Value --output text
```

用 `AdminEmail` + 该密码登录。首次登录会创建 `admin` 账号（argon2id 哈希落库）。
如果该邮箱已存在且 role=user，不会自动提权。

## 验证

```bash
./infra/smoke.sh
```

覆盖：登录、`/me`、用户列表、stats、创建、新用户登录、禁用、旧会话 401、禁用后登录失败、登出；结束时清理测试用户。

手工检查：

- `/` 返回登录页；
- `index.html` 的 `Cache-Control` 为 `no-cache`；
- `/assets/*` 为 `public,max-age=31536000,immutable`；
- 刷新页面仍保持登录（Cookie）。

## 更新

```bash
# 后端 / 前端一起更新
./infra/deploy.sh
```

- 只改前端也可以手动 `cd web && corepack pnpm build` 后 sync S3 + invalidation；
- 新增 GSI 需要等 CloudFormation 更新完成、索引 `ACTIVE` 且回填完成后再上线对应页面；
- 修改 SSM 密钥后需要重新部署或等待 Lambda 冷启动。

## 自定义域名（可选）

默认不配置。需要时：

1. 在 **us-east-1** 申请/导入 ACM 证书；
2. 在 `infra/template.yaml` 增加 `DomainName` / `AcmCertificateArn` 参数、`Aliases`、`ViewerCertificate`；
3. DNS CNAME 指向 CloudFront 域名；
4. 若前端与 API 不同源，需要处理 CORS + Cookie `Domain`/`SameSite`；同源 `/api` 不需要。

## 清理

> 删除栈会删除 DynamoDB 表（数据不可恢复）。确认栈名后再执行。

```bash
STACK=admin-kit-dev
BUCKET=admin-kit-dev-frontend
DEPLOY_BUCKET=admin-kit-deploy-<ACCOUNT_ID>-us-west-2

aws s3 rm "s3://${BUCKET}" --recursive
aws s3 rm "s3://${DEPLOY_BUCKET}" --recursive
aws cloudformation delete-stack --stack-name "$STACK"
aws cloudformation wait stack-delete-complete --stack-name "$STACK"
aws s3 rb "s3://${BUCKET}"
aws s3 rb "s3://${DEPLOY_BUCKET}"
```

## 排坑

| 现象 | 处理 |
|---|---|
| API 403 | 非 GET 请求缺 `x-amz-content-sha256`；前端请求层已注入，脚本/curl 要手动加 |
| 前端 403 | S3 桶策略必须授信本 distribution 的 OAC |
| 登录后立即 401 | 检查 Cookie `Secure`/同源；本地 HTTP 用 `ADMIN_KIT_COOKIE_SECURE=false` |
| `admin-kit-dev-users` 查询 500 | 检查 GSI `gsi1` 是否 ACTIVE/回填完成 |
| 前端刷新 404 | 使用 hash 路由；history 路由需要 CloudFront fallback |
| Lambda 冷启动报 SSM | 检查 IAM `/admin-kit/{env}/*` 与 KMS decrypt 权限 |
| 层不存在 | `WebAdapterLayerArn` 必须匹配区域/架构 |