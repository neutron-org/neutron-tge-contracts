use crate::lockdrop::{
    LockupInfoV2 as LockdropXYKLockupInfoV2, PoolType as LockdropXYKPoolType,
    UserInfo as LockdropXYKUserInfo,
};
use astroport::asset::{Asset, AssetInfo};
use astroport::restricted_vector::RestrictedVector;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    to_json_binary, Addr, CosmosMsg, Decimal, Decimal256, Env, StdError, StdResult, Uint128,
    Uint256, WasmMsg,
};
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// TODO: implement display trait
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, Copy)]
pub enum PoolType {
    USDC,
    ATOM,
}

impl From<LockdropXYKPoolType> for PoolType {
    fn from(t: LockdropXYKPoolType) -> PoolType {
        match t {
            LockdropXYKPoolType::USDC => PoolType::USDC,
            LockdropXYKPoolType::ATOM => PoolType::ATOM,
        }
    }
}

#[allow(clippy::from_over_into)]
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

    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        match value.as_slice() {
            b"usdc" => Ok(PoolType::USDC),
            b"atom" => Ok(PoolType::ATOM),
            _ => Err(StdError::generic_err("Invalid PoolType")),
        }
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

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Account which can update config
    pub owner: Option<String>,
    /// Original XYK lockdrop contract address
    pub xyk_lockdrop_contract: String,
    /// Credits contract address
    pub credits_contract: String,
    /// Auction contract address
    pub auction_contract: String,
    /// Generator (Staking for dual rewards) contract address
    pub generator: String,
    /// Describes rewards coefficients for each lockup duration
    pub lockup_rewards_info: Vec<LockupRewardsInfo>,
    /// Address of the LP token of the NTRN/USDC PCL pool
    pub usdc_token: String,
    /// Address of the LP token of the NTRN/ATOM PCL pool
    pub atom_token: String,

    /*
    Since no NTRN rewards are distributed from the PCL lockdrop contract, the fields below are
    just used to fill the contract's state with the same values as it is in the XYK lockdrop
    contract. So the values should be just copied from the XYK lockdrop contract's state.
    */
    /// Total NTRN lockdrop incentives distributed among the users.
    pub lockdrop_incentives: Uint128,
    /// Share of total NTRN incentives allocated to the NTRN/USDC PCL pool
    pub usdc_incentives_share: Uint128,
    /// Weighted LP Token balance used to calculate NTRN rewards a particular NTRN/USDC pool
    /// depositor can claim
    pub usdc_weighted_amount: Uint256,
    /// Share of total NTRN incentives allocated to the NTRN/ATOM PCL pool
    pub atom_incentives_share: Uint128,
    /// Weighted LP Token balance used to calculate NTRN rewards a particular NTRN/ATOM pool
    /// depositor can claim
    pub atom_weighted_amount: Uint256,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct UpdateConfigMsg {
    /// Generator (Staking for dual rewards) contract address
    pub generator_address: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    // ADMIN Function ::: To update configuration
    UpdateConfig {
        new_config: UpdateConfigMsg,
    },
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
    /// A handler to receive lockdrop liquidity migrated from xyl pools to PCL ones. Only callable
    /// by the original lockdrop contract. Expects two **Coin**s to be attached as funds.
    #[serde(rename = "migrate_xyk_liquidity")]
    MigrateXYKLiquidity {
        /// The type of the pool the lockup is related to.
        pool_type: LockdropXYKPoolType,
        /// The address of the user which owns the lockup.
        user_address_raw: String,
        /// The duration of the lock period.
        duration: u64,
        /// The lockup owner's info from the XYK lockdrop contract. Is used to create a UserInfo
        /// entry on the PCL lockdrop contract's side.
        user_info: LockdropXYKUserInfo,
        /// The lockup info from the XYK lockdrop contract. Is used to create a LockupInfoV2 entry
        /// on the PCL lockdrop contract's side.
        lockup_info: LockdropXYKLockupInfoV2,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    UpdatePoolOnDualRewardsClaim {
        pool_type: PoolType,
        prev_proxy_reward_balances: Vec<Asset>,
    },
    WithdrawUserLockupRewardsCallback {
        pool_type: PoolType,
        user_address: Addr,
        duration: u64,
        withdraw_lp_stake: bool,
    },
    /// Completes the liquidity migration process by making all necessary state updates for the lockup
    /// position.
    FinishLockupMigrationCallback {
        /// The type of the pool the lockup is related to.
        pool_type: PoolType,
        /// The address of the user which owns the lockup.
        user_address: Addr,
        /// The duration of the lock period.
        duration: u64,
        /// The address of the LP token of the pool.
        lp_token: String,
        /// The amount of staked LP token the PCL lockdrop contract possesses of before liquidity
        /// provision and staking to the generator. Used to calculate LP token amount received for
        /// liquidity provision.
        staked_lp_token_amount: Uint128,
        /// The lockup owner's info from the XYK lockdrop contract. Is used to create a UserInfo
        /// entry on the PCL lockdrop contract's side.
        user_info: LockdropXYKUserInfo,
        /// The lockup info from the XYK lockdrop contract. Is used to create a LockupInfoV2 entry
        /// on the PCL lockdrop contract's side.
        lockup_info: LockdropXYKLockupInfoV2,
    },
}

// Modified from
// https://github.com/CosmWasm/cosmwasm-plus/blob/v0.2.3/packages/cw20/src/receiver.rs#L15
impl CallbackMsg {
    pub fn to_cosmos_msg(self, env: &Env) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_json_binary(&ExecuteMsg::Callback(self))?,
            funds: vec![],
        }))
    }
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Config)]
    Config {},
    #[returns(StateResponse)]
    State {},
    #[returns(PoolInfo)]
    Pool { pool_type: PoolType },
    #[returns(UserInfoResponse)]
    UserInfo { address: String },
    #[returns(UserInfoWithListResponse)]
    UserInfoWithLockupsList { address: String },
    #[returns(LockUpInfoResponse)]
    LockUpInfo {
        user_address: String,
        pool_type: PoolType,
        duration: u64,
    },
    #[returns(Option<Uint128>)]
    QueryUserLockupTotalAtHeight {
        pool_type: PoolType,
        user_address: String,
        height: u64,
    },
    #[returns(Option<Uint128>)]
    QueryLockupTotalAtHeight { pool_type: PoolType, height: u64 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct LockupRewardsInfo {
    pub duration: u64,
    pub coefficient: Decimal256,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Config {
    /// Account which can update the config
    pub owner: Addr,
    /// Original XYK lockdrop contract address
    pub xyk_lockdrop_contract: Addr,
    /// Credits contract address
    pub credits_contract: Addr,
    /// Bootstrap Auction contract address
    pub auction_contract: Addr,
    /// Incentives contract address
    pub incentives: Addr,
    /// Total NTRN lockdrop incentives to be distributed among the users
    pub lockdrop_incentives: Uint128,
    /// Describes rewards coefficients for each lockup duration
    pub lockup_rewards_info: Vec<LockupRewardsInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, Default)]
pub struct State {
    /// Total NTRN incentives share
    pub total_incentives_share: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    pub lp_token: Addr,
    pub amount_in_lockups: Uint128,
    /// Share of total NTRN incentives allocated to this pool
    pub incentives_share: Uint128,
    /// Weighted LP Token balance used to calculate NTRN rewards a particular user can claim
    pub weighted_amount: Uint256,
    /// Ratio of Generator NTRN rewards accured to astroport pool share
    pub generator_ntrn_per_share: Decimal,
    /// Ratio of Generator Proxy rewards accured to astroport pool share
    pub generator_proxy_per_share: RestrictedVector<AssetInfo, Decimal>,
    /// Boolean value indicating if the LP Tokens are staked with the Generator contract or not
    pub is_staked: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, Default)]
pub struct UserInfo {
    /// Total NTRN tokens user received as rewards for participation in the lockdrop
    pub total_ntrn_rewards: Uint128,
    /// NTRN tokens transferred to user
    pub ntrn_transferred: bool,
    /// Number of lockup positions the user is having
    pub lockup_positions_index: u32,
}

impl From<LockdropXYKUserInfo> for UserInfo {
    fn from(i: LockdropXYKUserInfo) -> UserInfo {
        UserInfo {
            total_ntrn_rewards: i.total_ntrn_rewards,
            ntrn_transferred: i.ntrn_transferred,
            lockup_positions_index: i.lockup_positions_index,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct LockupInfoV1 {
    /// Terraswap LP units locked by the user
    pub lp_units_locked: Uint128,
    pub astroport_lp_transferred: Option<Uint128>,
    /// Boolean value indicating if the user's has withdrawn funds post the only 1 withdrawal limit cutoff
    pub withdrawal_flag: bool,
    /// NTRN tokens received as rewards for participation in the lockdrop
    pub ntrn_rewards: Uint128,
    /// Generator NTRN tokens loockup received as generator rewards
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
    /// NTRN tokens received as rewards for participation in the lockdrop
    pub ntrn_rewards: Uint128,
    /// Generator NTRN tokens loockup received as generator rewards
    pub generator_ntrn_debt: Uint128,
    /// Generator Proxy tokens lockup received as generator rewards
    pub generator_proxy_debt: RestrictedVector<AssetInfo, Uint128>,
    /// Timestamp beyond which this position can be unlocked
    pub unlock_timestamp: u64,
}

impl LockupInfoV2 {
    /// Creates a lockup entry for PCL lockdrop contract based on a lockup entry for XYK lockdrop
    /// contract. The **lp_units_locked** field is the amount of lp tokens minted by the PCL pool.
    pub fn from_xyk_lockup_info(
        i: LockdropXYKLockupInfoV2,
        lp_units_locked: Uint128,
    ) -> LockupInfoV2 {
        LockupInfoV2 {
            lp_units_locked,
            astroport_lp_transferred: None,
            withdrawal_flag: i.withdrawal_flag,
            ntrn_rewards: i.ntrn_rewards,
            generator_ntrn_debt: Uint128::zero(),
            generator_proxy_debt: Default::default(),
            unlock_timestamp: i.unlock_timestamp,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct StateResponse {
    /// Total NTRN incentives share
    pub total_incentives_share: Uint128,
    /// Vector containing LP addresses for all the supported LP Pools
    pub supported_pairs_list: Vec<PoolType>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserInfoResponse {
    /// Total NTRN tokens user received as rewards for participation in the lockdrop
    pub total_ntrn_rewards: Uint128,
    /// NTRN tokens transferred to user
    pub ntrn_transferred: bool,
    /// Lockup positions
    pub lockup_infos: Vec<LockUpInfoResponse>,
    /// NTRN tokens receivable as generator rewards that user can claim
    pub claimable_generator_ntrn_debt: Uint128,
    /// Number of lockup positions the user is having
    pub lockup_positions_index: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct UserInfoWithListResponse {
    /// Total NTRN tokens user received as rewards for participation in the lockdrop
    pub total_ntrn_rewards: Uint128,
    /// NTRN tokens transferred to user
    pub ntrn_transferred: bool,
    /// Lockup positions
    pub lockup_infos: Vec<LockUpInfoSummary>,
    /// Number of lockup positions the user is having
    pub lockup_positions_index: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
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
    /// NTRN tokens received as rewards for participation in the lockdrop
    pub ntrn_rewards: Uint128,
    pub duration: u64,
    /// Generator NTRN tokens lockup received as generator rewards
    pub generator_ntrn_debt: Uint128,
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
