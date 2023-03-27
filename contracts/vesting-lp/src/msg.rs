use astroport::vesting::QueryMsg as BaseQueryMsg;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

/// This structure describes the query messages available in the contract.
#[cw_serde]
#[derive(QueryResponses)]
#[query_responses(nested)]
#[serde(untagged)]
pub enum QueryMsg {
    Base(BaseQueryMsg),
    Ext(ExtraQueryMsg),
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum ExtraQueryMsg {
    /// Returns the total unclaimed amount of tokens for a specific address at certain height.
    #[returns(Uint128)]
    UnclaimedAmountAtHeight { address: String, height: u64 },
    /// Returns the total unclaimed amount of tokens for a specific address at certain height.
    #[returns(Uint128)]
    UnclaimedTotalAmountAtHeight { height: u64 },
}
