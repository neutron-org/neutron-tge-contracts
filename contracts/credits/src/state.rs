use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
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
pub struct Allocation {
    // pub start_timestamp: Timestamp, // why we need start_timestamp in add_vesting?
    // pub end_timestamp: Timestamp,
    pub allocated_amount: Uint128,
    pub withdrawn_amount: Uint128,
    pub schedule: Schedule,
}

#[cw_serde]
pub struct Schedule {
    /// Time when vesting/unlocking starts
    pub start_time: u64,
    /// Time before with no token is to be vested/unlocked
    pub cliff: u64,
    /// Duration of the vesting/unlocking process. At time `start_time + duration`, the tokens are
    /// vested/unlocked in full
    pub duration: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");

// assume that we cannot set vesting multiple times for same address
pub const ALLOCATIONS: Map<&Addr, Allocation> = Map::new("vestings");
