use cosmwasm_std::{to_binary, Addr, CosmosMsg, Decimal, Env, StdResult, Uint128, WasmMsg};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub owner: Option<String>,
    pub astro_token_address: String,
    pub airdrop_contract_address: String,
    pub lockdrop_contract_address: String,
    pub lp_tokens_vesting_duration: u64,
    pub init_timestamp: u64,
    pub deposit_window: u64,
    pub withdrawal_window: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UpdateConfigMsg {
    pub owner: Option<String>,
    pub astro_ust_pair_address: Option<String>,
    pub generator_contract: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct PoolInfo {
    ///  ASTRO-UST LP Pool address
    pub astro_ust_pool_address: Addr,
    ///  ASTRO-UST LP Token address
    pub astro_ust_lp_token_address: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    UpdateConfig { new_config: UpdateConfigMsg },

    DepositUst {},
    WithdrawUst { amount: Uint128 },

    InitPool { slippage: Option<Decimal> },
    StakeLpTokens {},

    ClaimRewards { withdraw_lp_shares: Option<Uint128> },
    Callback(CallbackMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    DelegateAstroTokens { user_address: String },
    IncreaseAstroIncentives {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    UpdateStateOnRewardClaim {
        prev_astro_balance: Uint128,
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
    UserInfo { address: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    /// Account who can update config
    pub owner: Addr,
    ///  ASTRO token address
    pub astro_token_address: Addr,
    /// Airdrop Contract address
    pub airdrop_contract_address: Addr,
    /// Lockdrop Contract address
    pub lockdrop_contract_address: Addr,
    /// ASTRO-UST Pool info
    pub pool_info: Option<PoolInfo>,
    ///  Astroport Generator contract with which ASTRO-UST LP Tokens are staked
    pub generator_contract: Option<Addr>,
    /// Total ASTRO token rewards to be used to incentivize bootstrap auction participants
    pub astro_incentive_amount: Option<Uint128>,
    ///  Number of seconds over which LP Tokens are vested
    pub lp_tokens_vesting_duration: u64,
    /// Timestamp since which ASTRO / UST deposits will be allowed
    pub init_timestamp: u64,
    /// Number of seconds post init_timestamp during which deposits / withdrawals will be allowed
    pub deposit_window: u64,
    /// Number of seconds post deposit_window completion during which only withdrawals are allowed
    pub withdrawal_window: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct State {
    /// Total ASTRO tokens delegated to the contract by lockdrop participants / airdrop recipients
    pub total_astro_delegated: Uint128,
    /// Total UST delegated to the contract
    pub total_ust_delegated: Uint128,
    /// ASTRO--UST LP Shares currently staked with the Staking contract
    pub is_lp_staked: bool,
    /// Total LP shares minted post liquidity addition to the ASTRO-UST Pool
    pub lp_shares_minted: Option<Uint128>,
    /// Timestamp at which liquidity was added to the ASTRO-UST LP Pool
    pub pool_init_timestamp: u64,
    /// Ratio of ASTRO rewards accrued to weighted_amount. Used to calculate ASTRO incentives accrued by each user
    pub generator_astro_per_share: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct UserInfo {
    /// Total ASTRO Tokens delegated by the user
    pub astro_delegated: Uint128,
    /// Total UST delegated by the user
    pub ust_delegated: Uint128,
    /// Withdrawal counter to capture if the user already withdrew UST during the "only withdrawals" window
    pub ust_withdrawn: bool,
    /// User's LP share balance
    pub lp_shares: Option<Uint128>,
    /// LP shares withdrawn by the user
    pub claimed_lp_shares: Uint128,
    /// User's ASTRO rewards for participating in the auction
    pub auction_incentive_amount: Option<Uint128>,
    /// ASTRO tokens were transferred to user
    pub astro_incentive_transferred: bool,
    /// ASTRO staking incentives (LP token staking) withdrawn by the user
    pub generator_astro_debt: Uint128,
    /// Ratio of ASTRO rewards claimed to amount. Used to calculate ASTRO incentives claimable by each user
    pub user_gen_astro_per_share: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct UserInfoResponse {
    /// Total ASTRO Tokens delegated by the user
    pub astro_delegated: Uint128,
    /// Total UST delegated by the user
    pub ust_delegated: Uint128,
    /// Withdrawal counter to capture if the user already withdrew UST during the "only withdrawals" window
    pub ust_withdrawn: bool,
    /// User's LP share balance
    pub lp_shares: Option<Uint128>,
    /// LP shares withdrawn by the user
    pub claimed_lp_shares: Uint128,
    /// LP shares that are available to withdraw
    pub withdrawable_lp_shares: Option<Uint128>,
    /// User's ASTRO rewards for participating in the auction
    pub auction_incentive_amount: Option<Uint128>,
    /// ASTRO tokens were transferred to user
    pub astro_incentive_transferred: bool,
    /// Claimable ASTRO staking rewards
    pub claimable_generator_astro: Uint128,
    /// ASTRO staking incentives (LP token staking) withdrawn by the user
    pub generator_astro_debt: Uint128,
    /// Ratio of ASTRO rewards claimed to amount. Used to calculate ASTRO incentives claimable by each user
    pub user_gen_astro_per_share: Decimal,
}
