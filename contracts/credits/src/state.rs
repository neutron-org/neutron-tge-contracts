use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Timestamp};
use cw_storage_plus::Item;

#[cw_serde]
pub struct Config {
    /// Date when you can execute `burn` method to burn CNTRN and get NTRN tokens
    pub when_claimable: Timestamp,
    /// DAO contract address
    pub dao_address: Addr,
    /// Airdrop contract address
    pub airdrop_address: Addr,
    /// Sale contract address
    pub sale_address: Addr,
    /// Lockdrop contract address,
    pub lockdrop_address: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
