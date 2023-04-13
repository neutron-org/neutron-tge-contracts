use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    pub owner: Addr,
    pub credits_address: Addr,
    pub reserve_address: Addr,
}

pub const CONFIG_KEY: &str = "config";
pub const CONFIG: Item<Config> = Item::new(CONFIG_KEY);

pub const AIRDROP_START_KEY: &str = "airdrop_start";
pub const AIRDROP_START: Item<u64> = Item::new(AIRDROP_START_KEY);

pub const VESTING_START_KEY: &str = "vesting_start";
pub const VESTING_START: Item<u64> = Item::new(VESTING_START_KEY);

pub const VESTING_DURATION_KEY: &str = "vesting_duration_key";
pub const VESTING_DURATION: Item<u64> = Item::new(VESTING_DURATION_KEY);

pub const AMOUNT_KEY: &str = "amount";
pub const AMOUNT: Item<Uint128> = Item::new(AMOUNT_KEY);

pub const AMOUNT_CLAIMED_KEY: &str = "claimed_amount";
pub const AMOUNT_CLAIMED: Item<Uint128> = Item::new(AMOUNT_CLAIMED_KEY);

// saves external network airdrop accounts
pub const ACCOUNT_MAP_KEY: &str = "account_map";
// external_address -> host_address
pub const ACCOUNT_MAP: Map<String, String> = Map::new(ACCOUNT_MAP_KEY);

pub const MERKLE_ROOT_PREFIX: &str = "merkle_root";
pub const MERKLE_ROOT: Item<String> = Item::new(MERKLE_ROOT_PREFIX);

pub const CLAIM_PREFIX: &str = "claim";
pub const CLAIM: Map<String, bool> = Map::new(CLAIM_PREFIX);

pub const HRP_PREFIX: &str = "hrp";
pub const HRP: Item<String> = Item::new(HRP_PREFIX);

pub const PAUSED_KEY: &str = "paused";
pub const PAUSED: Item<bool> = Item::new(PAUSED_KEY);
