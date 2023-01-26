use crate::msg::{Config, DepositInfo};
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

pub const CONFIG: Item<Config> = Item::new("config");

pub const TOTAL_TOKEN: Item<Uint128> = Item::new("total_token");
pub const TOTAL_DEPOSIT: Item<Uint128> = Item::new("total_deposit");
pub const DEPOSITS: Map<&Addr, DepositInfo> = Map::new("deposits");
