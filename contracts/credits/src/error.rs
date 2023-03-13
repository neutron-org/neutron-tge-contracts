use cosmwasm_std::StdError;
use cw20_base::ContractError as Cw20ContractError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Too early to claim")]
    TooEarlyToClaim,

    #[error("No funds to claim")]
    NoFundsToClaim,

    #[error("Incorrect funds supplied")]
    IncorrectFundsSupplied,

    #[error("No funds supplied")]
    NoFundsSupplied(),

    #[error("Airdrop address is not set")]
    AirdropNotConfigured,

    #[error("Lockdrop address is not set")]
    LockdropNotConfigured,

    #[error("When withdrawable is not set")]
    WhenWithdrawableIsNotConfigured,

    #[error("Address {address} is already vested")]
    AlreadyVested { address: String },

    #[error("transparent")]
    Cw20Error(#[from] Cw20ContractError),
}
