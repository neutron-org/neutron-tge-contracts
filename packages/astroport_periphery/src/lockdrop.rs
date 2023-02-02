use astroport::asset::{Asset, AssetInfo};
use astroport::restricted_vector::RestrictedVector;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Env, StdResult, Uint128, Uint256, WasmMsg,
};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Account which can update config
    pub owner: Option<String>,
    /// Timestamp when Contract will start accepting LP Token deposits
    pub init_timestamp: u64,
    /// Number of seconds during which lockup deposits will be accepted
    pub deposit_window: u64,
    /// Withdrawal Window Length :: Post the deposit window
    pub withdrawal_window: u64,
    /// Min. no. of weeks allowed for lockup
    pub min_lock_duration: u64,
    /// Max. no. of weeks allowed for lockup
    pub max_lock_duration: u64,
    /// Lockdrop Reward multiplier
    pub weekly_multiplier: u64,
    /// Lockdrop Reward divider
    pub weekly_divider: u64,
    /// Max lockup positions a user can have
    pub max_positions_per_user: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UpdateConfigMsg {
    /// Astroport token address
    pub astro_token_address: Option<String>,
    /// Bootstrap Auction contract address
    pub auction_contract_address: Option<String>,
    /// Generator (Staking for dual rewards) contract address
    pub generator_address: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    // Receive hook used to accept LP Token deposits
    Receive(Cw20ReceiveMsg),
    // ADMIN Function ::: To update configuration
    UpdateConfig {
        new_config: UpdateConfigMsg,
    },
    // Called by the bootstrap auction contract when liquidity is added to the
    // Pool to enable ASTRO withdrawals by users
    EnableClaims {},
    // ADMIN Function ::: Add new Pool (Only Terraswap Pools)
    InitializePool {
        terraswap_lp_token: String,
        incentives_share: u64,
    },
    // ADMIN Function ::: To set incentives_share for the Pool
    UpdatePool {
        terraswap_lp_token: String,
        incentives_share: u64,
    },
    // Function to facilitate LP Token withdrawals from lockups
    WithdrawFromLockup {
        terraswap_lp_token: String,
        duration: u64,
        amount: Uint128,
    },
    // ADMIN Function ::: To Migrate liquidity from terraswap to astroport
    MigrateLiquidity {
        terraswap_lp_token: String,
        astroport_pool_addr: String,
        slippage_tolerance: Option<Decimal>,
    },
    // ADMIN Function ::: To stake LP Tokens with the generator contract
    StakeLpTokens {
        terraswap_lp_token: String,
    },
    // Delegate ASTRO to Bootstrap via auction contract
    DelegateAstroToAuction {
        amount: Uint128,
    },
    // Facilitates ASTRO reward withdrawal which have not been delegated to bootstrap auction along with optional Unlock (can be forceful)
    // If withdraw_lp_stake is true and force_unlock is false, it Unlocks the lockup position if its lockup duration has concluded
    // If both withdraw_lp_stake and force_unlock are true, it forcefully unlocks the positon. user needs to approve ASTRO Token to
    // be transferred by the lockdrop contract to itself for forceful unlock
    ClaimRewardsAndOptionallyUnlock {
        terraswap_lp_token: String,
        duration: u64,
        withdraw_lp_stake: bool,
    },
    ClaimAssetReward {
        recipient: Option<String>,
        terraswap_lp_token: String,
        duration: u64,
    },
    // ADMIN Function ::: Toggle poll rewards
    TogglePoolRewards {
        terraswap_lp_token: String,
        enable: bool,
    },
    /// Callbacks; only callable by the contract itself.
    Callback(CallbackMsg),
    /// ProposeNewOwner creates a proposal to change contract ownership.
    /// The validity period for the proposal is set in the `expires_in` variable.
    ProposeNewOwner {
        /// Newly proposed contract owner
        owner: String,
        /// The date after which this proposal expires
        expires_in: u64,
    },
    /// DropOwnershipProposal removes the existing offer to change contract ownership.
    DropOwnershipProposal {},
    /// Used to claim contract ownership.
    ClaimOwnership {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Open a new user position or add to an existing position (Cw20ReceiveMsg)
    IncreaseLockup {
        duration: u64,
    },
    IncreaseAstroIncentives {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    UpdatePoolOnDualRewardsClaim {
        terraswap_lp_token: Addr,
        prev_astro_balance: Uint128,
        prev_proxy_reward_balances: Vec<Asset>,
    },
    WithdrawUserLockupRewardsCallback {
        terraswap_lp_token: Addr,
        user_address: Addr,
        duration: u64,
        withdraw_lp_stake: bool,
    },
    WithdrawLiquidityFromTerraswapCallback {
        terraswap_lp_token: Addr,
        astroport_pool: Addr,
        prev_assets: [terraswap::asset::Asset; 2],
        slippage_tolerance: Option<Decimal>,
    },
    DistributeAssetReward {
        previous_balance: Uint128,
        terraswap_lp_token: Addr,
        user_address: Addr,
        recipient: Addr,
        lock_duration: u64,
    },
}

// Modified from
// https://github.com/CosmWasm/cosmwasm-plus/blob/v0.2.3/packages/cw20/src/receiver.rs#L15
impl CallbackMsg {
    pub fn to_cosmos_msg(self, env: &Env) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::Callback(self))?,
            funds: vec![],
        }))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
    Pool {
        terraswap_lp_token: String,
    },
    UserInfo {
        address: String,
    },
    UserInfoWithLockupsList {
        address: String,
    },
    LockUpInfo {
        user_address: String,
        terraswap_lp_token: String,
        duration: u64,
    },
    PendingAssetReward {
        user_address: String,
        terraswap_lp_token: String,
        duration: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrationInfo {
    pub terraswap_migrated_amount: Uint128,
    pub astroport_lp_token: Addr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Account which can update the config
    pub owner: Addr,
    /// ASTRO Token address
    pub astro_token: Option<Addr>,
    /// Bootstrap Auction contract address
    pub auction_contract: Option<Addr>,
    /// Generator (Staking for dual rewards) contract address
    pub generator: Option<Addr>,
    /// Timestamp when Contract will start accepting LP Token deposits
    pub init_timestamp: u64,
    /// Number of seconds during which lockup deposits will be accepted
    pub deposit_window: u64,
    /// Withdrawal Window Length :: Post the deposit window
    pub withdrawal_window: u64,
    /// Min. no. of weeks allowed for lockup
    pub min_lock_duration: u64,
    /// Max. no. of weeks allowed for lockup
    pub max_lock_duration: u64,
    /// Lockdrop Reward multiplier
    pub weekly_multiplier: u64,
    /// Lockdrop Reward divider
    pub weekly_divider: u64,
    /// Total ASTRO lockdrop incentives to be distributed among the users
    pub lockdrop_incentives: Uint128,
    /// Max lockup positions a user can have
    pub max_positions_per_user: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct State {
    /// Total ASTRO incentives share
    pub total_incentives_share: u64,
    /// ASTRO Tokens delegated to the bootstrap auction contract
    pub total_astro_delegated: Uint128,
    /// Boolean value indicating if the user can withdraw their ASTRO rewards or not
    pub are_claims_allowed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    pub terraswap_pool: Addr,
    pub terraswap_amount_in_lockups: Uint128,
    pub migration_info: Option<MigrationInfo>,
    /// Share of total ASTRO incentives allocated to this pool
    pub incentives_share: u64,
    /// Weighted LP Token balance used to calculate ASTRO rewards a particular user can claim
    pub weighted_amount: Uint256,
    /// Ratio of Generator ASTRO rewards accured to astroport pool share
    pub generator_astro_per_share: Decimal,
    /// Ratio of Generator Proxy rewards accured to astroport pool share
    pub generator_proxy_per_share: RestrictedVector<AssetInfo, Decimal>,
    /// Boolean value indicating if the LP Tokens are staked with the Generator contract or not
    pub is_staked: bool,
    /// Flag defines whether the asset has rewards or not
    pub has_asset_rewards: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct UserInfo {
    /// Total ASTRO tokens user received as rewards for participation in the lockdrop
    pub total_astro_rewards: Uint128,
    /// Total ASTRO tokens user delegated to the LP bootstrap auction pool
    pub delegated_astro_rewards: Uint128,
    /// ASTRO tokens transferred to user
    pub astro_transferred: bool,
    /// Number of lockup positions the user is having
    pub lockup_positions_index: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct LockupInfoV1 {
    /// Terraswap LP units locked by the user
    pub lp_units_locked: Uint128,
    pub astroport_lp_transferred: Option<Uint128>,
    /// Boolean value indicating if the user's has withdrawn funds post the only 1 withdrawal limit cutoff
    pub withdrawal_flag: bool,
    /// ASTRO tokens received as rewards for participation in the lockdrop
    pub astro_rewards: Uint128,
    /// Generator ASTRO tokens loockup received as generator rewards
    pub generator_astro_debt: Uint128,
    /// Generator Proxy tokens lockup received as generator rewards
    pub generator_proxy_debt: Uint128,
    /// Timestamp beyond which this position can be unlocked
    pub unlock_timestamp: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct LockupInfoV2 {
    /// Terraswap LP units locked by the user
    pub lp_units_locked: Uint128,
    pub astroport_lp_transferred: Option<Uint128>,
    /// Boolean value indicating if the user's has withdrawn funds post the only 1 withdrawal limit cutoff
    pub withdrawal_flag: bool,
    /// ASTRO tokens received as rewards for participation in the lockdrop
    pub astro_rewards: Uint128,
    /// Generator ASTRO tokens loockup received as generator rewards
    pub generator_astro_debt: Uint128,
    /// Generator Proxy tokens lockup received as generator rewards
    pub generator_proxy_debt: RestrictedVector<AssetInfo, Uint128>,
    /// Timestamp beyond which this position can be unlocked
    pub unlock_timestamp: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    /// Total ASTRO incentives share
    pub total_incentives_share: u64,
    /// ASTRO Tokens delegated to the bootstrap auction contract
    pub total_astro_delegated: Uint128,
    /// Boolean value indicating if the user can withdraw thier ASTRO rewards or not
    pub are_claims_allowed: bool,
    /// Vector containing LP addresses for all the supported LP Pools
    pub supported_pairs_list: Vec<Addr>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserInfoResponse {
    /// Total ASTRO tokens user received as rewards for participation in the lockdrop
    pub total_astro_rewards: Uint128,
    /// Total ASTRO tokens user delegated to the LP bootstrap auction pool
    pub delegated_astro_rewards: Uint128,
    /// ASTRO tokens transferred to user
    pub astro_transferred: bool,
    /// Lockup positions
    pub lockup_infos: Vec<LockUpInfoResponse>,
    /// ASTRO tokens receivable as generator rewards that user can claim
    pub claimable_generator_astro_debt: Uint128,
    /// Number of lockup positions the user is having
    pub lockup_positions_index: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserInfoWithListResponse {
    /// Total ASTRO tokens user received as rewards for participation in the lockdrop
    pub total_astro_rewards: Uint128,
    /// Total ASTRO tokens user delegated to the LP bootstrap auction pool
    pub delegated_astro_rewards: Uint128,
    /// ASTRO tokens transferred to user
    pub astro_transferred: bool,
    /// Lockup positions
    pub lockup_infos: Vec<LockUpInfoSummary>,
    /// Number of lockup positions the user is having
    pub lockup_positions_index: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct LockUpInfoSummary {
    pub pool_address: String,
    pub duration: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct LockUpInfoResponse {
    /// Terraswap LP token
    pub terraswap_lp_token: Addr,
    /// Terraswap LP units locked by the user
    pub lp_units_locked: Uint128,
    /// Boolean value indicating if the user's has withdrawn funds post the only 1 withdrawal limit cutoff
    pub withdrawal_flag: bool,
    /// ASTRO tokens received as rewards for participation in the lockdrop
    pub astro_rewards: Uint128,
    pub duration: u64,
    /// Generator ASTRO tokens lockup received as generator rewards
    pub generator_astro_debt: Uint128,
    /// ASTRO tokens receivable as generator rewards that user can claim
    pub claimable_generator_astro_debt: Uint128,
    /// Generator Proxy tokens lockup received as generator rewards
    pub generator_proxy_debt: RestrictedVector<AssetInfo, Uint128>,
    /// Proxy tokens receivable as generator rewards that user can claim
    pub claimable_generator_proxy_debt: RestrictedVector<AssetInfo, Uint128>,
    /// Timestamp beyond which this position can be unlocked
    pub unlock_timestamp: u64,
    /// User's Astroport LP units, calculated as lp_units_locked (terraswap) / total LP units locked (terraswap) * Astroport LP units minted post migration
    pub astroport_lp_units: Option<Uint128>,
    pub astroport_lp_token: Option<Addr>,
    pub astroport_lp_transferred: Option<Uint128>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PendingAssetRewardResponse {
    pub amount: Uint128,
}
