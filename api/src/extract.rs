//! 统一错误格式的 axum 提取器包装。
//!
//! axum 原生 `Json` / `Query` 的 rejection 默认返回纯文本；本模块把它们映射为
//! [`ApiError::bad_request`]，保证 API 始终返回
//! `{"error":{"code":"BAD_REQUEST","message":"..."}}`。

use axum::extract::{FromRequest, FromRequestParts, Query, Request};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use crate::error::ApiError;

/// JSON body（必填）。
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let axum::Json(value) = axum::Json::<T>::from_request(req, state)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        Ok(Self(value))
    }
}

/// Query string。
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|e| ApiError::bad_request(e.body_text()))?;
        Ok(Self(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::routing::{get, post};
    use axum::Router;
    use serde::Deserialize;
    use tower::ServiceExt;

    #[derive(Deserialize)]
    struct Payload {
        name: String,
    }

    #[derive(Deserialize)]
    struct QueryPayload {
        limit: usize,
    }

    fn app() -> Router {
        Router::new()
            .route(
                "/json",
                post(|ApiJson(body): ApiJson<Payload>| async move { body.name }),
            )
            .route(
                "/query",
                get(|ApiQuery(q): ApiQuery<QueryPayload>| async move { q.limit.to_string() }),
            )
    }

    #[tokio::test]
    async fn valid_json_and_query() {
        let res = app()
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri("/json")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name":"hi"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res = app()
            .oneshot(
                HttpRequest::builder()
                    .uri("/query?limit=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn malformed_json_is_bad_request_envelope() {
        let res = app()
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri("/json")
                    .header("content-type", "application/json")
                    .body(Body::from("{not json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["code"], "BAD_REQUEST");
    }

    #[tokio::test]
    async fn malformed_query_is_bad_request_envelope() {
        let res = app()
            .oneshot(
                HttpRequest::builder()
                    .uri("/query?limit=abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["code"], "BAD_REQUEST");
    }
}
