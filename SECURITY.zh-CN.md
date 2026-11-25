# 安全

[English](SECURITY.md) | **简体中文**

## 报告漏洞

请使用本仓库的 **GitHub Security Advisories**（“Report a vulnerability”）。
未修复的安全问题不要开公开 issue。

请包含：受影响组件/commit、复现步骤、影响、建议修复。我们会确认并协调修复。

## 安全模型

- **会话**：Ed25519 签名 JWT，存放在 HttpOnly、Secure、SameSite=Lax Cookie。
  不在 localStorage 保存 token/密码。
- **密码**：argon2id；DynamoDB 只存哈希；绝不记录或返回明文。
- **授权**：服务端 `AdminAuth`/角色校验。前端隐藏菜单只是展示。
- **即时撤销**：禁用账号、改角色、重置密码都会递增 `session_version`；
  旧会话下一次请求返回 401。
- **管理员引导**：SSM `admin_email` + `admin_password`；仅当邮箱不存在时创建账号。
  已存在的 `user` 账号不会自动提权。
- **密钥**：SSM SecureString；Lambda 角色只能读取 `/admin-kit/{env}/*`；
  源码、模板、日志中都没有密钥。
- **前端**：浏览器不持有 AWS/DynamoDB 凭证；生产关闭 source map；
  无第三方遥测/CDN/字体。
- **CloudFront**：Lambda Function URL 使用 `AWS_IAM` + OAC；S3 桶私有。
  写请求必须带 `x-amz-content-sha256`。

## 已知缺口

- **没有登录限流**。如果后台可从公网访问，请在 `/api/auth/login` 前加 AWS WAF
  速率规则/限流器。
- 没有 MFA、邮箱验证、自助找回密码、自助注册。
- 没有 CSRF token；设计依赖同源 JSON 请求 + SameSite=Lax。
  如果要做跨站部署，必须增加 CSRF 防护和 CORS credentials。
- 默认没有 CSP/ResponseHeadersPolicy；按部署要求自行添加。
- 管理后台本身不是安全边界；敏感场景请用 WAF、IP allowlist 或 VPN 保护。

## 部署检查清单

- [ ] 生产环境 `ADMIN_KIT_COOKIE_SECURE=true`。
- [ ] 管理员邮箱/密码只从 SSM 读取，不提交。
- [ ] Lambda IAM 中没有 `dynamodb:Scan`。
- [ ] CloudFront `/api/*`：CachingDisabled + AllViewerExceptHostHeader。
- [ ] HTML no-cache；assets immutable。
- [ ] 公网登录接口前有 WAF/限流。
- [ ] 建议开启 CloudTrail/CloudWatch 告警（重复 401/5xx）。
- [ ] 轮换 `admin_password`：删除 SSM 参数后重新部署
      （只影响 bootstrap；已有账号仍保留原哈希）。