# Security

## Reporting a vulnerability

Please use GitHub **Security Advisories** (“Report a vulnerability”) on this
repository. Do not open a public issue for an unfixed security bug.

Include: affected component/commit, reproduction steps, impact, and any suggested
fix. We will acknowledge and coordinate a fix.

## Security model

- **Session**: Ed25519-signed JWT in an HttpOnly, Secure, SameSite=Lax Cookie.
  No tokens/passwords in localStorage.
- **Passwords**: argon2id; hashes only in DynamoDB; never logged or returned.
- **Authorization**: server-side `AdminAuth` / role checks. Frontend menu hiding
  is presentation only.
- **Immediate revocation**: disabling an account, changing its role, or resetting
  its password increments `session_version`; existing sessions fail on the next
  request with 401.
- **Bootstrap admin**: SSM `admin_email` + `admin_password`; lazy account creation
  only when the email does not exist. Existing `user` accounts are not promoted.
- **Secrets**: SSM SecureString; Lambda role can read only `/admin-kit/{env}/*`;
  no secrets in source, templates, or logs.
- **Frontend**: no AWS/DynamoDB credentials in the browser; production source maps
  disabled; no third-party telemetry/CDN/fonts.
- **CloudFront**: Lambda Function URL is `AWS_IAM` + OAC; the bucket is private.
  Mutating requests must include `x-amz-content-sha256`.

## Known gaps

- **No login rate limiting** in this version. If the admin is publicly reachable,
  put AWS WAF rate-based rules / a rate limiter in front of `/api/auth/login`.
- No MFA, email verification, self-service password reset, or signup.
- No CSRF token; the design relies on same-origin JSON requests + SameSite=Lax.
  If you add cross-site deployments, add CSRF protection and CORS credentials.
- No CSP/ResponseHeadersPolicy by default; add if your deployment requires it.
- Admin console is not a security boundary by itself; protect the distribution
  (WAF, IP allowlist, VPN) if it is sensitive.

## Deployment checklist

- [ ] `ADMIN_KIT_COOKIE_SECURE=true` in production.
- [ ] Admin email/password read from SSM, not committed.
- [ ] No `dynamodb:Scan` in the Lambda IAM policy.
- [ ] CloudFront behavior `/api/*`: CachingDisabled + AllViewerExceptHostHeader.
- [ ] HTML no-cache; assets immutable.
- [ ] WAF/rate limit on login if public.
- [ ] CloudTrail/CloudWatch alarms for repeated 401/5xx (recommended).
- [ ] Rotate `admin_password` by deleting the SSM parameter and redeploying
      (this only affects bootstrap; existing accounts keep their hashes).