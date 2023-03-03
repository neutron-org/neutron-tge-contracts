use cosmwasm_std::{StdError, Uint128};
use cw_utils::{Expiration, Scheduled};
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

    #[error("Invalid input")]
    InvalidInput {},

    #[error("Already claimed")]
    Claimed {},

    #[error("Wrong length")]
    WrongLength {},

    #[error("Verification failed")]
    VerificationFailed {},

    #[error("Invalid token type")]
    InvalidTokenType {},

    #[error("Insufficient Funds: Contract balance: {balance} does not cover the required amount: {amount}")]
    InsufficientFunds { balance: Uint128, amount: Uint128 },

    #[error("Cannot migrate from different contract type: {previous_contract}")]
    CannotMigrate { previous_contract: String },

    #[error("Airdrop expired at {expiration}")]
    Expired { expiration: Expiration },

    #[error("Airdrop stage {stage} not expired yet")]
    StageNotExpired { stage: u8, expiration: Expiration },

    #[error("Airdrop begins at {start}")]
    NotBegun { start: Scheduled },

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

    #[error("Unknown reply id {id}")]
    UnknownReplyId { id: u64 },

    #[error("Cannot issue vesting message: {description}")]
    Vesting { description: String },
}

impl From<semver::Error> for ContractError {
    fn from(err: semver::Error) -> Self {
        Self::SemVer(err.to_string())
    }
}
