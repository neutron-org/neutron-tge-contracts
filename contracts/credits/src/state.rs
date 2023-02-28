use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    /// DAO contract address
    pub dao_address: Addr,
    /// Airdrop contract address
    pub airdrop_address: Option<Addr>,
    /// Lockdrop contract address
    pub lockdrop_address: Option<Addr>,
    /// When can start withdrawing NTRN funds
    pub when_withdrawable: Timestamp,
}

#[cw_serde]
pub struct Allocation {
    /// Total allocated amount that can be withdrawn
    pub allocated_amount: Uint128,
    /// Amount that has already been withdrawn from account (Does not include reward withdraws)
    pub withdrawn_amount: Uint128,
    /// Vesting schedule settings for this allocation
    pub schedule: Schedule,
}

#[cw_serde]
pub struct Schedule {
    /// Timestamp in UNIX seconds when vesting/unlocking starts
    pub start_time: u64,
    /// Specified in seconds. Tokens start to get unlocked at `start_time + cliff` time.
    pub cliff: u64,
    /// Duration of the vesting/unlocking process.
    /// At time `start_time + duration`, 100% of the tokens are vested/unlocked in full.
    pub duration: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");

/// Assume that we cannot set vesting multiple times for same address
/// Vested allocations of cNTRN
pub const ALLOCATIONS: Map<&Addr, Allocation> = Map::new("allocations");
