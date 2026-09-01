use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use monopoly_common::error::{ApiError as CommonError, ErrorKind};

/// 处理器统一返回类型。
pub type ApiResult<T> = Result<T, ApiError>;

/// 服务器本地错误类型：包装通用错误并实现 `IntoResponse`（避免孤儿规则）。
#[derive(Debug)]
pub struct ApiError(CommonError);

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self(CommonError::BadRequest(msg.into()))
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self(CommonError::NotFound(msg.into()))
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self(CommonError::Forbidden(msg.into()))
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self(CommonError::Internal(msg.into()))
    }
}

impl From<CommonError> for ApiError {
    fn from(e: CommonError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0.kind() {
            ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::Conflict => StatusCode::CONFLICT,
            ErrorKind::Forbidden => StatusCode::FORBIDDEN,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(self.0.to_body())).into_response()
    }
}
