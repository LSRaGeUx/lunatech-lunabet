use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub struct AppError(pub anyhow::Error);

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(e: E) -> Self {
        AppError(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("request failed: {:#}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Erreur interne. Réessaie plus tard.",
        )
            .into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
