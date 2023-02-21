use cosmwasm_std::{to_binary, Addr, CosmosMsg, Env, StdResult, Uint128, WasmMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {
    pub owner: Option<String>,
    pub price_feed_contract: String,
    pub airdrop_contract_address: String,
    pub lockdrop_contract_address: String,
    pub reserve_contract_address: String,
    pub vesting_usdc_contract_address: String,
    pub vesting_atom_contract_address: String,
    pub lp_tokens_lock_window: u64,
    pub init_timestamp: u64,
    pub deposit_window: u64,
    pub withdrawal_window: u64,
    pub stable_denom: String,
    pub volatile_denom: String,
    pub base_denom: String,
    pub max_lock_period: u16,
    pub min_lock_period: u16,
    pub min_exchange_rate: u64,
    pub min_ntrn_amount: Uint128,
    pub vesting_migration_pack_size: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct UpdateConfigMsg {
    pub owner: Option<String>,
    pub price_feed_contract: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PoolInfo {
    ///  NTRN-USDC LP Pool address
    pub ntrn_usdc_pool_address: Addr,
    ///  NTRN-ATOM LP Pool address
    pub ntrn_atom_pool_address: Addr,
    ///  NTRN-USDC LP Token address
    pub ntrn_usdc_lp_token_address: Addr,
    ///  NTRN-ATOM LP Token address
    pub ntrn_atom_lp_token_address: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    UpdateConfig {
        new_config: UpdateConfigMsg,
    },
    Deposit {},
    Withdraw {
        amount_stable: Uint128,
        amount_volatile: Uint128,
    },
    InitPool {},
    SetPoolSize {},
    LockLp {
        asset: String,
        amount: Uint128,
        period: u16,
    },
    MigrateToVesting {},
    Callback(CallbackMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    FinalizePoolInitialization { prev_lp_balance: PoolBalance },
}

// // Modified from
// // https://github.com/CosmWasm/cosmwasm-plus/blob/v0.2.3/packages/cw20/src/receiver.rs#L15
impl CallbackMsg {
    pub fn to_cosmos_msg(&self, env: &Env) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::Callback(self.clone()))?,
            funds: vec![],
        }))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
    UserInfo { address: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    /// Account who can update config
    pub owner: Addr,
    /// Airdrop Contract address
    pub airdrop_contract_address: Addr,
    /// Reserve Contract address
    pub reserve_contract_address: Addr,
    /// Vesting LP-USDC Contract address
    pub vesting_usdc_contract_address: Addr,
    /// Vesting LP-ATOM Contract address
    pub vesting_atom_contract_address: Addr,
    /// Lockdrop Contract address
    pub lockdrop_contract_address: Addr,
    /// Price feed contract address
    pub price_feed_contract: Addr,
    /// Pool info
    pub pool_info: Option<PoolInfo>,
    /// Timestamp since which USDC / ATOM deposits will be allowed
    pub init_timestamp: u64,
    /// Number of seconds post init_timestamp during which deposits / withdrawals will be allowed
    pub deposit_window: u64,
    /// Number of seconds post deposit_window completion during which only withdrawals are allowed
    pub withdrawal_window: u64,
    /// Lock window for LP tokens
    pub lp_tokens_lock_window: u64,
    /// Base denom
    pub ntrn_denom: String,
    /// Stable denom
    pub usdc_denom: String,
    /// Volatile denom
    pub atom_denom: String,
    /// Min NTRN amount to be distributed as pool liquidity
    pub min_ntrn_amount: Uint128,
    /// min exchange freshness rate (seconds)
    pub min_exchange_rate_age: u64,
    /// vesting migration users pack size
    pub vesting_migration_pack_size: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct State {
    /// Total USDC deposited to the contract
    pub total_usdc_deposited: Uint128,
    /// Total ATOM deposited to the contract
    pub total_atom_deposited: Uint128,
    pub is_rest_lp_vested: bool,
    /// Total LP shares minted post liquidity addition to the cNTRN-Native Pool
    pub lp_usdc_shares_minted: Option<Uint128>,
    pub lp_atom_shares_minted: Option<Uint128>,
    /// Timestamp at which liquidity was added to the NTRN-Stable and NTRN-Volatile LP Pool
    pub pool_init_timestamp: u64,
    /// USDC NTRN amount
    pub usdc_ntrn_size: Uint128,
    /// ATOM NTRN amount
    pub atom_ntrn_size: Uint128,
    /// LP count for USDC amount
    pub usdc_lp_size: Uint128,
    /// LP count for ATOM amount
    pub atom_lp_size: Uint128,
    /// locked USDC LP shares
    pub usdc_lp_locked: Uint128,
    /// locked ATOM LP shares
    pub atom_lp_locked: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct UserInfo {
    pub address: String,
    /// Total Stable delegated by the user
    pub usdc_deposited: Uint128,
    /// Total Volatile delegated by the user
    pub atom_deposited: Uint128,
    /// Withdrawal counter to capture if the user already withdrew tokens during the "only withdrawals" window
    pub withdrawn: bool,
    /// LP shares locked for the user
    pub usdc_lp_locked: Uint128,
    /// LP shares locked for the user
    pub atom_lp_locked: Uint128,
    /// Vested?
    pub is_vested: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct UserInfoResponse {
    /// Total stable delegated by the user
    pub usdc_deposited: Uint128,
    /// Total stable delegated by the user
    pub atom_deposited: Uint128,
    /// Withdrawal counter to capture if the user already withdrew UST during the "only withdrawals" window
    pub withdrawn: bool,
    pub atom_lp_amount: Uint128,
    pub usdc_lp_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PriceFeedQuery {
    GetPrice { symbols: Vec<String> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PriceFeedResponse {
    pub prices: Vec<u64>,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LockDropExecute {
    IncreaseLockupFor {
        asset: String,
        amount: Uint128,
        period: u16,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct UserLpInfo {
    pub atom_lp_amount: Uint128,
    pub usdc_lp_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PoolBalance {
    pub atom: Uint128,
    pub usdc: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct VestingMigrationUser {
    pub address: String,
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VestingExecuteMsg {
    MigrateVestingUsers { users: Vec<VestingMigrationUser> },
}
