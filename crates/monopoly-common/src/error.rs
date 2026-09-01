use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 面向客户端的错误类别（HTTP 状态码映射见 server 层）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorKind {
    BadRequest,
    NotFound,
    Conflict,
    Forbidden,
    Internal,
}

/// 服务器统一的业务错误。
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("资源不存在: {0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("服务器内部错误: {0}")]
    Internal(String),
}

impl ApiError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            ApiError::BadRequest(_) => ErrorKind::BadRequest,
            ApiError::NotFound(_) => ErrorKind::NotFound,
            ApiError::Conflict(_) => ErrorKind::Conflict,
            ApiError::Forbidden(_) => ErrorKind::Forbidden,
            ApiError::Internal(_) => ErrorKind::Internal,
        }
    }

    pub fn to_body(&self) -> ErrorBody {
        ErrorBody {
            code: format!("{:?}", self.kind()).to_lowercase(),
            message: self.to_string(),
        }
    }
}

/// 错误响应体（JSON 序列化用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}
