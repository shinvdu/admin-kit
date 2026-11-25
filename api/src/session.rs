//! HttpOnly Cookie 会话（JWT 读写）。
//!
//! - `admin_session=<jwt>; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=2592000`
//! - 线上 `ADMIN_KIT_COOKIE_SECURE=true`（默认），本地 HTTP 开发显式设为 `false`；
//! - JS 不可读，避免 XSS 窃取；`SameSite=Lax` + JSON-only 写请求防 CSRF；
//! - 登出：`Max-Age=0` 覆盖同名 Cookie。

use axum::http::HeaderMap;

pub const SESSION_COOKIE: &str = "admin_session";
pub const SESSION_MAX_AGE_SECS: u64 = 2_592_000; // 30 天
/// JWT 有效期与 Cookie 有效期保持一致。
pub const SESSION_TTL_SECS: u64 = SESSION_MAX_AGE_SECS;

/// `ADMIN_KIT_COOKIE_SECURE` 默认 true；仅显式 `false/0/no` 时关闭（本地 HTTP 开发）。
pub fn cookie_secure() -> bool {
    match std::env::var("ADMIN_KIT_COOKIE_SECURE") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            !(v == "false" || v == "0" || v == "no")
        }
        Err(_) => true,
    }
}

pub fn session_cookie(jwt: &str) -> String {
    session_cookie_with_secure(jwt, cookie_secure())
}

pub fn session_cookie_with_secure(jwt: &str, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={jwt}; HttpOnly; {}SameSite=Lax; Path=/; Max-Age={SESSION_MAX_AGE_SECS}",
        if secure { "Secure; " } else { "" }
    )
}

pub fn clear_session_cookie() -> String {
    clear_session_cookie_with_secure(cookie_secure())
}

pub fn clear_session_cookie_with_secure(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; HttpOnly; {}SameSite=Lax; Path=/; Max-Age=0",
        if secure { "Secure; " } else { "" }
    )
}

/// 从 `Cookie` 头读取指定 Cookie（不依赖 cookie crate）。
pub fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            if key.trim() == name {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn secure_session_cookie_flags() {
        let c = session_cookie_with_secure("jwt.token", true);
        assert!(c.starts_with(
            "admin_session=jwt.token; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=2592000"
        ));
    }

    #[test]
    fn insecure_session_cookie_for_local_dev() {
        let c = session_cookie_with_secure("jwt.token", false);
        assert!(!c.contains("Secure"));
        assert!(c.contains("HttpOnly"));
        assert!(c.contains("SameSite=Lax"));
        assert!(c.contains("Path=/"));
        assert!(c.contains("Max-Age=2592000"));
    }

    #[test]
    fn clear_session_cookie_expires_immediately() {
        let c = clear_session_cookie_with_secure(true);
        assert!(c.starts_with("admin_session=;"));
        assert!(c.contains("Max-Age=0"));
        assert!(c.contains("Secure"));
    }

    #[test]
    fn read_cookie_from_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_static("foo=bar; admin_session=jwt.value; baz=qux"),
        );
        assert_eq!(
            read_cookie(&headers, SESSION_COOKIE).as_deref(),
            Some("jwt.value")
        );
        assert_eq!(read_cookie(&headers, "foo").as_deref(), Some("bar"));
        assert_eq!(read_cookie(&headers, "missing"), None);
        assert_eq!(read_cookie(&HeaderMap::new(), SESSION_COOKIE), None);
    }

    #[test]
    fn cookie_name_is_admin_session() {
        assert_eq!(SESSION_COOKIE, "admin_session");
    }
}
