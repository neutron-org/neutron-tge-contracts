use cosmwasm_std::{Addr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    Info { address: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub owner: String,
    pub base_denom: String,
    pub reserve: String,
    pub token: String,
    pub slot_duration: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Deposit {},
    Withdraw { amount: Option<Uint128> },
    WithdrawTokens {},
    PostInitialize { config: EventConfig },
    ReleaseTokens {},
    WithdrawReserve {},
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: String,
    pub receiver: String,
    pub token: String,
    pub launch_config: Option<EventConfig>,
    pub base_denom: String,
    pub tokens_released: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct DepositResponse {
    pub deposit: Uint128,
    pub total_deposit: Uint128,
    pub withdrawable_amount: Uint128,
    pub tokens_to_claim: Uint128,
    pub can_claim: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub token: Addr,
    pub event_config: Option<EventConfig>,
    pub base_denom: String,
    pub tokens_released: bool,
    pub reserve: Addr,
    pub slot_duration: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema)]
pub struct EventConfig {
    pub amount: Uint128,
    pub stage1_begin: u64, // timestamp when deposit and withdraw is allowed
    pub stage2_begin: u64, // timestamp when withdraw is allowed one time. The percentage of allowed withdrawal decreases from 100% to 0% over time.
    pub stage2_end: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema, Default)]
pub struct InfoResponse {
    pub deposit: Uint128,
    pub total_deposit: Uint128,
    pub withdrawable_amount: Uint128,
    pub tokens_to_claim: Uint128,
    pub clamable: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, JsonSchema, Default)]
pub struct DepositInfo {
    pub amount: Uint128,
    pub withdrew_stage2: bool,
    pub tokens_claimed: bool,
}
