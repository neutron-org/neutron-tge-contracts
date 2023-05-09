use cosmwasm_std::StdError;
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
    Expired { expiration: u64 },

    #[error("withdraw_all is unavailable, it will become available at {available_at}")]
    WithdrawAllUnavailable { available_at: u64 },

    #[error("Airdrop begins at {start}")]
    NotBegun { start: u64 },

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

    #[error("Vesting (at {vesting_start}) cannot start before airdrop (at {airdrop_start})")]
    VestingBeforeAirdrop {
        airdrop_start: u64,
        vesting_start: u64,
    },

    #[error("underfunded ibc fees: required {required_fees} got {got_fees}")]
    Underfunded { got_fees: u128, required_fees: u128 },
}

impl From<semver::Error> for ContractError {
    fn from(err: semver::Error) -> Self {
        Self::SemVer(err.to_string())
    }
}
