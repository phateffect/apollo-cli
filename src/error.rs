use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApolloError {
    #[error("login failed; check the Apollo URL, username, and password")]
    LoginFailed,
    #[error("not logged in; run `apollo login` first")]
    NotAuthenticated,
    #[error("Apollo request failed with HTTP {status}: {message}")]
    Http { status: StatusCode, message: String },
    #[error("invalid Apollo response: {0}")]
    InvalidResponse(String),
}
