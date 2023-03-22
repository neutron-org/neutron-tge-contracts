use crate::asset::{AssetInfo, PairInfo};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;
use cosmwasm_std::{Decimal256, Uint128, Uint256, Uint64};

/// This structure stores general parameters for the contract.
/// Modified by us
#[cw_serde]
pub struct InstantiateMsg {
    /// The factory contract address
    pub factory_contract: String,
    /// The assets that have a pool for which this contract provides price feeds
    pub asset_infos: Vec<AssetInfo>,
    /// Minimal interval between Update{}'s
    pub period: u64,
}

/// This structure describes the execute functions available in the contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Update/accumulate prices
    Update {},
    /// Update period
    UpdatePeriod { new_period: u64 },
}

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Calculates a new TWAP with updated precision
    #[returns(Vec<(AssetInfo, Uint256)>)]
    Consult {
        /// The asset for which to compute a new TWAP value
        token: AssetInfo,
        /// The amount of tokens for which to compute the token price
        amount: Uint128,
    },
    #[returns(Vec<(AssetInfo, Decimal256)>)]
    TWAPAtHeight {
        /// The asset for which to compute a new TWAP value
        token: AssetInfo,
        /// The amount of tokens for which to compute the token price
        height: Uint64,
    },
    /// Returns the contract's conriguration structure
    #[returns(Config)]
    Config {},
    /// Returns the timestamp of the block when the previous update happened
    #[returns(u64)]
    LastUpdateTimestamp {},
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {}

/// Global configuration for the contract
#[cw_serde]
pub struct Config {
    /// The address that's allowed to change contract parameters
    pub owner: Addr,
    /// The factory contract address
    pub factory: Addr,
    /// The assets in the pool. Each asset is described using a [`AssetInfo`]
    pub asset_infos: Vec<AssetInfo>,
    /// Information about the pair (LP token address, pair type etc)
    pub pair: PairInfo,
    /// Time between two consecutive TWAP updates.
    pub period: u64,
}
