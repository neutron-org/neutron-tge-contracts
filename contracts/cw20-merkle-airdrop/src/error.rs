use cosmwasm_std::{StdError, Timestamp};
use hex::FromHexError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Hex(#[from] FromHexError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Already claimed")]
    Claimed {},

    #[error("Wrong length")]
    WrongLength {},

    #[error("Verification failed")]
    VerificationFailed {},

    #[error("Cannot migrate from different contract type: {previous_contract}")]
    CannotMigrate { previous_contract: String },

    #[error("Airdrop expired at {expiration}")]
    Expired { expiration: Timestamp },

    #[error("Airdrop begins at {start}")]
    NotBegun { start: Timestamp },

    #[error("Airdrop is paused")]
    Paused {},

    #[error("Airdrop is not paused")]
    NotPaused {},

    #[error("Semver parsing error: {0}")]
    SemVer(String),

    #[error("Credits contract address is not set")]
    CreditsAddress {},

    #[error("Reserve contract address is not set")]
    ReserveAddress {},
}

impl From<semver::Error> for ContractError {
    fn from(err: semver::Error) -> Self {
        Self::SemVer(err.to_string())
    }
}
