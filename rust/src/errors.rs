//! A single error type whose `Display` matches Python's `str(exception)` for every
//! error this application raises, because those strings end up in notifications.

#[derive(Debug, Clone)]
pub struct AppError {
    pub message: String,
}

impl AppError {
    pub fn new(message: impl Into<String>) -> AppError {
        AppError { message: message.into() }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<crate::http::HttpError> for AppError {
    fn from(error: crate::http::HttpError) -> AppError {
        AppError::new(error.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> AppError {
        AppError::new(error.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// `KeyError('results')` renders as `'results'`.
pub fn key_error(key: &str) -> AppError {
    AppError::new(crate::pyrepr::repr_str(key))
}

/// `ValueError("...")` renders as its message.
pub fn value_error(message: impl Into<String>) -> AppError {
    AppError::new(message)
}
