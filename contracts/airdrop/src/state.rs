use astroport_periphery::airdrop::{Config, State, UserInfo};
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
/// Stores the global state of contract at the given key
pub const STATE: Item<State> = Item::new("state");
/// Stores user information for the specified address
pub const USERS: Map<&Addr, UserInfo> = Map::new("users");
