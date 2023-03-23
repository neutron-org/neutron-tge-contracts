use astroport::vesting::{
    ConfigResponse, OrderBy, VestingAccountResponse, VestingAccountsResponse,
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the configuration for the contract using a [`ConfigResponse`] object.
    #[returns(ConfigResponse)]
    Config {},
    /// Returns information about an address vesting tokens using a [`VestingAccountResponse`] object.
    #[returns(VestingAccountResponse)]
    VestingAccount { address: String },
    /// Returns a list of addresses that are vesting tokens using a [`VestingAccountsResponse`] object.
    #[returns(VestingAccountsResponse)]
    VestingAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    },
    /// Returns the total unvested amount of tokens for a specific address.
    #[returns(Uint128)]
    AvailableAmount { address: String },
    /// Timestamp returns the current timestamp
    #[returns(u64)]
    Timestamp {},
    /// Returns the total unclaimed amount of tokens for a specific address at certain height.
    #[returns(Uint128)]
    UnclaimedAmountAtHeight { address: String, height: u64 },
    /// Returns the total unclaimed amount of tokens for a specific address at certain height.
    #[returns(Uint128)]
    UnclaimedTotalAmountAtHeight { height: u64 },
}
