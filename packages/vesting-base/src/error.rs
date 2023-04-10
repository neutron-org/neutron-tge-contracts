use cosmwasm_std::{OverflowError, StdError};
use cw_utils::PaymentError;
use thiserror::Error;

/// This enum describes generator vesting contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Amount is not available!")]
    AmountIsNotAvailable {},

    #[error("Vesting schedule error on addr: {0}. Should satisfy: (start < end and at_start < total) or (start = end and at_start = total)")]
    VestingScheduleError(String),

    #[error("Vesting schedule amount error. The total amount should be equal to the CW20 receive amount.")]
    VestingScheduleAmountError {},

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Vesting token is not set!")]
    VestingTokenIsNotSet {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
