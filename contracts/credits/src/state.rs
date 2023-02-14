use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    /// DAO contract address
    pub dao_address: Addr,
    /// Airdrop contract address
    pub airdrop_address: Option<Addr>,
    /// Lockdrop contract address,
    pub lockdrop_address: Option<Addr>,
}

#[cw_serde]
pub struct VestingItem {
    pub start_timestamp: Timestamp, // why we need start_timestamp in add_vesting?
    pub end_timestamp: Timestamp,
    pub amount: Uint128,
}

pub const CONFIG: Item<Config> = Item::new("config");

pub const VESTINGS: Map<(&Addr, u64), VestingItem> = Map::new("vestings");
