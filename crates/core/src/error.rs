//! 统一的 API 错误类型。
//!
//! 所有错误最终都映射为结构化的 JSON 响应，格式统一：
//! `{"error": {"code": "NOT_FOUND", "message": "..."}}`。
//!
//! 相比 operon-core 的唯一扩展是 [`AppError::BadGateway`]（HTTP 502）。
//! 该变体携带业务错误码，便于 AI / GitHub OAuth 返回 `AI_ERROR`、
//! `AI_NOT_CONFIGURED`、`OAUTH_ERROR` 等 §5.5 约定的 code，而状态码统一 502。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    /// 邮箱已被注册（`409 EMAIL_TAKEN`）。
    #[error("email already registered")]
    EmailTaken,
    #[error("bad gateway ({code}): {message}")]
    BadGateway { code: String, message: String },
    #[error("internal: {0}")]
    Internal(String),
}

impl AppError {
    /// 通用 502（code = `BAD_GATEWAY`）。
    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self::BadGateway {
            code: "BAD_GATEWAY".into(),
            message: message.into(),
        }
    }

    /// 带业务错误码的 502，例如 `AI_ERROR` / `AI_NOT_CONFIGURED` / `OAUTH_ERROR`。
    pub fn bad_gateway_with_code(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::BadGateway {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::EmailTaken => StatusCode::CONFLICT,
            Self::BadGateway { .. } => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// 对外错误码。
    pub fn code(&self) -> &str {
        match self {
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::NotFound(_) => "NOT_FOUND",
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::Conflict(_) => "CONFLICT",
            Self::EmailTaken => "EMAIL_TAKEN",
            Self::BadGateway { code, .. } => code,
            Self::Internal(_) => "INTERNAL",
        }
    }
}

/// 任何 `anyhow::Error` 都能直接 `?` 冒泡成内部错误。
impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: String,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let code = self.code().to_string();
        let message = match &self {
            Self::Unauthorized(m)
            | Self::Forbidden(m)
            | Self::NotFound(m)
            | Self::BadRequest(m)
            | Self::Conflict(m)
            | Self::Internal(m) => m.clone(),
            Self::EmailTaken => "email already registered".to_string(),
            Self::BadGateway { message, .. } => message.clone(),
        };
        tracing::warn!(%code, message = %message, "request failed");
        (
            status,
            Json(ErrorBody {
                error: ErrorDetail { code, message },
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
    fn status_code_mapping() {
        assert_eq!(
            AppError::Unauthorized("x".into()).status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            AppError::Forbidden("x".into()).status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            AppError::NotFound("x".into()).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            AppError::BadRequest("x".into()).status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AppError::Conflict("x".into()).status(),
            StatusCode::CONFLICT
        );
        assert_eq!(AppError::bad_gateway("x").status(), StatusCode::BAD_GATEWAY);
        assert_eq!(
            AppError::Internal("x".into()).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn error_codes() {
        assert_eq!(AppError::bad_gateway("x").code(), "BAD_GATEWAY");
        assert_eq!(
            AppError::bad_gateway_with_code("AI_ERROR", "x").code(),
            "AI_ERROR"
        );
        assert_eq!(
            AppError::bad_gateway_with_code("AI_NOT_CONFIGURED", "x").code(),
            "AI_NOT_CONFIGURED"
        );
        assert_eq!(AppError::EmailTaken.code(), "EMAIL_TAKEN");
        assert_eq!(AppError::EmailTaken.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn json_error_format() {
        let resp = AppError::NotFound("用户不存在".into()).into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "NOT_FOUND");
        assert_eq!(json["error"]["message"], "用户不存在");
    }

    #[tokio::test]
    async fn bad_gateway_json_format() {
        let resp = AppError::bad_gateway_with_code("AI_ERROR", "DeepSeek 限流，请稍后重试")
            .into_response();
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "AI_ERROR");
        assert_eq!(json["error"]["message"], "DeepSeek 限流，请稍后重试");
    }
}
