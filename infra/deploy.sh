#!/usr/bin/env bash
# admin-kit 独立后台：DynamoDB + Lambda + CloudFront + S3 端到端部署
#
# 用法：
#   ./deploy.sh
#   PROJECT=admin-kit ENV=dev ./deploy.sh
#   ADMIN_KIT_ADMIN_EMAIL=you@example.com ./deploy.sh
#
# 安全：
#   - 凭证只从项目根 .env 或环境变量读取，不打印；
#   - admin_password 只生成一次并写入 SSM SecureString，不打印明文；
#   - 不操作 admin-kit-dev 等已有栈（本脚本只使用 admin-kit-*）。
set -euo pipefail

if [ -f "$HOME/proxy.sh" ]; then
  # shellcheck disable=SC1090
  source "$HOME/proxy.sh"
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

if [ -f "$ROOT/.env" ]; then
  set -a
  # shellcheck disable=SC1090
  source "$ROOT/.env"
  set +a
fi

: "${AWS_ACCESS_KEY_ID:?AWS_ACCESS_KEY_ID 未设置（项目根 .env）}"
: "${AWS_SECRET_ACCESS_KEY:?AWS_SECRET_ACCESS_KEY 未设置}"
export AWS_DEFAULT_REGION="${AWS_DEFAULT_REGION:-us-west-2}"
export AWS_REGION="${AWS_REGION:-$AWS_DEFAULT_REGION}"
export AWS_PAGER=""

REGION="$AWS_DEFAULT_REGION"
PROJECT="${PROJECT:-admin-kit}"
ENV="${ENV:-dev}"
STACK_NAME="${PROJECT}-${ENV}"
PARAMS_FILE="$SCRIPT_DIR/params.json"

param_get() {
  python3 - "$PARAMS_FILE" "$1" <<'PY'
import json, sys
p = json.load(open(sys.argv[1]))
v = p.get(sys.argv[2], "")
if isinstance(v, bool):
    print("true" if v else "false")
elif v is None:
    print("")
else:
    print(v)
PY
}

if [ ! -f "$PARAMS_FILE" ]; then
  cp "$SCRIPT_DIR/params.example.json" "$PARAMS_FILE"
  echo "已生成 $PARAMS_FILE，请检查后重跑" >&2
  exit 1
fi

ADMIN_EMAIL="${ADMIN_KIT_ADMIN_EMAIL:-$(param_get AdminEmail)}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@example.com}"
case "$ADMIN_EMAIL" in
  *@*.*) ;;
  *) echo "ERROR: AdminEmail 不合法: $ADMIN_EMAIL" >&2; exit 1 ;;
esac

ACCOUNT_ID="$(aws sts get-caller-identity --query Account --output text)"
DEPLOY_BUCKET="${ADMIN_KIT_DEPLOY_BUCKET:-${PROJECT}-deploy-${ACCOUNT_ID}-${REGION}}"
FRONTEND_BUCKET="${PROJECT}-${ENV}-frontend"
SECRETS_PATH="/${PROJECT}/${ENV}/"
LAYER_ARN="$(param_get WebAdapterLayerArn)"
ARCH="$(param_get LambdaArchitecture)"
MEMORY="$(param_get LambdaMemoryMB)"

step() { printf '\n====> %s\n' "$1"; }

echo "account=${ACCOUNT_ID} region=${REGION} stack=${STACK_NAME}"
echo "frontend=s3://${FRONTEND_BUCKET} deploy=s3://${DEPLOY_BUCKET}"

# 防止误操作线上 admin-kit 栈
case "$STACK_NAME" in
  admin-kit-dev|moxie-dev|operon-dev)
    echo "ERROR: 拒绝操作受保护栈 $STACK_NAME" >&2; exit 1 ;;
esac

ssm_exists() {
  aws ssm get-parameter --name "$1" --with-decryption \
    --query Parameter.Value --output text >/dev/null 2>&1
}

# ---- 1. SSM ----
step "[1/7] 确保 SSM 密钥存在"
if ssm_exists "${SECRETS_PATH}jwt_seed"; then
  echo "      jwt_seed 已存在"
else
  SEED="$(openssl rand -base64 32)"
  aws ssm put-parameter --name "${SECRETS_PATH}jwt_seed" \
    --type SecureString --value "$SEED" >/dev/null
  unset SEED
  echo "      jwt_seed 已创建"
fi

aws ssm put-parameter --name "${SECRETS_PATH}admin_email" --type String \
  --value "$ADMIN_EMAIL" --overwrite >/dev/null
echo "      admin_email=${ADMIN_EMAIL}"

if ssm_exists "${SECRETS_PATH}admin_password"; then
  echo "      admin_password 已存在（重置：删除该参数后重跑）"
else
  if [ -n "${ADMIN_KIT_ADMIN_PASSWORD:-}" ]; then
    ADMIN_PASSWORD="$ADMIN_KIT_ADMIN_PASSWORD"
  else
    ADMIN_PASSWORD="$(openssl rand -hex 16)"
  fi
  aws ssm put-parameter --name "${SECRETS_PATH}admin_password" \
    --type SecureString --value "$ADMIN_PASSWORD" >/dev/null
  unset ADMIN_PASSWORD
  echo "      admin_password 已创建（读取：aws ssm get-parameter --name ${SECRETS_PATH}admin_password --with-decryption --query Parameter.Value --output text）"
fi

# ---- 2. Rust ----
step "[2/7] 编译 admin-kit-api（musl）"
rustup target add x86_64-unknown-linux-musl >/dev/null 2>&1 || true
cargo build --release --target x86_64-unknown-linux-musl -p admin-kit-api
BIN="$ROOT/target/x86_64-unknown-linux-musl/release/admin-kit-api"
[ -f "$BIN" ] || { echo "ERROR: 未找到 $BIN" >&2; exit 1; }

# ---- 3. 打包上传 ----
step "[3/7] 打包 bootstrap.zip 并上传"
ZIP="$ROOT/target/admin-kit-bootstrap.zip"
PKG_DIR="$(mktemp -d)"
cleanup() { rm -rf "$PKG_DIR"; rm -f "$ZIP"; }
trap cleanup EXIT
rm -f "$ZIP"
cp "$BIN" "$PKG_DIR/bootstrap"
( cd "$PKG_DIR" && zip -q "$ZIP" bootstrap )
ls -lh "$ZIP"

if ! aws s3api head-bucket --bucket "$DEPLOY_BUCKET" >/dev/null 2>&1; then
  if [ "$REGION" = "us-east-1" ]; then
    aws s3api create-bucket --bucket "$DEPLOY_BUCKET" --region "$REGION" >/dev/null
  else
    aws s3api create-bucket --bucket "$DEPLOY_BUCKET" --region "$REGION" \
      --create-bucket-configuration LocationConstraint="$REGION" >/dev/null
  fi
  echo "      已创建部署桶 $DEPLOY_BUCKET"
fi
CODE_KEY="code/$(date +%s)/bootstrap.zip"
aws s3api put-object --bucket "$DEPLOY_BUCKET" --key "$CODE_KEY" --body "$ZIP" \
  --cli-read-timeout 290 --cli-connect-timeout 30 >/dev/null
echo "      已上传 s3://${DEPLOY_BUCKET}/${CODE_KEY}"

# ---- 4. CloudFormation ----
step "[4/7] CloudFormation 部署 ${STACK_NAME}"
aws cloudformation deploy \
  --stack-name "$STACK_NAME" \
  --template-file "$SCRIPT_DIR/template.yaml" \
  --parameter-overrides \
    "Project=${PROJECT}" \
    "Environment=${ENV}" \
    "LambdaArchitecture=${ARCH}" \
    "LambdaMemoryMB=${MEMORY}" \
    "CodeBucket=${DEPLOY_BUCKET}" \
    "CodeKey=${CODE_KEY}" \
    "WebAdapterLayerArn=${LAYER_ARN}" \
  --capabilities CAPABILITY_NAMED_IAM \
  --no-fail-on-empty-changeset

get_output() {
  aws cloudformation describe-stacks --stack-name "$STACK_NAME" \
    --query "Stacks[0].Outputs[?OutputKey=='$1'].OutputValue" --output text
}
SITE_URL="$(get_output SiteUrl)"
CF_DOMAIN="$(get_output CloudFrontDomain)"
DIST_ID="$(get_output DistributionId)"
TABLE_NAME="$(get_output UsersTableName)"
FUNCTION_URL="$(get_output FunctionUrl)"
echo "      site=${SITE_URL}"

# ---- 5. 前端 ----
step "[5/7] 构建 Soybean Admin 并同步到 s3://${FRONTEND_BUCKET}"
[ -f "$ROOT/web/package.json" ] || {
  echo "ERROR: web 尚未就绪" >&2; exit 1;
}
[ -f "$ROOT/web/pnpm-lock.yaml" ] || echo "WARN: 缺少 pnpm-lock.yaml" >&2

(
  cd "$ROOT/web"
  if command -v pnpm >/dev/null 2>&1; then
    pnpm install --frozen-lockfile
    pnpm build
  else
    corepack pnpm install --frozen-lockfile
    corepack pnpm build
  fi
)
[ -f "$ROOT/web/dist/index.html" ] || {
  echo "ERROR: web/dist/index.html 不存在" >&2; exit 1;
}

aws s3 sync "$ROOT/web/dist/" "s3://${FRONTEND_BUCKET}/" --delete \
  --exclude "*.html" \
  --cache-control "public,max-age=31536000,immutable" >/dev/null
for f in index.html; do
  if [ -f "$ROOT/web/dist/$f" ]; then
    aws s3 cp "$ROOT/web/dist/$f" "s3://${FRONTEND_BUCKET}/$f" \
      --content-type "text/html; charset=utf-8" \
      --cache-control "no-cache" >/dev/null
  fi
done
echo "      前端已同步"

# ---- 6. 失效缓存 ----
step "[6/7] CloudFront 失效缓存"
aws cloudfront create-invalidation --distribution-id "$DIST_ID" --paths "/*" >/dev/null
echo "      已提交 invalidation"

# ---- 7. 验证 ----
step "[7/7] 验证"
echo "站点:          ${SITE_URL}"
echo "CloudFront:    ${CF_DOMAIN}"
echo "Distribution:  ${DIST_ID}"
echo "DynamoDB:      ${TABLE_NAME}"
echo "Function URL:  ${FUNCTION_URL}（仅 CloudFront OAC 可调用）"
echo
echo "管理员登录:    ${ADMIN_EMAIL}"
echo "读取密码:      aws ssm get-parameter --name ${SECRETS_PATH}admin_password --with-decryption --query Parameter.Value --output text"
echo "（密码不会打印在本终端；请用上面命令读取）"
echo
echo "健康检查（CloudFront 首次生效可能需要 1-5 分钟）："
for i in $(seq 1 12); do
  code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 20 "${SITE_URL}/health" || true)"
  echo "  attempt ${i}: HTTP ${code}"
  [ "$code" = "200" ] && break
  sleep 15
done
echo
echo "验证步骤："
echo "  1. 打开 ${SITE_URL}"
echo "  2. 用 ${ADMIN_EMAIL} + SSM 密码登录"
echo "  3. 进入“用户管理”，创建/禁用/重置密码"
echo "  4. 刷新页面，确认 Cookie 会话保持"
