#!/usr/bin/env bash
# 线上冒烟：登录 → me → 用户列表 → 创建 → 禁用 → 被禁用用户登录失败
# 不打印密码/Cookie；只用于 admin-kit-dev。
set -euo pipefail

if [ -f "$HOME/proxy.sh" ]; then
  # shellcheck disable=SC1090
  source "$HOME/proxy.sh"
fi
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"
if [ -f "$ROOT/.env" ]; then
  set -a; # shellcheck disable=SC1090
  source "$ROOT/.env"; set +a
fi
export AWS_DEFAULT_REGION="${AWS_DEFAULT_REGION:-us-west-2}"
export AWS_REGION="${AWS_REGION:-$AWS_DEFAULT_REGION}"
export AWS_PAGER=""
STACK_NAME="${PROJECT:-admin-kit}-${ENV:-dev}"
SECRETS_PATH="/${PROJECT:-admin-kit}/${ENV:-dev}/"

get_output() {
  aws cloudformation describe-stacks --stack-name "$STACK_NAME" \
    --query "Stacks[0].Outputs[?OutputKey=='$1'].OutputValue" --output text
}
SITE="$(get_output SiteUrl)"
ADMIN_EMAIL="$(aws ssm get-parameter --name "${SECRETS_PATH}admin_email" \
  --query Parameter.Value --output text)"
ADMIN_PASSWORD="$(aws ssm get-parameter --name "${SECRETS_PATH}admin_password" \
  --with-decryption --query Parameter.Value --output text)"
[ -n "$SITE" ] && [ "$SITE" != "None" ] || { echo "找不到 SiteUrl" >&2; exit 1; }

sha() { printf '%s' "$1" | sha256sum | awk '{print $1}'; }
EMPTY_SHA="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
JAR="$TMP/cookies.txt"
pass=0; fail=0
check() { # name expected actual
  if [ "$2" = "$3" ]; then echo "  PASS  $1 ($3)"; pass=$((pass+1));
  else echo "  FAIL  $1 (expected $2, got $3)"; fail=$((fail+1)); fi
}
json_post() { # url body jar
  curl -sS -o "$TMP/body" -w '%{http_code}' --max-time 30 \
    -c "$3" -b "$3" -H 'content-type: application/json' \
    -H "x-amz-content-sha256: $(sha "$2")" -d "$2" "$1" || true
}
json_patch() { # url body jar
  curl -sS -o "$TMP/body" -w '%{http_code}' --max-time 30 \
    -c "$3" -b "$3" -H 'content-type: application/json' \
    -H "x-amz-content-sha256: $(sha "$2")" -X PATCH -d "$2" "$1" || true
}

echo "site=${SITE}"
echo "admin=${ADMIN_EMAIL}"

BODY="$(python3 -c 'import json,sys; print(json.dumps({"email":sys.argv[1],"password":sys.argv[2]}))' "$ADMIN_EMAIL" "$ADMIN_PASSWORD")"
code="$(json_post "${SITE}/api/auth/login" "$BODY" "$JAR")"
check "admin login" 200 "$code"
if [ "$code" != "200" ]; then echo "  body: $(head -c 300 "$TMP/body")"; exit 1; fi

code="$(curl -sS -o "$TMP/body" -w '%{http_code}' --max-time 30 -b "$JAR" "${SITE}/api/auth/me")"
check "GET /api/auth/me" 200 "$code"

code="$(curl -sS -o "$TMP/body" -w '%{http_code}' --max-time 30 -b "$JAR" "${SITE}/api/admin/users?limit=5")"
check "GET /api/admin/users" 200 "$code"
python3 - "$TMP/body" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
assert isinstance(d.get("items"), list), d
print("  users loaded:", len(d["items"]), "next_cursor:", bool(d.get("next_cursor")))
PY

code="$(curl -sS -o "$TMP/body" -w '%{http_code}' --max-time 30 -b "$JAR" "${SITE}/api/admin/stats")"
check "GET /api/admin/stats" 200 "$code"

TEST_EMAIL="smoke-$(date +%s)@example.com"
TEST_PASSWORD="Smoke-$(openssl rand -hex 8)"
CREATE="$(python3 -c 'import json,sys; print(json.dumps({"email":sys.argv[1],"password":sys.argv[2],"role":"user"}))' "$TEST_EMAIL" "$TEST_PASSWORD")"
code="$(json_post "${SITE}/api/admin/users" "$CREATE" "$JAR")"
check "create user" 201 "$code"
USER_ID="$(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(d.get("user",d).get("id",""))' "$TMP/body" 2>/dev/null || true)"
echo "  created=${TEST_EMAIL} id=${USER_ID}"
if [ -z "$USER_ID" ]; then echo "  body: $(head -c 300 "$TMP/body")"; exit 1; fi

# 新用户登录
JAR2="$TMP/cookies2.txt"
BODY2="$(python3 -c 'import json,sys; print(json.dumps({"email":sys.argv[1],"password":sys.argv[2]}))' "$TEST_EMAIL" "$TEST_PASSWORD")"
code="$(json_post "${SITE}/api/auth/login" "$BODY2" "$JAR2")"
check "new user login" 200 "$code"

# 禁用
code="$(json_patch "${SITE}/api/admin/users/${USER_ID}" '{"disabled":true}' "$JAR")"
check "disable user" 200 "$code"

# 旧会话立即失效
code="$(curl -sS -o "$TMP/body" -w '%{http_code}' --max-time 30 -b "$JAR2" "${SITE}/api/auth/me")"
check "disabled user old session revoked" 401 "$code"

# 禁用后登录失败
code="$(json_post "${SITE}/api/auth/login" "$BODY2" "$JAR2")"
check "disabled user login rejected" 401 "$code"

# 退出
code="$(curl -sS -o "$TMP/body" -w '%{http_code}' --max-time 30 \
  -b "$JAR" -H "x-amz-content-sha256: ${EMPTY_SHA}" -X POST "${SITE}/api/auth/logout")"
check "logout" 204 "$code"

# 清理冒烟用户（没有删除 API；直接删测试数据，避免污染后台列表）
TABLE="$(get_output UsersTableName)"
if [ -n "$USER_ID" ] && [ -n "$TABLE" ] && [ "$TABLE" != "None" ]; then
  aws dynamodb delete-item --table-name "$TABLE" \
    --key "{\"pk\":{\"S\":\"ACCOUNT#user:${USER_ID}\"},\"sk\":{\"S\":\"META\"}}" >/dev/null 2>&1 || true
  aws dynamodb delete-item --table-name "$TABLE" \
    --key "{\"pk\":{\"S\":\"EMAIL#${TEST_EMAIL}\"},\"sk\":{\"S\":\"META\"}}" >/dev/null 2>&1 || true
  echo "  cleaned smoke user ${TEST_EMAIL}"
fi

echo
echo "PASS=${pass} FAIL=${fail}"
[ "$fail" = "0" ]
