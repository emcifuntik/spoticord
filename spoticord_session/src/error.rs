use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    /// The user executed an action inside of a channel that is not supported
    #[error("The specified channel is invalid for this operation")]
    InvalidChannel,

    /// Generic authentication failure
    #[error("Authentication failed")]
    AuthenticationFailed,

    /// Cannot perform this action on an active session
    #[error("Cannot perform this action on an active session")]
    AlreadyActive,

    /// Generic error with custom message
    #[error("{0}")]
    Other(String),

    #[error(transparent)]
    Serenity(#[from] serenity::Error),

    #[error(transparent)]
    Storage(#[from] anyhow::Error),

    #[error(transparent)]
    JoinError(#[from] songbird::error::JoinError),

    #[error(transparent)]
    Librespot(#[from] librespot::core::Error),
}

pub type Result<T> = ::core::result::Result<T, Error>;
