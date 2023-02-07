use cosmwasm_std::{to_binary, Addr, CosmosMsg, Decimal, Env, StdResult, Uint128, WasmMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct InstantiateMsg {
    pub owner: Option<String>,
    pub cntrn_token_contract: String,
    pub airdrop_contract_address: String,
    pub lockdrop_contract_address: String,
    pub lp_tokens_vesting_duration: u64,
    pub init_timestamp: u64,
    pub deposit_window: u64,
    pub withdrawal_window: u64,
    pub native_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct UpdateConfigMsg {
    pub owner: Option<String>,
    pub cntrn_native_pair_address: Option<String>,
    pub generator_contract: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PoolInfo {
    ///  cNTRN-NATIVE LP Pool address
    pub cntrn_native_pool_address: Addr,
    ///  cNTRN-NATIVE LP Token address
    pub cntrn_native_lp_token_address: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    // Receive_delete(Cw20ReceiveMsg),
    UpdateConfig {
        new_config: UpdateConfigMsg,
    },
    Deposit {
        cntrn_amount: Uint128,
    },
    Withdraw {
        amount_opposite: Uint128,
        amount_cntrn: Uint128,
    },
    InitPool {
        slippage: Option<Decimal>,
    },
    StakeLpTokens {},

    ClaimRewards {
        withdraw_lp_shares: Option<Uint128>,
    },
    Callback(CallbackMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    DelegateCNtrnTokens { user_address: String },
    IncreaseCNtrnIncentives {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    UpdateStateOnRewardClaim {
        prev_cntrn_balance: Uint128,
    },
    UpdateStateOnLiquidityAdditionToPool {
        prev_lp_balance: Uint128,
    },
    WithdrawUserRewardsCallback {
        user_address: Addr,
        withdraw_lp_shares: Option<Uint128>,
    },
}

// Modified from
// https://github.com/CosmWasm/cosmwasm-plus/blob/v0.2.3/packages/cw20/src/receiver.rs#L15
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
    /// Lockdrop Contract address
    pub lockdrop_contract_address: Addr,
    /// ASTRO-UST Pool info
    pub pool_info: Option<PoolInfo>,
    ///  Astroport Generator contract with which ASTRO-UST LP Tokens are staked
    pub generator_contract: Option<Addr>,
    /// Total ASTRO token rewards to be used to incentivize bootstrap auction participants
    pub cntrn_incentive_amount: Option<Uint128>,
    ///  Number of seconds over which LP Tokens are vested
    pub lp_tokens_vesting_duration: u64,
    /// Timestamp since which ASTRO / UST deposits will be allowed
    pub init_timestamp: u64,
    /// Number of seconds post init_timestamp during which deposits / withdrawals will be allowed
    pub deposit_window: u64,
    /// Number of seconds post deposit_window completion during which only withdrawals are allowed
    pub withdrawal_window: u64,
    /// Base denom contract cNTRN
    pub cntrn_token_address: Addr,
    /// Opposit denom
    pub native_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct State {
    /// Total cNTRN tokens delegated to the contract
    pub total_cntrn_deposited: Uint128,
    /// Total Opposite (native) deposited to the contract
    pub total_opposite_deposited: Uint128,
    /// cNTRN--NATIVE LP Shares currently staked with the Staking contract
    pub is_lp_staked: bool,
    /// Total LP shares minted post liquidity addition to the cNTRN-Native Pool
    pub lp_shares_minted: Option<Uint128>,
    /// Timestamp at which liquidity was added to the cNTRN-Native LP Pool
    pub pool_init_timestamp: u64,
    /// Ratio of cNTRN rewards accrued to weighted_amount. Used to calculate cNTRN incentives accrued by each user
    pub generator_cntrn_per_share: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct UserInfo {
    /// Total cw20 (cNTRN) Tokens delegated by the user
    pub cntrn_delegated: Uint128,
    /// Total Opposite (native) delegated by the user
    pub opposite_delegated: Uint128,
    /// Withdrawal counter to capture if the user already withdrew opposite (native) during the "only withdrawals" window
    pub opposite_withdrawn: bool,
    /// User's LP share balance
    pub lp_shares: Option<Uint128>,
    /// LP shares withdrawn by the user
    pub claimed_lp_shares: Uint128,
    /// User's cNTRN rewards for participating in the auction
    pub auction_incentive_amount: Option<Uint128>,
    /// cNTRN tokens were transferred to user
    pub cntrn_incentive_transferred: bool,
    /// cNTRN staking incentives (LP token staking) withdrawn by the user
    pub generator_cntrn_debt: Uint128,
    /// Ratio of cNTRN rewards claimed to amount. Used to calculate cNTRN incentives claimable by each user
    pub user_gen_cntrn_per_share: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct UserInfoResponse {
    /// Total cNTRN Tokens delegated by the user
    pub cntrn_delegated: Uint128,
    /// Total NATIVE delegated by the user
    pub native_delegated: Uint128,
    /// Withdrawal counter to capture if the user already withdrew UST during the "only withdrawals" window
    pub native_withdrawn: bool,
    /// User's LP share balance
    pub lp_shares: Option<Uint128>,
    /// LP shares withdrawn by the user
    pub claimed_lp_shares: Uint128,
    /// LP shares that are available to withdraw
    pub withdrawable_lp_shares: Option<Uint128>,
    /// User's cNTRN rewards for participating in the auction
    pub auction_incentive_amount: Option<Uint128>,
    /// cNTRN tokens were transferred to user
    pub cntrn_incentive_transferred: bool,
    /// Claimable cNTRN staking rewards
    pub claimable_generator_cntrn: Uint128,
    /// cNTRN staking incentives (LP token staking) withdrawn by the user
    pub generator_cntrn_debt: Uint128,
    /// Ratio of cNTRN rewards claimed to amount. Used to calculate ASTRO incentives claimable by each user
    pub user_gen_cntrn_per_share: Decimal,
}
