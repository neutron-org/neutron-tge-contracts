use astroport_periphery::auction::{Config, State, UserInfo};
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");
pub const USERS: Map<&Addr, UserInfo> = Map::new("users");
