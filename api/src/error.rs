//! 明确业务错误码的 API 错误类型。
//!
//! 响应格式与 `admin_kit_core::AppError` 一致：
//! `{"error":{"code":"...","message":"..."}}`。
//!
//! 之所以不复用 `AppError`：本服务需要 `401 INVALID_CREDENTIALS`、
//! `400 SELF_ACTION`、`400 LAST_ADMIN` 这类与 HTTP 状态码不可一一推导的业务码。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", message)
    }

    /// 登录失败统一错误：不区分邮箱不存在 / 密码错误 / 账号禁用。
    pub fn invalid_credentials() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "INVALID_CREDENTIALS",
            "invalid credentials",
        )
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "FORBIDDEN", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "NOT_FOUND", message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "BAD_REQUEST", message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "CONFLICT", message)
    }

    pub fn email_taken() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "EMAIL_TAKEN",
            "email already registered",
        )
    }

    pub fn self_action(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "SELF_ACTION", message)
    }

    pub fn last_admin(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "LAST_ADMIN", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", message)
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // 只记录状态码与业务码；不记录 message（避免把邮箱等敏感信息写进日志）。
        tracing::warn!(status = %self.status, code = %self.code, "request failed");
        (
            self.status,
            Json(ErrorBody {
                error: ErrorDetail {
                    code: self.code,
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[test]
    fn status_and_code_matrix() {
        assert_eq!(
            ApiError::invalid_credentials().status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(ApiError::forbidden("x").status, StatusCode::FORBIDDEN);
        assert_eq!(ApiError::not_found("x").status, StatusCode::NOT_FOUND);
        assert_eq!(ApiError::bad_request("x").status, StatusCode::BAD_REQUEST);
        assert_eq!(ApiError::email_taken().status, StatusCode::CONFLICT);
        assert_eq!(ApiError::self_action("x").status, StatusCode::BAD_REQUEST);
        assert_eq!(ApiError::last_admin("x").status, StatusCode::BAD_REQUEST);
        assert_eq!(
            ApiError::internal("x").status,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(ApiError::email_taken().code, "EMAIL_TAKEN");
    }

    #[tokio::test]
    async fn nested_error_envelope() {
        let resp = ApiError::invalid_credentials().into_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["code"], "INVALID_CREDENTIALS");
        assert_eq!(json["error"]["message"], "invalid credentials");
        assert!(json.get("code").is_none());
    }
}
