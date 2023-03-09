use crate::asset::{Asset, AssetInfo};
use crate::factory::PairType;
use crate::restricted_vector::RestrictedVector;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{to_binary, Addr, Decimal, Env, StdResult, SubMsg, Uint128, Uint64, WasmMsg};
use cw20::Cw20ReceiveMsg;

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Address that can change contract settings
    pub owner: String,
    /// Address of factory contract
    pub factory: String,
    /// Address that can set active generators and their alloc points
    pub generator_controller: Option<String>,
    /// The voting escrow delegation contract address
    pub voting_escrow_delegation: Option<String>,
    /// The voting escrow contract address
    pub voting_escrow: Option<String>,
    /// Address of guardian
    pub guardian: Option<String>,
    /// ASTRO token contract address
    pub astro_token: String,
    /// Amount of ASTRO distributed per block among all pairs
    pub tokens_per_block: Uint128,
    /// Start block for distributing ASTRO
    pub start_block: Uint64,
    /// The ASTRO vesting contract that drips ASTRO rewards
    pub vesting_contract: String,
    /// Whitelist code id
    pub whitelist_code_id: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Update the address of the ASTRO vesting contract
    /// ## Executor
    /// Only the owner can execute it.
    UpdateConfig {
        /// The new vesting contract address
        vesting_contract: Option<String>,
        /// The new generator controller contract address
        generator_controller: Option<String>,
        /// The new generator guardian
        guardian: Option<String>,
        /// The new voting escrow delegation contract address
        voting_escrow_delegation: Option<String>,
        /// The new voting escrow contract address
        voting_escrow: Option<String>,
        /// The amount of generators
        checkpoint_generator_limit: Option<u32>,
    },
    /// Setup generators with their respective allocation points.
    /// ## Executor
    /// Only the owner or generator controller can execute this.
    SetupPools {
        /// The list of pools with allocation point.
        pools: Vec<(String, Uint128)>,
    },
    /// Update the given pool's ASTRO allocation slice
    /// ## Executor
    /// Only the owner or generator controller can execute this.
    UpdatePool {
        /// The address of the LP token contract address whose allocation we change
        lp_token: String,
        /// This flag determines whether the pool gets 3rd party token rewards
        has_asset_rewards: bool,
    },
    /// Update rewards and return it to user.
    ClaimRewards {
        /// the LP token contract address
        lp_tokens: Vec<String>,
    },
    /// Withdraw LP tokens from the Generator
    Withdraw {
        /// The address of the LP token to withdraw
        lp_token: String,
        /// The amount to withdraw
        amount: Uint128,
    },
    /// Withdraw LP tokens from the Generator without withdrawing outstanding rewards
    EmergencyWithdraw {
        /// The address of the LP token to withdraw
        lp_token: String,
    },
    /// Sends orphan proxy rewards (which were left behind after emergency withdrawals) to another address
    SendOrphanProxyReward {
        /// The transfer recipient
        recipient: String,
        /// The address of the LP token contract for which we send orphaned rewards
        lp_token: String,
    },
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// Set a new amount of ASTRO to distribute per block
    /// ## Executor
    /// Only the owner can execute this.
    SetTokensPerBlock {
        /// The new amount of ASTRO to distro per block
        amount: Uint128,
    },
    /// Creates a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this.
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the proposal to change the contract owner
        expires_in: u64,
    },
    /// Removes a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this
    DropOwnershipProposal {},
    /// Claims contract ownership
    /// ## Executor
    /// Only the newly proposed owner can execute this
    ClaimOwnership {},
    /// Sets a new proxy contract for a specific generator
    /// Sets a proxy for the pool
    /// ## Executor
    /// Only the current owner or generator controller can execute this
    MoveToProxy {
        lp_token: String,
        proxy: String,
    },
    MigrateProxy {
        lp_token: String,
        new_proxy: String,
    },
    /// Add or remove token to the block list
    UpdateBlockedTokenslist {
        /// Tokens to add
        add: Option<Vec<AssetInfo>>,
        /// Tokens to remove
        remove: Option<Vec<AssetInfo>>,
    },
    /// Sets the allocation point to zero for the specified pool
    DeactivatePool {
        lp_token: String,
    },
    /// Sets the allocation point to zero for each pool by the pair type
    DeactivatePools {
        pair_types: Vec<PairType>,
    },
    /// Updates the boost emissions for specified user and generators
    CheckpointUserBoost {
        generators: Vec<String>,
        user: Option<String>,
    },
    /// Process action after the callback
    Callback {
        action: ExecuteOnReply,
    },
}

#[cw_serde]
pub enum ExecuteOnReply {
    /// Updates reward and returns it to user.
    ClaimRewards {
        /// The list of LP tokens contract
        lp_tokens: Vec<Addr>,
        /// The rewards recipient
        account: Addr,
    },
    /// Stake LP tokens in the Generator to receive token emissions
    Deposit {
        /// The LP token to stake
        lp_token: Addr,
        /// The account that receives ownership of the staked tokens
        account: Addr,
        /// The amount of tokens to deposit
        amount: Uint128,
    },
    /// Withdraw LP tokens from the Generator
    Withdraw {
        /// The LP tokens to withdraw
        lp_token: Addr,
        /// The account that receives the withdrawn LP tokens
        account: Addr,
        /// The amount of tokens to withdraw
        amount: Uint128,
    },
    /// Sets a new amount of ASTRO to distribute per block between all active generators
    SetTokensPerBlock {
        /// The new amount of ASTRO to distribute per block
        amount: Uint128,
    },
    /// Migrate LP tokens and collected rewards to new proxy
    MigrateProxy { lp_addr: Addr, new_proxy_addr: Addr },
    /// Stake LP tokens into new reward proxy
    MigrateProxyDepositLP {
        lp_addr: Addr,
        prev_proxy_addr: Addr,
        amount: Uint128,
    },
}

impl ExecuteOnReply {
    pub fn into_submsg(self, env: &Env) -> StdResult<SubMsg> {
        let msg = SubMsg::new(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::Callback { action: self })?,
            funds: vec![],
        });

        Ok(msg)
    }
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the length of the array that contains all the active pool generators
    #[returns(usize)]
    ActivePoolLength {},
    /// PoolLength returns the length of the array that contains all the instantiated pool generators
    #[returns(usize)]
    PoolLength {},
    /// Deposit returns the LP token amount deposited in a specific generator
    #[returns(Uint128)]
    Deposit { lp_token: String, user: String },
    /// Returns the current virtual amount in a specific generator
    #[returns(Uint128)]
    UserVirtualAmount { lp_token: String, user: String },
    /// Returns the total virtual supply of generator
    #[returns(Uint128)]
    TotalVirtualSupply { generator: String },
    /// PendingToken returns the amount of rewards that can be claimed by an account that deposited a specific LP token in a generator
    #[returns(PendingTokenResponse)]
    PendingToken { lp_token: String, user: String },
    /// Config returns the main contract parameters
    #[returns(Config)]
    Config {},
    /// RewardInfo returns reward information for a specified LP token
    #[returns(RewardInfoResponse)]
    RewardInfo { lp_token: String },
    /// OrphanProxyRewards returns orphaned reward information for the specified LP token
    #[returns(Vec<(AssetInfo, Uint128)>)]
    OrphanProxyRewards { lp_token: String },
    /// PoolInfo returns information about a pool associated with the specified LP token alongside
    /// the total pending amount of ASTRO and proxy rewards claimable by generator stakers (for that LP token)
    #[returns(PoolInfoResponse)]
    PoolInfo { lp_token: String },
    /// SimulateFutureReward returns the amount of ASTRO that will be distributed until a future block and for a specific generator
    #[returns(Uint128)]
    SimulateFutureReward { lp_token: String, future_block: u64 },
    /// Returns a list of stakers for a specific generator
    #[returns(Vec<StakerResponse>)]
    PoolStakers {
        lp_token: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns the blocked list of tokens
    #[returns(Vec<AssetInfo>)]
    BlockedTokensList {},
    /// Returns a list of reward proxy contracts which have been ever used
    #[returns(Vec<Addr>)]
    RewardProxiesList {},
}

/// This structure holds the response returned when querying the amount of pending rewards that can be withdrawn from a 3rd party
/// rewards contract
#[cw_serde]
pub struct PendingTokenResponse {
    /// The amount of pending ASTRO
    pub pending: Uint128,
    /// The amount of pending 3rd party reward tokens
    pub pending_on_proxy: Option<Vec<Asset>>,
}

/// This structure describes the main information of pool
#[cw_serde]
pub struct PoolInfo {
    /// Accumulated amount of reward per share unit. Used for reward calculations
    pub last_reward_block: Uint64,
    pub reward_global_index: Decimal,
    /// the reward proxy contract
    pub reward_proxy: Option<Addr>,
    /// Accumulated reward indexes per reward proxy. Vector of pairs (reward_proxy, index).
    pub accumulated_proxy_rewards_per_share: RestrictedVector<Addr, Decimal>,
    /// for calculation of new proxy rewards
    pub proxy_reward_balance_before_update: Uint128,
    /// the orphan proxy rewards which are left by emergency withdrawals. Vector of pairs (reward_proxy, index).
    pub orphan_proxy_rewards: RestrictedVector<Addr, Uint128>,
    /// The pool has assets giving additional rewards
    pub has_asset_rewards: bool,
    /// Total virtual amount
    pub total_virtual_supply: Uint128,
}

/// This structure stores the outstanding amount of token rewards that a user accrued.
/// Currently the contract works with UserInfoV2 structure, but this structure is kept for
/// compatibility with the old version.
#[cw_serde]
#[derive(Default)]
pub struct UserInfo {
    /// The amount of LP tokens staked
    pub amount: Uint128,
    /// The amount of ASTRO rewards a user already received or is not eligible for; used for proper reward calculation
    pub reward_debt: Uint128,
    /// Proxy reward amount a user already received or is not eligible for; used for proper reward calculation
    pub reward_debt_proxy: Uint128,
}

/// This structure stores the outstanding amount of token rewards that a user accrued.
#[cw_serde]
#[derive(Default)]
pub struct UserInfoV2 {
    /// The amount of LP tokens staked
    pub amount: Uint128,
    /// The amount of ASTRO rewards a user already received or is not eligible for; used for proper reward calculation
    pub reward_user_index: Decimal,
    /// Proxy reward amount a user already received per reward proxy; used for proper reward calculation
    /// Vector of pairs (reward_proxy, reward debited).
    pub reward_debt_proxy: RestrictedVector<Addr, Uint128>,
    /// The amount of user boosted emissions
    pub virtual_amount: Uint128,
}

/// This structure holds the response returned when querying for the token addresses used to reward a specific generator
#[cw_serde]
pub struct RewardInfoResponse {
    /// The address of the base reward token
    pub base_reward_token: Addr,
    /// The address of the 3rd party reward token
    pub proxy_reward_token: Option<Addr>,
}

/// This structure holds the response returned when querying for a pool's information
#[cw_serde]
pub struct PoolInfoResponse {
    /// The slice of ASTRO that this pool's generator gets per block
    pub alloc_point: Uint128,
    /// Amount of ASTRO tokens being distributed per block to this LP pool
    pub astro_tokens_per_block: Uint128,
    /// The last block when token emissions were snapshotted (distributed)
    pub last_reward_block: u64,
    /// Current block number. Useful for computing APRs off-chain
    pub current_block: u64,
    /// Total amount of ASTRO rewards already accumulated per LP token staked
    pub global_reward_index: Decimal,
    /// Pending amount of total ASTRO rewards which are claimable by stakers right now
    pub pending_astro_rewards: Uint128,
    /// The address of the 3rd party reward proxy contract
    pub reward_proxy: Option<Addr>,
    /// Pending amount of total proxy rewards which are claimable by stakers right now
    pub pending_proxy_rewards: Option<Uint128>,
    /// Total amount of 3rd party token rewards already accumulated per LP token staked per proxy
    pub accumulated_proxy_rewards_per_share: Vec<(Addr, Decimal)>,
    /// Reward balance for the dual rewards proxy before updating accrued rewards
    pub proxy_reward_balance_before_update: Uint128,
    /// The amount of orphan proxy rewards which are left behind by emergency withdrawals and not yet transferred out
    pub orphan_proxy_rewards: Vec<(Addr, Uint128)>,
    /// Total amount of lp tokens staked in the pool's generator
    pub lp_supply: Uint128,
}

/// This structure stores the core parameters for the Generator contract.
#[cw_serde]
pub struct Config {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// The Factory address
    pub factory: Addr,
    /// Contract address which can only set active generators and their alloc points
    pub generator_controller: Option<Addr>,
    /// The voting escrow contract address
    pub voting_escrow: Option<Addr>,
    /// The voting escrow delegation contract address
    pub voting_escrow_delegation: Option<Addr>,
    /// The ASTRO token address
    pub astro_token: Addr,
    /// Total amount of ASTRO rewards per block
    pub tokens_per_block: Uint128,
    /// Total allocation points. Must be the sum of all allocation points in all active generators
    pub total_alloc_point: Uint128,
    /// The block number when the ASTRO distribution starts
    pub start_block: Uint64,
    /// The vesting contract from which rewards are distributed
    pub vesting_contract: Addr,
    /// The list of active pools with allocation points
    pub active_pools: Vec<(Addr, Uint128)>,
    /// The list of blocked tokens
    pub blocked_tokens_list: Vec<AssetInfo>,
    /// The guardian address which can add or remove tokens from blacklist
    pub guardian: Option<Addr>,
    /// The amount of generators
    pub checkpoint_generator_limit: Option<u32>,
}

/// This structure describes a migration message.
#[cw_serde]
pub struct MigrateMsg {
    /// The Factory address
    pub factory: Option<String>,
    /// Contract address which can only set active generators and their alloc points
    pub generator_controller: Option<String>,
    /// The blocked list of tokens
    pub blocked_list_tokens: Option<Vec<AssetInfo>>,
    /// The guardian address
    pub guardian: Option<String>,
    /// Whitelist code id
    pub whitelist_code_id: Option<u64>,
    /// The voting escrow contract
    pub voting_escrow: Option<String>,
    /// The voting escrow delegation contract
    pub voting_escrow_delegation: Option<String>,
    /// The limit of generators
    pub generator_limit: Option<u32>,
}

/// This structure describes custom hooks for the CW20.
#[cw_serde]
pub enum Cw20HookMsg {
    /// Deposit performs a token deposit on behalf of the message sender.
    Deposit {},
    /// DepositFor performs a token deposit on behalf of another address that's not the message sender.
    DepositFor(Addr),
}

/// This structure holds the parameters used to return information about a staked in
/// a specific generator.
#[cw_serde]
pub struct StakerResponse {
    // The staker's address
    pub account: String,
    // The amount that the staker currently has in the generator
    pub amount: Uint128,
}
