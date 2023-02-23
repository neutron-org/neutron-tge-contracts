use astroport::asset::{Asset, AssetInfo};
use astroport::restricted_vector::RestrictedVector;
use cosmwasm_std::{
    from_slice, to_binary, Addr, CosmosMsg, Decimal, Env, StdResult, Uint128, Uint256, WasmMsg,
};
use cw20::Cw20ReceiveMsg;
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// TODO: implement display trait
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum PoolType {
    USDC,
    ATOM,
}

impl Into<String> for PoolType {
    fn into(self) -> String {
        match self {
            PoolType::USDC => "usdc".to_string(),
            PoolType::ATOM => "atom".to_string(),
        }
    }
}


impl PoolType {
    fn bytes(&self) -> &[u8] {
        match self {
            PoolType::USDC => "usdc".as_bytes(),
            PoolType::ATOM => "atom".as_bytes(),
        }
    }
}

impl KeyDeserialize for PoolType {
    type Output = PoolType;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        from_slice(&value)
    }
}

impl<'a> PrimaryKey<'a> for PoolType {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(self.bytes())]
    }
}

impl<'a> Prefixer<'a> for PoolType {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Ref(self.bytes())]
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Account which can update config
    pub owner: Option<String>,
    /// Address of ATOM/NTRN token
    pub atom_token: String,
    /// Address of USDC/NTRN token
    pub usdc_token: String,
    /// Credit cintract address
    pub credit_contract: String,
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
    pub monthly_multiplier: u64,
    /// Lockdrop Reward divider
    pub monthly_divider: u64,
    /// Max lockup positions a user can have
    pub max_positions_per_user: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UpdateConfigMsg {
    /// Bootstrap Auction contract address
    pub auction_contract_address: Option<String>,
    /// Generator (Staking for dual rewards) contract address
    pub generator_address: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    IncreaseLockupFor {
        user_address: String,
        pool_type: PoolType,
        amount: Uint128,
        duration: u64,
    },
    // Receive hook used to accept LP Token deposits
    Receive(Cw20ReceiveMsg),
    IncreaseNTRNIncentives {},
    // ADMIN Function ::: To update configuration
    UpdateConfig {
        new_config: UpdateConfigMsg,
    },
    // Function to facilitate LP Token withdrawals from lockups
    WithdrawFromLockup {
        pool_type: PoolType,
        duration: u64,
        amount: Uint128,
    },

    // ADMIN Function ::: To Migrate liquidity from terraswap to astroport
    // MigrateLiquidity {
    //     terraswap_lp_token: String,
    //     astroport_pool_addr: String,
    //     slippage_tolerance: Option<Decimal>,
    // },
    // ADMIN Function ::: To stake LP Tokens with the generator contract
    // StakeLpTokens {
    //     pool_type: PoolType,
    // },
    // Facilitates ASTRO reward withdrawal which have not been delegated to bootstrap auction along with optional Unlock (can be forceful)
    // If withdraw_lp_stake is true and force_unlock is false, it Unlocks the lockup position if its lockup duration has concluded
    // If both withdraw_lp_stake and force_unlock are true, it forcefully unlocks the positon. user needs to approve ASTRO Token to
    // be transferred by the lockdrop contract to itself for forceful unlock
    ClaimRewardsAndOptionallyUnlock {
        pool_type: PoolType,
        duration: u64,
        withdraw_lp_stake: bool,
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
    // Called by the bootstrap auction contract when liquidity is added to the
    // Pool to enable ASTRO withdrawals by users
    // EnableClaims {},
    // ADMIN Function ::: Add new Pool (Only Terraswap Pools)
    InitializePool {
        pool_type: PoolType,
        incentives_share: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    UpdatePoolOnDualRewardsClaim {
        pool_type: PoolType,
        prev_ntrn_balance: Uint128,
        prev_proxy_reward_balances: Vec<Asset>,
    },
    WithdrawUserLockupRewardsCallback {
        pool_type: PoolType,
        user_address: Addr,
        duration: u64,
        withdraw_lp_stake: bool,
    },
    // WithdrawLiquidityFromTerraswapCallback {
    //     terraswap_lp_token: Addr,
    //     astroport_pool: Addr,
    //     prev_assets: [terraswap::asset::Asset; 2],
    //     slippage_tolerance: Option<Decimal>,
    // },
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
        pool_type: PoolType,
    },
    UserInfo {
        address: String,
    },
    UserInfoWithLockupsList {
        address: String,
    },
    LockUpInfo {
        user_address: String,
        pool_type: PoolType,
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
    /// Credit contract address
    pub credit_contract: Addr,
    /// Bootstrap Auction contract address
    pub auction_contract: Option<Addr>,
    /// Generator (Staking for dual rewards) contract address
    pub generator: Option<Addr>,
    /// Timestamp when Contract will start accepting LP Token deposits
    pub init_timestamp: u64,
    /// Number of seconds during which lockup positions be accepted
    pub lock_window: u64,
    /// Number of seconds during which lockup deposits will be accepted
    pub deposit_window: u64,
    /// Withdrawal Window Length :: Post the deposit window
    pub withdrawal_window: u64,
    /// Min. no. of weeks allowed for lockup
    pub min_lock_duration: u64,
    /// Max. no. of weeks allowed for lockup
    pub max_lock_duration: u64,
    /// Lockdrop Reward multiplier
    pub montly_multiplier: u64,
    /// Lockdrop Reward divider
    pub monthly_divider: u64,
    /// Total NTRN lockdrop incentives to be distributed among the users
    pub lockdrop_incentives: Uint128,
    /// Max lockup positions a user can have
    pub max_positions_per_user: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct State {
    /// Total ASTRO incentives share
    pub total_incentives_share: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    pub pool: Addr,
    pub amount_in_lockups: Uint128,
    // pub migration_info: Option<MigrationInfo>,
    /// Share of total ASTRO incentives allocated to this pool
    pub incentives_share: u64,
    /// Weighted LP Token balance used to calculate ASTRO rewards a particular user can claim
    pub weighted_amount: Uint256,
    /// Ratio of Generator ASTRO rewards accured to astroport pool share
    pub generator_ntrn_per_share: Decimal,
    /// Ratio of Generator Proxy rewards accured to astroport pool share
    pub generator_proxy_per_share: RestrictedVector<AssetInfo, Decimal>,
    /// Boolean value indicating if the LP Tokens are staked with the Generator contract or not
    pub is_staked: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct UserInfo {
    /// Total ASTRO tokens user received as rewards for participation in the lockdrop
    pub total_ntrn_rewards: Uint128,
    /// ASTRO tokens transferred to user
    pub ntrn_transferred: bool,
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
    pub ntrn_rewards: Uint128,
    /// Generator ASTRO tokens loockup received as generator rewards
    pub generator_ntrn_debt: Uint128,
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
    pub ntrn_rewards: Uint128,
    /// Generator ASTRO tokens loockup received as generator rewards
    pub generator_ntrn_debt: Uint128,
    /// Generator Proxy tokens lockup received as generator rewards
    pub generator_proxy_debt: RestrictedVector<AssetInfo, Uint128>,
    /// Timestamp beyond which this position can be unlocked
    pub unlock_timestamp: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    /// Total ASTRO incentives share
    pub total_incentives_share: u64,
    /// Vector containing LP addresses for all the supported LP Pools
    pub supported_pairs_list: Vec<PoolType>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserInfoResponse {
    /// Total ASTRO tokens user received as rewards for participation in the lockdrop
    pub total_astro_rewards: Uint128,
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
    /// ASTRO tokens transferred to user
    pub astro_transferred: bool,
    /// Lockup positions
    pub lockup_infos: Vec<LockUpInfoSummary>,
    /// Number of lockup positions the user is having
    pub lockup_positions_index: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct LockUpInfoSummary {
    pub pool_type: PoolType,
    pub duration: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct LockUpInfoResponse {
    /// Terraswap LP token
    pub pool_type: PoolType,
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
    pub astroport_lp_token: Addr,
    pub astroport_lp_transferred: Option<Uint128>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PendingAssetRewardResponse {
    pub amount: Uint128,
}
