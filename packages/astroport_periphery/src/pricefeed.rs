use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Coin, Uint64};

#[cw_serde]
pub struct PriceFeedRate {
    // Rate of an asset relative to USD
    pub rate: Uint64,
    // The resolve time of the request ID
    pub resolve_time: Uint64,
    // The request ID where the rate was derived from
    pub request_id: Uint64,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(String)]
    GetError {},
    #[returns(Vec<PriceFeedRate>)]
    // Returns the RefData of a given symbol
    GetRate {},
}

#[cw_serde]
pub struct InstantiateMsg {
    // A unique ID for the oracle request
    pub client_id: String,
    // The oracle script ID to query
    pub oracle_script_id: Uint64,
    // The number of validators that are requested to respond
    pub ask_count: Uint64,
    // The minimum number of validators that need to respond
    pub min_count: Uint64,
    // The maximum amount of band in uband to be paid to the data source providers
    // e.g. vec![Coin::new(100, "uband")]
    pub fee_limit: Vec<Coin>,
    // Amount of gas to pay to prepare raw requests
    pub prepare_gas: Uint64,
    // Amount of gas reserved for execution
    pub execute_gas: Uint64,
    // Minimum number of sources required to return a successful response
    pub multiplier: Uint64,
    // The list of symbols to query
    pub symbols: Vec<String>,
    // The owner of the contract
    pub owner: Option<String>,
    // The maximum time interval between updates
    pub max_update_interval: Option<u64>,
}

#[cw_serde]
pub struct Config {
    pub client_id: String,
    pub oracle_script_id: Uint64,
    pub ask_count: Uint64,
    pub min_count: Uint64,
    pub fee_limit: Vec<Coin>,
    pub prepare_gas: Uint64,
    pub execute_gas: Uint64,
    pub multiplier: Uint64,
    pub symbols: Vec<String>,
    pub max_update_interval: u64,
    pub owner: Addr,
}

#[cw_serde]
pub struct UpdateConfigMsg {
    pub client_id: Option<String>,
    pub oracle_script_id: Option<Uint64>,
    pub ask_count: Option<Uint64>,
    pub min_count: Option<Uint64>,
    pub fee_limit: Option<Vec<Coin>>,
    pub prepare_gas: Option<Uint64>,
    pub execute_gas: Option<Uint64>,
    pub multiplier: Option<Uint64>,
    pub symbols: Option<Vec<String>>,
    pub max_update_interval: Option<u64>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Request {},
    UpdateConfig { new_config: UpdateConfigMsg },
    UpdateOwner { new_owner: String },
}
