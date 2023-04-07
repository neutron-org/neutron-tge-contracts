use cosmwasm_std::StdError;
use thiserror::Error;

/// This enum describes oracle contract errors
#[derive(PartialEq, Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Period not elapsed")]
    WrongPeriod {},

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Asset infos are not set")]
    AssetInfosNotSet {},

    #[error("Invalid token")]
    InvalidToken {},
}
