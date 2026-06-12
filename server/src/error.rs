use thiserror::Error;
use tonic::Status;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid argument: {0}")]
    Invalid(String),
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("decode error")]
    Decode(#[from] prost::DecodeError),
}

impl From<AppError> for Status {
    fn from(err: AppError) -> Self {
        match err {
            AppError::NotFound(msg) => Status::not_found(msg),
            AppError::Invalid(msg) => Status::invalid_argument(msg),
            AppError::Db(e) => Status::internal(format!("db: {}", e)),
            AppError::Decode(e) => Status::internal(format!("decode: {}", e)),
        }
    }
}
