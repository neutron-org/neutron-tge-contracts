use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("post_initialize called multiple times")]
    DuplicatePostInit {},

    #[error("Invalid TGE config {text}")]
    InvalidEventConfig { text: String },

    #[error("Empty TGE config")]
    EmptyEventConfig {},

    #[error("Invalid deposit: {text}")]
    DepositError { text: String },

    #[error("Invalid withdraw: {text}")]
    WithdrawError { text: String },

    #[error("Invalid withdraw tokens: {text}")]
    WithdrawTokensError { text: String },

    #[error("Invalid reserve withdraw: {text}")]
    InvalidReserveWithdraw { text: String },

    #[error("Invalid release tokens: {text}")]
    ReleaseTokensError { text: String },

    #[error("Fee can not be bigger than 1")]
    InvalidFee {},
}
