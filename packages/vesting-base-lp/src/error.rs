use cosmwasm_std::{Decimal, OverflowError, StdError};
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

    #[error("Contract is in migration state. Please wait for migration to complete.")]
    MigrationIncomplete {},

    #[error(
    "Provided slippage tolerance {slippage_tolerance} is more than the max allowed {max_slippage_tolerance}"
    )]
    MigrationSlippageToBig {
        slippage_tolerance: Decimal,
        max_slippage_tolerance: Decimal,
    },

    #[error("Migration is complete")]
    MigrationComplete {},
}

#[allow(clippy::from_over_into)]
impl Into<StdError> for ContractError {
    fn into(self) -> StdError {
        StdError::generic_err(self.to_string())
    }
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}

pub fn ext_unsupported_err(extension: impl Into<String> + std::fmt::Display) -> StdError {
    StdError::generic_err(format!(
        "Extension is not enabled for the contract: {}.",
        extension
    ))
}
