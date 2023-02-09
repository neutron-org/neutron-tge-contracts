use std::convert::TryInto;

use astroport::asset::{addr_validate_to_lower, pair_info_by_pool, Asset, AssetInfo};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::generator::{
    ExecuteMsg as GenExecuteMsg, PendingTokenResponse, QueryMsg as GenQueryMsg, RewardInfoResponse,
};
use astroport::restricted_vector::RestrictedVector;
use astroport::DecimalCheckedOps;
use astroport_periphery::utils::Decimal256CheckedOps;
use cosmwasm_std::{
    attr, coins, entry_point, from_binary, to_binary, Addr, BankMsg, Binary, Coin, CosmosMsg,
    Decimal, Decimal256, Deps, DepsMut, Env, MessageInfo, Order, Response, StdError, StdResult,
    Storage, SubMsg, Uint128, Uint256, WasmMsg,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg};
use cw_storage_plus::Path;

use crate::migration::{
    migrate_generator_proxy_per_share_to_v120, ASSET_POOLS_V101, ASSET_POOLS_V111,
};
use crate::raw_queries::{raw_balance, raw_generator_deposit};
use astroport_periphery::auction::Cw20HookMsg::DelegateAstroTokens;
use astroport_periphery::lockdrop::{
    CallbackMsg, Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, LockUpInfoResponse,
    LockUpInfoSummary, LockupInfoV2, MigrateMsg, MigrationInfo, PendingAssetRewardResponse,
    PoolInfo, QueryMsg, State, StateResponse, UpdateConfigMsg, UserInfoResponse,
    UserInfoWithListResponse,
};
use astroport_periphery::U64Key;

use crate::state::{
    CompatibleLoader, ASSET_POOLS, CONFIG, LOCKUP_INFO, OWNERSHIP_PROPOSAL, STATE, USER_INFO,
};

const SECONDS_PER_WEEK: u64 = 86400 * 7;

pub const UNTRN_DENOM: &str = "untrn";

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "neutron_lockdrop";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minimum lockup positions for user.
const MIN_POSITIONS_PER_USER: u32 = 1;

/// Creates a new contract with the specified parameters packed in the `msg` variable.
/// Returns a [`Response`] with the specified attributes if the operation was successful, or a [`ContractError`] if the contract was not created
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg**  is a message of type [`InstantiateMsg`] which contains the parameters used for creating the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    // CHECK :: init_timestamp needs to be valid
    if env.block.time.seconds() > msg.init_timestamp {
        return Err(StdError::generic_err(format!(
            "Invalid init_timestamp. Current timestamp : {}",
            env.block.time.seconds()
        )));
    }

    // CHECK :: min_lock_duration , max_lock_duration need to be valid (min_lock_duration < max_lock_duration)
    if msg.max_lock_duration < msg.min_lock_duration || msg.min_lock_duration == 0u64 {
        return Err(StdError::generic_err("Invalid Lockup durations"));
    }

    // CHECK ::Weekly divider/multiplier cannot be 0
    if msg.weekly_divider == 0u64 || msg.weekly_multiplier == 0u64 {
        return Err(StdError::generic_err(
            "weekly divider/multiplier cannot be 0",
        ));
    }

    if msg.max_positions_per_user < MIN_POSITIONS_PER_USER {
        return Err(StdError::generic_err(
            "The maximum number of locked positions per user cannot be lower than a minimum acceptable value."
        ));
    }

    let config = Config {
        owner: msg
            .owner
            .map(|v| addr_validate_to_lower(deps.api, &v))
            .transpose()?
            .unwrap_or(info.sender),
        credit_contract: addr_validate_to_lower(deps.api, &msg.credit_contract)?,
        auction_contract: None,
        generator: None,
        init_timestamp: msg.init_timestamp,
        deposit_window: msg.deposit_window,
        withdrawal_window: msg.withdrawal_window,
        min_lock_duration: msg.min_lock_duration,
        max_lock_duration: msg.max_lock_duration,
        weekly_multiplier: msg.weekly_multiplier,
        weekly_divider: msg.weekly_divider,
        lockdrop_incentives: Uint128::zero(),
        max_positions_per_user: msg.max_positions_per_user,
    };

    CONFIG.save(deps.storage, &config)?;
    STATE.save(deps.storage, &State::default())?;
    Ok(Response::default())
}

/// ## Description
/// Exposes all the execute functions available in the contract.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
///
/// ## Execute messages
///
/// * **ExecuteMsg::Receive(msg)** Parse incoming messages from the cNTRN token.
///
/// * **ExecuteMsg::UpdateConfig { new_config }** Admin function to update configuration parameters.
///
/// * **ExecuteMsg::InitializePool {
///     terraswap_lp_token,
///     incentives_share,
/// }** Facilitates addition of new Pool (Terraswap Pools) whose LP tokens can then be locked in the lockdrop contract.
///
/// * **ExecuteMsg::MigrateLiquidity {
///     terraswap_lp_token,
///     astroport_pool_addr,
///     slippage_tolerance,
/// }** Migrate Liquidity from Terraswap to Astroport.
///
/// * **ExecuteMsg::StakeLpTokens { terraswap_lp_token }** Facilitates staking of Astroport LP tokens for a particular LP pool with the generator contract.
///
/// * **ExecuteMsg::EnableClaims {}** Enables NTRN Claims by users.
///
/// * **ExecuteMsg::ClaimRewardsAndOptionallyUnlock {
///             terraswap_lp_token,
///             duration,
///             withdraw_lp_stake,
///         }** Claims user Rewards for a particular Lockup position.
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a request to change contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::UpdateConfig { new_config } => handle_update_config(deps, info, new_config),
        ExecuteMsg::InitializePool {
            terraswap_lp_token,
            incentives_share,
        } => handle_initialize_pool(deps, env, info, terraswap_lp_token, incentives_share),
        ExecuteMsg::MigrateLiquidity {
            terraswap_lp_token,
            astroport_pool_addr,
            slippage_tolerance,
        } => handle_migrate_liquidity(
            deps,
            env,
            info,
            terraswap_lp_token,
            astroport_pool_addr,
            slippage_tolerance,
        ),
        ExecuteMsg::StakeLpTokens { terraswap_lp_token } => {
            handle_stake_lp_tokens(deps, env, info, terraswap_lp_token)
        }
        ExecuteMsg::EnableClaims {} => handle_enable_claims(deps, env, info),
        ExecuteMsg::ClaimRewardsAndOptionallyUnlock {
            terraswap_lp_token,
            duration,
            withdraw_lp_stake,
        } => handle_claim_rewards_and_unlock_for_lockup(
            deps,
            env,
            info,
            terraswap_lp_token,
            duration,
            withdraw_lp_stake,
        ),
        ExecuteMsg::Callback(msg) => _handle_callback(deps, env, info, msg),
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config: Config = CONFIG.load(deps.storage)?;
            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(|e| e)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;
            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL).map_err(|e| e)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;
                Ok(())
            })
            .map_err(|e| e)
        }
        ExecuteMsg::IncreaseNTRNIncentives {} => handle_increasing_ntrn_incentives(deps, env, info),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then an [`StdError`] is returned,
/// otherwise it returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`]. This is the CW20 message that has to be processed.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, StdError> {
    let user_address = addr_validate_to_lower(deps.api, &cw20_msg.sender)?;
    // CHECK :: Tokens sent > 0
    if cw20_msg.amount == Uint128::zero() {
        return Err(StdError::generic_err(
            "Number of tokens sent should be > 0 ",
        ));
    }

    let amount = cw20_msg.amount;

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::IncreaseLockup { duration } => {
            handle_increase_lockup(deps, env, info, user_address, duration, amount)
        }
    }
}

/// ## Description
/// Handles callback. Returns a [`ContractError`] on failure.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`CallbackMsg`].
fn _handle_callback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> StdResult<Response> {
    // Only the contract itself can call callbacks
    if info.sender != env.contract.address {
        return Err(StdError::generic_err(
            "callbacks cannot be invoked externally",
        ));
    }
    match msg {
        CallbackMsg::UpdatePoolOnDualRewardsClaim {
            terraswap_lp_token,
            prev_ntrn_balance,
            prev_proxy_reward_balances,
        } => update_pool_on_dual_rewards_claim(
            deps,
            env,
            terraswap_lp_token,
            prev_ntrn_balance,
            prev_proxy_reward_balances,
        ),
        CallbackMsg::WithdrawUserLockupRewardsCallback {
            terraswap_lp_token,
            user_address,
            duration,
            withdraw_lp_stake,
        } => callback_withdraw_user_rewards_for_lockup_optional_withdraw(
            deps,
            env,
            terraswap_lp_token,
            user_address,
            duration,
            withdraw_lp_stake,
        ),
        CallbackMsg::WithdrawLiquidityFromTerraswapCallback {
            terraswap_lp_token,
            astroport_pool,
            prev_assets,
            slippage_tolerance,
        } => callback_deposit_liquidity_in_astroport(
            deps,
            env,
            terraswap_lp_token,
            astroport_pool,
            prev_assets,
            slippage_tolerance,
        ),
    }
}

/// Exposes all the queries available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns the config info.
///
/// * **QueryMsg::State {}** Returns the contract's state info.
///
/// * **QueryMsg::Pool { terraswap_lp_token }** Returns info regarding a certain supported LP token pool.
///
/// * **QueryMsg::UserInfo { address }** Returns info regarding a user (total NTRN rewards, list of lockup positions).
///
/// * **QueryMsg::UserInfoWithLockupsList { address }** Returns info regarding a user with lockups.
///
/// * **QueryMsg::LockUpInfo {
///             user_address,
///             terraswap_lp_token,
///             duration,
///         }** Returns info regarding a particular lockup position with a given duration and identifer for the LP tokens locked.
///
/// * **QueryMsg::PendingAssetReward {
///             user_address,
///             terraswap_lp_token,
///             duration,
///         }** Returns the amount of pending asset rewards for the specified recipient and for a specific lockup position.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::Pool { terraswap_lp_token } => to_binary(&query_pool(deps, terraswap_lp_token)?),
        QueryMsg::UserInfo { address } => to_binary(&query_user_info(deps, env, address)?),
        QueryMsg::UserInfoWithLockupsList { address } => {
            to_binary(&query_user_info_with_lockups_list(deps, env, address)?)
        }
        QueryMsg::LockUpInfo {
            user_address,
            terraswap_lp_token,
            duration,
        } => to_binary(&query_lockup_info(
            deps,
            &env,
            &user_address,
            terraswap_lp_token,
            duration,
        )?),
    }
}

/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

/// Admin function to update Configuration parameters. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **new_config** is an object of type [`UpdateConfigMsg`]. Same as UpdateConfigMsg struct
pub fn handle_update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_config: UpdateConfigMsg,
) -> StdResult<Response> {
    let mut config = CONFIG.load(deps.storage)?;
    let mut attributes = vec![attr("action", "update_config")];

    // CHECK :: Only owner can call this function
    if info.sender != config.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    if let Some(auction) = new_config.auction_contract_address {
        match config.auction_contract {
            Some(_) => {
                return Err(StdError::generic_err("Auction contract already set."));
            }
            None => {
                config.auction_contract = Some(addr_validate_to_lower(deps.api, &auction)?);
                attributes.push(attr("auction_contract", auction))
            }
        }
    };

    if let Some(generator) = new_config.generator_address {
        // If generator is set, we check is any LP tokens are currently staked before updating generator address
        if config.generator.is_some() {
            for pool in ASSET_POOLS
                .keys(deps.storage, None, None, Order::Ascending)
                .collect::<Result<Vec<Addr>, StdError>>()?
            {
                let pool_info = ASSET_POOLS.load(deps.storage, &pool)?;
                if pool_info.is_staked {
                    return Err(StdError::generic_err(format!(
                        "{} astro LP tokens already staked. Unstake them before updating generator",
                        pool
                    )));
                }
            }
        }

        config.generator = Some(addr_validate_to_lower(deps.api, &generator)?);
        attributes.push(attr("new_generator", generator))
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attributes(attributes))
}

/// Facilitates increasing ASTRO incentives that are to be distributed as Lockdrop participation reward. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
pub fn handle_increasing_ntrn_incentives(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, StdError> {
    let mut config = CONFIG.load(deps.storage)?;

    if env.block.time.seconds()
        >= config.init_timestamp + config.deposit_window + config.withdrawal_window
    {
        return Err(StdError::generic_err("ASTRO is already being distributed"));
    };

    let incentive = info.funds.iter().find(|c| c.denom == UNTRN_DENOM);
    let amount = if let Some(coin) = incentive {
        coin.amount
    } else {
        return Err(StdError::GenericErr {
            msg: format!("{} is not found", UNTRN_DENOM),
        });
    };
    // Anyone can increase astro incentives
    config.lockdrop_incentives = config.lockdrop_incentives.checked_add(amount)?;

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new()
        .add_attribute("action", "astro_incentives_increased")
        .add_attribute("amount", amount))
}

/// Admin function to initialize new LP Pool. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **amount** is an object of type [`Uint128`]. Number of ASTRO tokens which are to be added to current incentives
///
/// * **terraswap_lp_token** is an object of type [`String`]. Terraswap LP token address
///
/// * **incentives_share** is an object of type [`u64`]. Parameter defining share of total ASTRO incentives are allocated for this pool
pub fn handle_initialize_pool(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    terraswap_lp_token: String,
    incentives_share: u64,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    // CHECK ::: Only owner can call this function
    if info.sender != config.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    // CHECK :: Is lockdrop deposit window closed
    if env.block.time.seconds() >= config.init_timestamp + config.deposit_window {
        return Err(StdError::generic_err(
            "Pools cannot be added post deposit window closure",
        ));
    }

    let terraswap_lp_token = addr_validate_to_lower(deps.api, &terraswap_lp_token)?;

    // CHECK ::: Is LP Token Pool already initialized
    if ASSET_POOLS
        .may_load(deps.storage, &terraswap_lp_token)?
        .is_some()
    {
        return Err(StdError::generic_err("Already supported"));
    }

    let terraswap_pool = {
        let res: Option<cw20::MinterResponse> = deps
            .querier
            .query_wasm_smart(&terraswap_lp_token, &Cw20QueryMsg::Minter {})?;
        addr_validate_to_lower(
            deps.api,
            &res.ok_or_else(|| StdError::generic_err("No minter for the LP token!"))?
                .minter,
        )?
    };

    // POOL INFO :: Initialize new pool
    let pool_info = PoolInfo {
        terraswap_pool,
        terraswap_amount_in_lockups: Default::default(),
        migration_info: None,
        incentives_share,
        weighted_amount: Default::default(),
        generator_ntrn_per_share: Default::default(),
        generator_proxy_per_share: RestrictedVector::default(),
        is_staked: false,
    };
    // STATE UPDATE :: Save state and PoolInfo
    ASSET_POOLS.save(deps.storage, &terraswap_lp_token, &pool_info)?;

    state.total_incentives_share += incentives_share;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "initialize_pool"),
        attr("terraswap_lp_token", terraswap_lp_token),
        attr("incentives_share", incentives_share.to_string()),
    ]))
}

/// Admin function to update LP Pool Configuration. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **amount** is an object of type [`Uint128`]. Number of ASTRO tokens which are to be added to current incentives
///
/// * **terraswap_lp_token** is an object of type [`String`]. Parameter to identify the pool. Equals pool's terraswap Lp token address
///
/// * **incentives_share** is an object of type [`u64`]. Parameter defining share of total ASTRO incentives are allocated for this pool
pub fn handle_update_pool(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    terraswap_lp_token: String,
    incentives_share: u64,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    // CHECK ::: Only owner can call this function
    if info.sender != config.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    // CHECK :: Is lockdrop deposit window closed
    if env.block.time.seconds() >= config.init_timestamp + config.deposit_window {
        return Err(StdError::generic_err(
            "Pools cannot be updated post deposit window closure",
        ));
    }

    let terraswap_lp_token = addr_validate_to_lower(deps.api, &terraswap_lp_token)?;

    // CHECK ::: Is LP Token Pool initialized
    let mut pool_info = ASSET_POOLS.load(deps.storage, &terraswap_lp_token)?;

    // CHECK ::: Incentives cannot be decreased when lockdrop in process
    if env.block.time.seconds() > config.init_timestamp
        && incentives_share < pool_info.incentives_share
    {
        return Err(StdError::generic_err(
            "Lockdrop in process. Incentives cannot be decreased for any pool",
        ));
    }

    // update total incentives
    state.total_incentives_share =
        state.total_incentives_share - pool_info.incentives_share + incentives_share;

    // Update Pool Incentives
    pool_info.incentives_share = incentives_share;

    ASSET_POOLS.save(deps.storage, &terraswap_lp_token, &pool_info)?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "update_pool"),
        attr("terraswap_lp_token", terraswap_lp_token),
        attr("set_incentives_share", incentives_share.to_string()),
    ]))
}

/// Enable ASTRO Claims by users. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
pub fn handle_enable_claims(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    // CHECK :: ONLY AUCTION CONTRACT CAN CALL THIS FUNCTION
    if let Some(auction) = config.auction_contract {
        if info.sender != auction {
            return Err(StdError::generic_err("Unauthorized"));
        }
    } else {
        return Err(StdError::generic_err("Auction contract hasn't been set!"));
    }

    // CHECK :: Have the deposit / withdraw windows concluded
    if env.block.time.seconds()
        < (config.init_timestamp + config.deposit_window + config.withdrawal_window)
    {
        return Err(StdError::generic_err(
            "Deposit / withdraw windows not closed yet",
        ));
    }

    // CHECK ::: Claims are only enabled once
    if state.are_claims_allowed {
        return Err(StdError::generic_err("Already allowed"));
    }
    state.are_claims_allowed = true;

    STATE.save(deps.storage, &state)?;
    Ok(Response::new().add_attribute("action", "allow_claims"))
}

/// Migrates Liquidity from Terraswap to Astroport. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **terraswap_lp_token** is an object of type [`String`]. Parameter to identify the pool
///
/// * **astroport_pool_addr** is an object of type [`String`].
///
/// * **slippage_tolerance** is an optional object of type [`Decimal`]. Astroport Pool address to which the liquidity is to be migrated
pub fn handle_migrate_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    terraswap_lp_token: String,
    astroport_pool_addr: String,
    slippage_tolerance: Option<Decimal>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    // Only owner can call this function
    if info.sender != config.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    // CHECK :: may the liquidity be migrated or not ?
    if env.block.time.seconds()
        < config.init_timestamp + config.deposit_window + config.withdrawal_window
    {
        return Err(StdError::generic_err(
            "Deposit / Withdrawal windows not closed",
        ));
    }
    let terraswap_lp_token = addr_validate_to_lower(deps.api, &terraswap_lp_token)?;
    let astroport_pool = addr_validate_to_lower(deps.api, &astroport_pool_addr)?;

    let mut pool_info = ASSET_POOLS.load(deps.storage, &terraswap_lp_token)?;

    // CHECK :: has the liquidity already been migrated or not ?
    if pool_info.migration_info.is_some() {
        return Err(StdError::generic_err("Liquidity already migrated"));
    }

    let mut cosmos_msgs: Vec<CosmosMsg> = vec![];

    let lp_balance: BalanceResponse = deps.querier.query_wasm_smart(
        &terraswap_lp_token,
        &Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;

    // COSMOS MSG :: WITHDRAW LIQUIDITY FROM TERRASWAP
    let msg = WasmMsg::Execute {
        contract_addr: terraswap_lp_token.to_string(),
        funds: vec![],
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: pool_info.terraswap_pool.to_string(),
            msg: to_binary(&terraswap::pair::Cw20HookMsg::WithdrawLiquidity {})?,
            amount: lp_balance.balance,
        })?,
    };
    cosmos_msgs.push(msg.into());

    let terraswap_lp_info: terraswap::asset::PairInfo = deps.querier.query_wasm_smart(
        &pool_info.terraswap_pool,
        &terraswap::pair::QueryMsg::Pair {},
    )?;

    let mut assets = vec![];

    for asset_info in terraswap_lp_info.asset_infos.iter() {
        assets.push(terraswap::asset::Asset {
            amount: match &asset_info {
                terraswap::asset::AssetInfo::NativeToken { denom } => {
                    terraswap::querier::query_balance(
                        &deps.querier,
                        env.contract.address.clone(),
                        denom.clone(),
                    )?
                }
                terraswap::asset::AssetInfo::Token { contract_addr } => {
                    terraswap::querier::query_token_balance(
                        &deps.querier,
                        addr_validate_to_lower(deps.api, contract_addr)?,
                        env.contract.address.clone(),
                    )?
                }
            },
            info: asset_info.to_owned(),
        })
    }

    // COSMOS MSG :: CALLBACK AFTER LIQUIDITY WITHDRAWAL
    let update_state_msg = CallbackMsg::WithdrawLiquidityFromTerraswapCallback {
        terraswap_lp_token: terraswap_lp_token.clone(),
        astroport_pool: astroport_pool.clone(),
        prev_assets: assets.try_into().unwrap(),
        slippage_tolerance,
    }
    .to_cosmos_msg(&env)?;
    cosmos_msgs.push(update_state_msg);

    let astroport_lp_token = {
        let msg = astroport::pair::QueryMsg::Pair {};
        let res: astroport::asset::PairInfo =
            deps.querier.query_wasm_smart(&astroport_pool, &msg)?;
        res.liquidity_token
    };

    pool_info.migration_info = Some(MigrationInfo {
        astroport_lp_token,
        terraswap_migrated_amount: lp_balance.balance,
    });
    ASSET_POOLS.save(deps.storage, &terraswap_lp_token, &pool_info)?;

    Ok(Response::new().add_messages(cosmos_msgs))
}

/// Stakes one of the supported LP Tokens with the Generator contract. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **terraswap_lp_token** is an object of type [`String`]. Pool's terraswap LP token address whose Astroport LP tokens are to be staked.
pub fn handle_stake_lp_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    terraswap_lp_token: String,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    let mut cosmos_msgs = vec![];

    // CHECK ::: Only owner can call this function
    if info.sender != config.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    let terraswap_lp_token = addr_validate_to_lower(deps.api, &terraswap_lp_token)?;

    // CHECK ::: Is LP Token Pool supported or not ?
    let mut pool_info = ASSET_POOLS.load(deps.storage, &terraswap_lp_token)?;

    let MigrationInfo {
        astroport_lp_token, ..
    } = pool_info
        .migration_info
        .as_ref()
        .ok_or_else(|| StdError::generic_err("Terraswap liquidity hasn't migrated yet!"))?;

    let amount = {
        let res: BalanceResponse = deps.querier.query_wasm_smart(
            astroport_lp_token,
            &Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?;
        res.balance
    };

    let generator = config
        .generator
        .as_ref()
        .ok_or_else(|| StdError::generic_err("Generator address hasn't set yet!"))?;

    cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: astroport_lp_token.to_string(),
        funds: vec![],
        msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
            spender: generator.to_string(),
            amount,
            expires: Some(cw20::Expiration::AtHeight(env.block.height + 1u64)),
        })?,
    }));

    cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: astroport_lp_token.to_string(),
        funds: vec![],
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: generator.to_string(),
            msg: to_binary(&astroport::generator::Cw20HookMsg::Deposit {})?,
            amount,
        })?,
    }));

    // UPDATE STATE & SAVE
    pool_info.is_staked = true;
    ASSET_POOLS.save(deps.storage, &terraswap_lp_token, &pool_info)?;

    Ok(Response::new()
        .add_messages(cosmos_msgs)
        .add_attributes(vec![
            attr("action", "stake_to_generator"),
            attr("terraswap_lp_token", terraswap_lp_token),
            attr("astroport_lp_amount", amount),
        ]))
}

/// Hook function to increase Lockup position size when any of the supported LP Tokens are sent to the contract by the user. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **user_address** is an object of type [`Addr`]. User which sent the following LP token
///
/// * **duration** is an object of type [`u64`]. Number of weeks the LP token is locked for (lockup period begins post the withdrawal window closure).
///
/// * **amount** is an object of type [`Uint128`]. Number of LP tokens sent by the user.
pub fn handle_increase_lockup(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    user_address: Addr,
    duration: u64,
    amount: Uint128,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let terraswap_lp_token = info.sender;

    // CHECK ::: LP Token supported or not ?
    let mut pool_info = ASSET_POOLS.load(deps.storage, &terraswap_lp_token)?;
    let mut user_info = USER_INFO
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    // CHECK :: Lockdrop deposit window open
    let current_time = env.block.time.seconds();
    if current_time < config.init_timestamp
        || current_time >= config.init_timestamp + config.deposit_window
    {
        return Err(StdError::generic_err("Deposit window closed"));
    }

    // CHECK :: Valid Lockup Duration
    if duration > config.max_lock_duration || duration < config.min_lock_duration {
        return Err(StdError::generic_err(format!(
            "Lockup duration needs to be between {} and {}",
            config.min_lock_duration, config.max_lock_duration
        )));
    }

    pool_info.weighted_amount += calculate_weight(amount, duration, &config)?;
    pool_info.terraswap_amount_in_lockups += amount;

    let lockup_key = (&terraswap_lp_token, &user_address, U64Key::new(duration));

    let lockup_info = match LOCKUP_INFO.compatible_may_load(
        deps.as_ref(),
        lockup_key.clone(),
        &config.generator,
    )? {
        Some(mut li) => {
            li.lp_units_locked = li.lp_units_locked.checked_add(amount)?;
            li
        }
        None => {
            if config.max_positions_per_user == user_info.lockup_positions_index {
                return Err(StdError::generic_err(format!(
                    "Users can only have max {} lockup positions",
                    config.max_positions_per_user
                )));
            }
            // Update number of lockup positions the user is having
            user_info.lockup_positions_index += 1;

            LockupInfoV2 {
                lp_units_locked: amount,
                astroport_lp_transferred: None,
                ntrn_rewards: Uint128::zero(),
                unlock_timestamp: config.init_timestamp
                    + config.deposit_window
                    + config.withdrawal_window
                    + (duration * SECONDS_PER_WEEK),
                generator_ntrn_debt: Uint128::zero(),
                generator_proxy_debt: Default::default(),
                withdrawal_flag: false,
            }
        }
    };

    // SAVE UPDATED STATE
    LOCKUP_INFO.save(deps.storage, lockup_key, &lockup_info)?;
    ASSET_POOLS.save(deps.storage, &terraswap_lp_token, &pool_info)?;
    USER_INFO.save(deps.storage, &user_address, &user_info)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "increase_lockup_position"),
        attr("terraswap_lp_token", terraswap_lp_token),
        attr("user", user_address),
        attr("duration", duration.to_string()),
        attr("amount", amount),
    ]))
}

/// Claims user Rewards for a particular Lockup position. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **terraswap_lp_token** is an object of type [`String`]. Terraswap LP token to identify the LP pool whose Token is locked in the lockup position.
///
/// * **duration** is an object of type [`u64`]. Lockup duration (number of weeks).
///
/// * **withdraw_lp_stake** is an object of type [`bool`]. Boolean value indicating if the LP tokens are to be withdrawn or not.
pub fn handle_claim_rewards_and_unlock_for_lockup(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    terraswap_lp_token: String,
    duration: u64,
    withdraw_lp_stake: bool,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;

    if !state.are_claims_allowed {
        return Err(StdError::generic_err("Reward claim not allowed"));
    }

    if env.block.time.seconds()
        < config.init_timestamp + config.deposit_window + config.withdrawal_window
    {
        return Err(StdError::generic_err(
            "Deposit / withdraw windows are still open",
        ));
    }

    let user_address = info.sender;

    let terraswap_lp_token = addr_validate_to_lower(deps.api, &terraswap_lp_token)?;

    // CHECK ::: Is LP Token Pool supported or not ?
    let pool_info = ASSET_POOLS.load(deps.storage, &terraswap_lp_token)?;

    let mut user_info = USER_INFO
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    // If user's total ASTRO rewards == 0 :: We update all of the user's lockup positions to calculate ASTRO rewards and for each alongwith their equivalent Astroport LP Shares
    if user_info.total_ntrn_rewards == Uint128::zero() {
        user_info.total_ntrn_rewards = update_user_lockup_positions_and_calc_rewards(
            deps.branch(),
            &config,
            &state,
            &user_address,
        )?;
    }

    USER_INFO.save(deps.storage, &user_address, &user_info)?;

    // Check is there lockup or not ?
    let lockup_key = (&terraswap_lp_token, &user_address, U64Key::new(duration));
    let lockup_info =
        LOCKUP_INFO.compatible_load(deps.as_ref(), lockup_key.clone(), &config.generator)?;

    // CHECK :: Can the Lockup position be unlocked or not ?
    if withdraw_lp_stake && env.block.time.seconds() < lockup_info.unlock_timestamp {
        return Err(StdError::generic_err(format!(
            "{} seconds to unlock",
            lockup_info.unlock_timestamp - env.block.time.seconds()
        )));
    }

    if lockup_info.astroport_lp_transferred.is_some() {
        return Err(StdError::generic_err(
            "Astro LP Tokens have already been claimed!",
        ));
    }

    let mut cosmos_msgs = vec![];

    if let Some(MigrationInfo {
        astroport_lp_token, ..
    }) = &pool_info.migration_info
    {
        if pool_info.is_staked {
            let generator = config
                .generator
                .as_ref()
                .ok_or_else(|| StdError::generic_err("Generator should be set at this moment!"))?;

            // QUERY :: Check if there are any pending staking rewards
            let pending_rewards: PendingTokenResponse = deps.querier.query_wasm_smart(
                generator,
                &GenQueryMsg::PendingToken {
                    lp_token: astroport_lp_token.to_string(),
                    user: env.contract.address.to_string(),
                },
            )?;

            let pending_on_proxy = &pending_rewards.pending_on_proxy.unwrap_or_default();

            if !pending_rewards.pending.is_zero()
                || pending_on_proxy.iter().any(|asset| !asset.amount.is_zero())
            {
                let rwi: RewardInfoResponse = deps.querier.query_wasm_smart(
                    generator,
                    &GenQueryMsg::RewardInfo {
                        lp_token: astroport_lp_token.to_string(),
                    },
                )?;

                let astro_balance = {
                    let res: BalanceResponse = deps.querier.query_wasm_smart(
                        rwi.base_reward_token,
                        &Cw20QueryMsg::Balance {
                            address: env.contract.address.to_string(),
                        },
                    )?;
                    res.balance
                };

                let prev_proxy_reward_balances: Vec<Asset> = pending_on_proxy
                    .iter()
                    .map(|asset| {
                        let balance = asset
                            .info
                            .query_pool(&deps.querier, env.contract.address.clone())
                            .unwrap_or_default();

                        Asset {
                            info: asset.info.clone(),
                            amount: balance,
                        }
                    })
                    .collect();

                cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: generator.to_string(),
                    funds: vec![],
                    msg: to_binary(&GenExecuteMsg::Withdraw {
                        lp_token: astroport_lp_token.to_string(),
                        amount: Uint128::zero(),
                    })?,
                }));

                cosmos_msgs.push(
                    CallbackMsg::UpdatePoolOnDualRewardsClaim {
                        terraswap_lp_token: terraswap_lp_token.clone(),
                        prev_ntrn_balance: astro_balance,
                        prev_proxy_reward_balances,
                    }
                    .to_cosmos_msg(&env)?,
                );
            }
        } else if user_info.ntrn_transferred && !withdraw_lp_stake {
            return Err(StdError::generic_err("No rewards available to claim!"));
        }
    }

    cosmos_msgs.push(
        CallbackMsg::WithdrawUserLockupRewardsCallback {
            terraswap_lp_token,
            user_address,
            duration,
            withdraw_lp_stake,
        }
        .to_cosmos_msg(&env)?,
    );

    Ok(Response::new().add_messages(cosmos_msgs))
}

/// Updates contract state after dual staking rewards are claimed from the generator contract. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **terraswap_lp_token** is an object of type [`String`]. Pool identifier to identify the LP pool whose rewards have been claimed.
///
/// * **prev_ntrn_balance** is an object of type [`Uint128`]. Contract's NTRN token balance before claim.
///
/// * **prev_proxy_reward_balances** is a vector of type [`Asset`]. Contract's Generator Proxy reward token balance before claim.
pub fn update_pool_on_dual_rewards_claim(
    deps: DepsMut,
    env: Env,
    terraswap_lp_token: Addr,
    prev_ntrn_balance: Uint128,
    prev_proxy_reward_balances: Vec<Asset>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut pool_info = ASSET_POOLS.load(deps.storage, &terraswap_lp_token)?;

    let generator = config
        .generator
        .as_ref()
        .ok_or_else(|| StdError::generic_err("Generator hasn't been set yet!"))?;
    let MigrationInfo {
        astroport_lp_token, ..
    } = pool_info
        .migration_info
        .as_ref()
        .ok_or_else(|| StdError::generic_err("Pool should be migrated!"))?;

    let rwi: RewardInfoResponse = deps.querier.query_wasm_smart(
        generator,
        &GenQueryMsg::RewardInfo {
            lp_token: astroport_lp_token.to_string(),
        },
    )?;

    let lp_balance: Uint128 = deps.querier.query_wasm_smart(
        generator,
        &GenQueryMsg::Deposit {
            lp_token: astroport_lp_token.to_string(),
            user: env.contract.address.to_string(),
        },
    )?;

    let base_reward_received;
    // Increment claimed Astro rewards per LP share
    pool_info.generator_ntrn_per_share += {
        let res: BalanceResponse = deps.querier.query_wasm_smart(
            rwi.base_reward_token,
            &Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?;
        base_reward_received = res.balance - prev_ntrn_balance;
        Decimal::from_ratio(base_reward_received, lp_balance)
    };

    // Increment claimed Proxy rewards per LP share
    for prev_balance in prev_proxy_reward_balances {
        let current_balance = prev_balance
            .info
            .query_pool(&deps.querier, env.contract.address.clone())?;
        let received_amount = current_balance - prev_balance.amount;
        pool_info.generator_proxy_per_share.update(
            &prev_balance.info,
            Decimal::from_ratio(received_amount, lp_balance),
        )?;
    }

    // SAVE UPDATED STATE OF THE POOL
    ASSET_POOLS.save(deps.storage, &terraswap_lp_token, &pool_info)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "update_generator_dual_rewards"),
        attr("terraswap_lp_token", terraswap_lp_token),
        attr("astro_reward_received", base_reward_received),
        attr(
            "generator_astro_per_share",
            pool_info.generator_ntrn_per_share.to_string(),
        ),
    ]))
}

/// Withdraws user rewards and LP Tokens after claims / unlocks. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **terraswap_lp_token** is an object of type [`Addr`]. Pool identifier to identify the LP pool.
///
/// * **user_address** is an object of type [`Addr`]. User address who is claiming the rewards / unlocking his lockup position.
///
/// * **duration** is a vector of type [`u64`]. Duration of the lockup for which rewards have been claimed / position unlocked.
///
/// * **withdraw_lp_stake** is an object of type [`bool`]. Boolean value indicating if the ASTRO LP Tokens are to be sent to the user or not.
pub fn callback_withdraw_user_rewards_for_lockup_optional_withdraw(
    deps: DepsMut,
    env: Env,
    terraswap_lp_token: Addr,
    user_address: Addr,
    duration: u64,
    withdraw_lp_stake: bool,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut pool_info = ASSET_POOLS.load(deps.storage, &terraswap_lp_token)?;
    let lockup_key = (&terraswap_lp_token, &user_address, U64Key::new(duration));
    let mut lockup_info =
        LOCKUP_INFO.compatible_load(deps.as_ref(), lockup_key.clone(), &config.generator)?;

    let mut user_info = USER_INFO
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    let mut cosmos_msgs = vec![];
    let mut attributes = vec![
        attr("action", "withdraw_rewards_and_or_unlock"),
        attr("terraswap_lp_token", &terraswap_lp_token),
        attr("user_address", &user_address),
        attr("duration", duration.to_string()),
    ];

    if let Some(MigrationInfo {
        astroport_lp_token, ..
    }) = &pool_info.migration_info
    {
        let generator = config
            .generator
            .as_ref()
            .ok_or_else(|| StdError::generic_err("Generator should be set"))?;

        // Calculate Astro LP share for the lockup position
        let astroport_lp_amount: Uint128 = {
            let balance: Uint128 = if pool_info.is_staked {
                deps.querier.query_wasm_smart(
                    generator,
                    &GenQueryMsg::Deposit {
                        lp_token: astroport_lp_token.to_string(),
                        user: env.contract.address.to_string(),
                    },
                )?
            } else {
                let res: BalanceResponse = deps.querier.query_wasm_smart(
                    astroport_lp_token,
                    &Cw20QueryMsg::Balance {
                        address: env.contract.address.to_string(),
                    },
                )?;
                res.balance
            };

            (lockup_info
                .lp_units_locked
                .full_mul(balance)
                .checked_div(Uint256::from(pool_info.terraswap_amount_in_lockups))?)
            .try_into()?
        };

        // If Astro LP tokens are staked with Astro generator
        if pool_info.is_staked {
            let rwi: RewardInfoResponse = deps.querier.query_wasm_smart(
                generator,
                &GenQueryMsg::RewardInfo {
                    lp_token: astroport_lp_token.to_string(),
                },
            )?;

            // Calculate claimable Astro staking rewards for this lockup
            let total_lockup_astro_rewards =
                pool_info.generator_ntrn_per_share * astroport_lp_amount;
            let pending_astro_rewards =
                total_lockup_astro_rewards.checked_sub(lockup_info.generator_ntrn_debt)?;
            lockup_info.generator_ntrn_debt = total_lockup_astro_rewards;

            // If claimable Astro staking rewards > 0, claim them
            if pending_astro_rewards > Uint128::zero() {
                cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: rwi.base_reward_token.to_string(),
                    funds: vec![],
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: user_address.to_string(),
                        amount: pending_astro_rewards,
                    })?,
                }));
            }
            attributes.push(attr("generator_astro_reward", pending_astro_rewards));

            let mut pending_proxy_rewards: Vec<Asset> = vec![];
            // If this LP token is getting dual incentives
            // Calculate claimable proxy staking rewards for this lockup
            lockup_info.generator_proxy_debt = lockup_info
                .generator_proxy_debt
                .inner_ref()
                .iter()
                .map(|(asset, debt)| {
                    let generator_proxy_per_share = pool_info
                        .generator_proxy_per_share
                        .load(asset)
                        .unwrap_or_default();
                    let total_lockup_proxy_reward =
                        generator_proxy_per_share.checked_mul_uint128(astroport_lp_amount)?;
                    let pending_proxy_reward: Uint128 =
                        total_lockup_proxy_reward.checked_sub(*debt)?;

                    if !pending_proxy_reward.is_zero() {
                        pending_proxy_rewards.push(Asset {
                            info: asset.clone(),
                            amount: pending_proxy_reward,
                        });
                    }
                    Ok((asset.clone(), total_lockup_proxy_reward))
                })
                .collect::<StdResult<Vec<_>>>()?
                .into();

            // If this is a void transaction (no state change), then return error.
            // Void tx scenario = ASTRO already claimed, 0 pending ASTRO staking reward, 0 pending proxy rewards, not unlocking LP tokens in this tx
            if !withdraw_lp_stake
                && user_info.ntrn_transferred
                && pending_astro_rewards == Uint128::zero()
                && pending_proxy_rewards.is_empty()
            {
                return Err(StdError::generic_err("No rewards available to claim!"));
            }

            // If claimable proxy staking rewards > 0, claim them
            for pending_proxy_reward in pending_proxy_rewards {
                cosmos_msgs
                    .push(pending_proxy_reward.into_msg(&deps.querier, user_address.clone())?);
            }

            //  COSMOSMSG :: If LP Tokens are staked, we unstake the amount which needs to be returned to the user
            if withdraw_lp_stake {
                cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: generator.to_string(),
                    funds: vec![],
                    msg: to_binary(&GenExecuteMsg::Withdraw {
                        lp_token: astroport_lp_token.to_string(),
                        amount: astroport_lp_amount,
                    })?,
                }));
            }
        }

        if withdraw_lp_stake {
            // COSMOSMSG :: Returns LP units locked by the user in the current lockup position
            cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: astroport_lp_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: user_address.to_string(),
                    amount: astroport_lp_amount,
                })?,
                funds: vec![],
            }));
            pool_info.terraswap_amount_in_lockups = pool_info
                .terraswap_amount_in_lockups
                .checked_sub(lockup_info.lp_units_locked)?;
            ASSET_POOLS.save(deps.storage, &terraswap_lp_token, &pool_info)?;

            attributes.push(attr("astroport_lp_unlocked", astroport_lp_amount));
            lockup_info.astroport_lp_transferred = Some(astroport_lp_amount);
        }
        LOCKUP_INFO.save(deps.storage, lockup_key, &lockup_info)?;
    } else if withdraw_lp_stake {
        return Err(StdError::generic_err("Pool should be migrated!"));
    }

    // Transfers claimable one time ASTRO rewards to the user that the user gets for all his lock
    if !user_info.ntrn_transferred {
        // Calculating how much Astro user can claim (from total one time reward)
        let total_claimable_astro_rewards = user_info.total_ntrn_rewards;
        if total_claimable_astro_rewards > Uint128::zero() {
            cosmos_msgs.push(CosmosMsg::Bank(BankMsg::Send {
                to_address: user_address.to_string(),
                amount: coins(total_claimable_astro_rewards.u128(), UNTRN_DENOM),
            }))
        }
        user_info.ntrn_transferred = true;
        attributes.push(attr(
            "total_claimable_astro_reward",
            total_claimable_astro_rewards,
        ));
        USER_INFO.save(deps.storage, &user_address, &user_info)?;
    }

    Ok(Response::new()
        .add_messages(cosmos_msgs)
        .add_attributes(attributes))
}

/// Deposits Liquidity in Astroport after its withdrawn from terraswap. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **terraswap_lp_token** is an object of type [`Addr`]. Pool identifier to identify the LP pool.
///
/// * **astroport_pool** is an object of type [`Addr`]. Astroport Pool details to which the liquidity is to be migrated.
///
/// * **prev_assets** is a array of type [`terraswap::asset::Asset`]. Balances of terraswap pool assets before liquidity was withdrawn.
///
/// * **slippage_tolerance** is an optional object of type [`Decimal`].
pub fn callback_deposit_liquidity_in_astroport(
    deps: DepsMut,
    env: Env,
    terraswap_lp_token: Addr,
    astroport_pool: Addr,
    prev_assets: [terraswap::asset::Asset; 2],
    slippage_tolerance: Option<Decimal>,
) -> StdResult<Response> {
    let mut cosmos_msgs = vec![];

    let mut assets = vec![];
    let mut coins = vec![];

    for prev_asset in prev_assets.iter() {
        match prev_asset.info.clone() {
            terraswap::asset::AssetInfo::NativeToken { denom } => {
                let mut new_asset = astroport::asset::Asset {
                    info: astroport::asset::AssetInfo::NativeToken {
                        denom: denom.clone(),
                    },
                    amount: terraswap::querier::query_balance(
                        &deps.querier,
                        env.contract.address.clone(),
                        denom.clone(),
                    )?
                    .checked_sub(prev_asset.amount)?,
                };

                new_asset.amount -= new_asset.compute_tax(&deps.querier)?;

                coins.push(Coin {
                    denom,
                    amount: new_asset.amount,
                });
                assets.push(new_asset);
            }
            terraswap::asset::AssetInfo::Token { contract_addr } => {
                let amount = terraswap::querier::query_token_balance(
                    &deps.querier,
                    addr_validate_to_lower(deps.api, &contract_addr)?,
                    env.contract.address.clone(),
                )?
                .checked_sub(prev_asset.amount)?;

                cosmos_msgs.push(
                    WasmMsg::Execute {
                        contract_addr: contract_addr.to_string(),
                        funds: vec![],
                        msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                            spender: astroport_pool.to_string(),
                            expires: Some(cw20::Expiration::AtHeight(env.block.height + 1u64)),
                            amount,
                        })?,
                    }
                    .into(),
                );

                assets.push(astroport::asset::Asset {
                    info: astroport::asset::AssetInfo::Token {
                        contract_addr: addr_validate_to_lower(deps.api, &contract_addr)?,
                    },
                    amount,
                });
            }
        }
    }

    coins.sort_by(|a, b| a.denom.cmp(&b.denom));

    cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: astroport_pool.to_string(),
        funds: coins,
        msg: to_binary(&astroport::pair::ExecuteMsg::ProvideLiquidity {
            assets: assets.clone().try_into().unwrap(),
            slippage_tolerance,
            auto_stake: None,
            receiver: None,
        })?,
    }));

    Ok(Response::new()
        .add_messages(cosmos_msgs)
        .add_attributes(vec![
            attr("action", "migrate_liquidity_to_astroport"),
            attr("terraswap_lp_token", terraswap_lp_token),
            attr("astroport_pool", astroport_pool),
            attr("liquidity", format!("{}-{}", assets[0], assets[1])),
        ]))
}

/// Returns the contract's State.
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_state(deps: Deps) -> StdResult<StateResponse> {
    let state: State = STATE.load(deps.storage)?;
    Ok(StateResponse {
        total_incentives_share: state.total_incentives_share,
        are_claims_allowed: state.are_claims_allowed,
        supported_pairs_list: ASSET_POOLS
            .keys(deps.storage, None, None, Order::Ascending)
            .collect::<Result<Vec<Addr>, StdError>>()?,
    })
}

/// Returns the pool's State.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **terraswap_lp_token** is an object of type [`String`].
pub fn query_pool(deps: Deps, terraswap_lp_token: String) -> StdResult<PoolInfo> {
    let terraswap_lp_token = addr_validate_to_lower(deps.api, &terraswap_lp_token)?;
    let pool_info: PoolInfo = ASSET_POOLS.load(deps.storage, &terraswap_lp_token)?;
    Ok(pool_info)
}

/// Returns summarized details regarding the user.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **user** is an object of type [`String`].
pub fn query_user_info(deps: Deps, env: Env, user: String) -> StdResult<UserInfoResponse> {
    let user_address = addr_validate_to_lower(deps.api, &user)?;
    let user_info = USER_INFO
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    let mut total_astro_rewards = Uint128::zero();
    let mut lockup_infos = vec![];

    let mut claimable_generator_astro_debt = Uint128::zero();
    for pool in ASSET_POOLS
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<Addr>, StdError>>()?
    {
        for duration in LOCKUP_INFO
            .prefix((&pool, &user_address))
            .keys(deps.storage, None, None, Order::Ascending)
            .collect::<Result<Vec<u64>, StdError>>()?
        {
            let lockup_info = query_lockup_info(deps, &env, &user, pool.to_string(), duration)?;
            total_astro_rewards += lockup_info.astro_rewards;
            claimable_generator_astro_debt += lockup_info.claimable_generator_astro_debt;
            lockup_infos.push(lockup_info);
        }
    }

    Ok(UserInfoResponse {
        total_astro_rewards,
        astro_transferred: user_info.ntrn_transferred,
        lockup_infos,
        claimable_generator_astro_debt,
        lockup_positions_index: user_info.lockup_positions_index,
    })
}

/// Returns summarized details regarding the user with lockups list.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **user** is an object of type [`String`].
pub fn query_user_info_with_lockups_list(
    deps: Deps,
    _env: Env,
    user: String,
) -> StdResult<UserInfoWithListResponse> {
    let user_address = addr_validate_to_lower(deps.api, &user)?;
    let user_info = USER_INFO
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    let mut lockup_infos = vec![];

    for pool in ASSET_POOLS
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<Addr>, StdError>>()?
    {
        for duration in LOCKUP_INFO
            .prefix((&pool, &user_address))
            .keys(deps.storage, None, None, Order::Ascending)
            .collect::<Result<Vec<u64>, StdError>>()?
        {
            lockup_infos.push(LockUpInfoSummary {
                pool_address: pool.to_string(),
                duration,
            });
        }
    }

    Ok(UserInfoWithListResponse {
        total_astro_rewards: user_info.total_ntrn_rewards,
        astro_transferred: user_info.ntrn_transferred,
        lockup_infos,
        lockup_positions_index: user_info.lockup_positions_index,
    })
}

/// Returns summarized details regarding the user
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **user_address** is an object of type [`&str`].
///
/// * **terraswap_lp_token** is an object of type [`String`].
///
/// * **duration** is an object of type [`u64`].
pub fn query_lockup_info(
    deps: Deps,
    env: &Env,
    user_address: &str,
    terraswap_lp_token: String,
    duration: u64,
) -> StdResult<LockUpInfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;

    let terraswap_lp_token = addr_validate_to_lower(deps.api, &terraswap_lp_token)?;
    let user_address = addr_validate_to_lower(deps.api, user_address)?;

    let lockup_key = (&terraswap_lp_token, &user_address, U64Key::new(duration));
    let mut pool_info = ASSET_POOLS.load(deps.storage, &terraswap_lp_token)?;
    let mut lockup_info = LOCKUP_INFO.compatible_load(deps, lockup_key, &config.generator)?;

    let mut lockup_astroport_lp_units_opt: Option<Uint128> = None;
    let mut astroport_lp_token_opt: Option<Addr> = None;
    let mut claimable_generator_astro_debt = Uint128::zero();
    let mut claimable_generator_proxy_debt: RestrictedVector<AssetInfo, Uint128> =
        RestrictedVector::default();
    if let Some(astroport_lp_transferred) = lockup_info.astroport_lp_transferred {
        lockup_astroport_lp_units_opt = Some(astroport_lp_transferred);
        astroport_lp_token_opt = pool_info.migration_info.map(|v| v.astroport_lp_token);
    } else if let Some(MigrationInfo {
        astroport_lp_token, ..
    }) = pool_info.migration_info.clone()
    {
        let pool_astroport_lp_units;
        let lockup_astroport_lp_units = {
            // Query Astro LP Tokens balance for the pool
            pool_astroport_lp_units = if pool_info.is_staked {
                raw_generator_deposit(
                    deps.querier,
                    config
                        .generator
                        .as_ref()
                        .ok_or_else(|| StdError::generic_err("Should be set!"))?,
                    astroport_lp_token.as_bytes(),
                    env.contract.address.as_bytes(),
                )?
            } else {
                raw_balance(
                    deps.querier,
                    &astroport_lp_token,
                    env.contract.address.as_bytes(),
                )?
            };
            // Calculate Lockup Astro LP shares
            (lockup_info
                .lp_units_locked
                .full_mul(pool_astroport_lp_units)
                .checked_div(Uint256::from(pool_info.terraswap_amount_in_lockups))?)
            .try_into()?
        };
        lockup_astroport_lp_units_opt = Some(lockup_astroport_lp_units);
        astroport_lp_token_opt = Some(astroport_lp_token.clone());
        // If LP tokens are staked, calculate the rewards claimable by the user for this lockup position
        if pool_info.is_staked && !lockup_astroport_lp_units.is_zero() {
            let generator = config
                .generator
                .clone()
                .ok_or_else(|| StdError::generic_err("Generator should be set at this moment!"))?;

            // QUERY :: Check if there are any pending staking rewards
            let pending_rewards: PendingTokenResponse = deps.querier.query_wasm_smart(
                &generator,
                &GenQueryMsg::PendingToken {
                    lp_token: astroport_lp_token.to_string(),
                    user: env.contract.address.to_string(),
                },
            )?;

            // Calculate claimable Astro staking rewards for this lockup
            pool_info.generator_ntrn_per_share +=
                Decimal::from_ratio(pending_rewards.pending, pool_astroport_lp_units);

            let total_lockup_astro_rewards =
                pool_info.generator_ntrn_per_share * lockup_astroport_lp_units;
            claimable_generator_astro_debt =
                total_lockup_astro_rewards - lockup_info.generator_ntrn_debt;

            // Calculate claimable Proxy staking rewards for this lockup
            if let Some(pending_on_proxy) = pending_rewards.pending_on_proxy {
                for reward in pending_on_proxy {
                    let generator_proxy_per_share = pool_info.generator_proxy_per_share.update(
                        &reward.info,
                        Decimal::from_ratio(reward.amount, pool_astroport_lp_units),
                    )?;

                    let debt = generator_proxy_per_share
                        .checked_mul_uint128(lockup_astroport_lp_units)?
                        .checked_sub(
                            lockup_info
                                .generator_proxy_debt
                                .inner_ref()
                                .iter()
                                .find_map(|a| if reward.info == a.0 { Some(a.1) } else { None })
                                .unwrap_or_default(),
                        )?;

                    claimable_generator_proxy_debt.update(&reward.info, debt)?;
                }
            }
        }
    }
    // Calculate currently expected ASTRO Rewards if not finalized
    if lockup_info.ntrn_rewards == Uint128::zero() {
        let weighted_lockup_balance =
            calculate_weight(lockup_info.lp_units_locked, duration, &config)?;
        lockup_info.ntrn_rewards = calculate_astro_incentives_for_lockup(
            weighted_lockup_balance,
            pool_info.weighted_amount,
            pool_info.incentives_share,
            state.total_incentives_share,
            config.lockdrop_incentives,
        )?;
    }

    Ok(LockUpInfoResponse {
        terraswap_lp_token,
        lp_units_locked: lockup_info.lp_units_locked,
        withdrawal_flag: lockup_info.withdrawal_flag,
        astro_rewards: lockup_info.ntrn_rewards,
        generator_astro_debt: lockup_info.generator_ntrn_debt,
        claimable_generator_astro_debt,
        generator_proxy_debt: lockup_info.generator_proxy_debt,
        claimable_generator_proxy_debt,
        unlock_timestamp: lockup_info.unlock_timestamp,
        astroport_lp_units: lockup_astroport_lp_units_opt,
        astroport_lp_token: astroport_lp_token_opt,
        astroport_lp_transferred: lockup_info.astroport_lp_transferred,
        duration,
    })
}

/// Calculates maximum % of LP balances deposited that can be withdrawn
/// ## Params
/// * **current_timestamp** is an object of type [`u64`]. Current block timestamp
///
/// * **config** is an object of type [`Config`]. Contract configuration
fn calculate_max_withdrawal_percent_allowed(current_timestamp: u64, config: &Config) -> Decimal {
    let withdrawal_cutoff_init_point = config.init_timestamp + config.deposit_window;

    // Deposit window :: 100% withdrawals allowed
    if current_timestamp < withdrawal_cutoff_init_point {
        return Decimal::from_ratio(100u32, 100u32);
    }

    let withdrawal_cutoff_second_point =
        withdrawal_cutoff_init_point + (config.withdrawal_window / 2u64);
    // Deposit window closed, 1st half of withdrawal window :: 50% withdrawals allowed
    if current_timestamp <= withdrawal_cutoff_second_point {
        return Decimal::from_ratio(50u32, 100u32);
    }

    // max withdrawal allowed decreasing linearly from 50% to 0% vs time elapsed
    let withdrawal_cutoff_final = withdrawal_cutoff_init_point + config.withdrawal_window;
    //  Deposit window closed, 2nd half of withdrawal window :: max withdrawal allowed decreases linearly from 50% to 0% vs time elapsed
    if current_timestamp < withdrawal_cutoff_final {
        let time_left = withdrawal_cutoff_final - current_timestamp;
        Decimal::from_ratio(
            50u64 * time_left,
            100u64 * (withdrawal_cutoff_final - withdrawal_cutoff_second_point),
        )
    }
    // Withdrawals not allowed
    else {
        Decimal::from_ratio(0u32, 100u32)
    }
}

/// Calculates ASTRO rewards for a particular Lockup position
/// ## Params
/// * **lockup_weighted_balance** is an object of type [`Uint256`]. Lockup position's weighted terraswap LP balance
///
/// * **total_weighted_amount** is an object of type [`Uint256`]. Total weighted terraswap LP balance of the Pool
///
/// * **pool_incentives_share** is an object of type [`u64`]. Share of total ASTRO incentives allocated to this pool
///
/// * **total_incentives_share** is an object of type [`u64`]. Calculated total incentives share for allocating among pools
///
/// * **total_lockdrop_incentives** is an object of type [`Uint128`]. Total ASTRO incentives to be distributed among Lockdrop participants
pub fn calculate_astro_incentives_for_lockup(
    lockup_weighted_balance: Uint256,
    total_weighted_amount: Uint256,
    pool_incentives_share: u64,
    total_incentives_share: u64,
    total_lockdrop_incentives: Uint128,
) -> StdResult<Uint128> {
    if total_incentives_share == 0u64 || total_weighted_amount == Uint256::zero() {
        Ok(Uint128::zero())
    } else {
        Ok(Decimal256::from_ratio(
            Uint256::from(pool_incentives_share) * lockup_weighted_balance,
            Uint256::from(total_incentives_share) * total_weighted_amount,
        )
        .checked_mul_uint256(total_lockdrop_incentives.into())?)
    }
}

/// Returns effective weight for the amount to be used for calculating lockdrop rewards.
/// ## Params
/// * **amount** is an object of type [`Uint128`]. Number of LP tokens.
///
/// * **duration** is an object of type [`u64`]. Number of weeks.
///
/// * **config** is an object of type [`Config`]. Config with weekly multiplier and divider.
fn calculate_weight(amount: Uint128, duration: u64, config: &Config) -> StdResult<Uint256> {
    let lock_weight = Decimal256::one()
        + Decimal256::from_ratio(
            (duration - 1) * config.weekly_multiplier,
            config.weekly_divider,
        );
    Ok(lock_weight.checked_mul_uint256(amount.into())?.into())
}

/// Calculates ASTRO rewards for each of the user position.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **config** is an object of type [`Config`].
///
/// * **state** is an object of type [`State`].
///
/// * **user_address** is an object of type [`Addr`]
fn update_user_lockup_positions_and_calc_rewards(
    deps: DepsMut,
    config: &Config,
    state: &State,
    user_address: &Addr,
) -> StdResult<Uint128> {
    let mut total_astro_rewards = Uint128::zero();

    let mut keys: Vec<(Addr, u64)> = vec![];

    for pool_key in ASSET_POOLS
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<Addr>, StdError>>()?
    {
        for duration in LOCKUP_INFO
            .prefix((&pool_key, user_address))
            .keys(deps.storage, None, None, Order::Ascending)
            .collect::<Result<Vec<u64>, StdError>>()?
        {
            keys.push((pool_key.clone(), duration));
        }
    }
    for (pool, duration) in keys {
        let pool_info = ASSET_POOLS.load(deps.storage, &pool)?;
        let lockup_key = (&pool, user_address, U64Key::new(duration));
        let mut lockup_info =
            LOCKUP_INFO.compatible_load(deps.as_ref(), lockup_key.clone(), &config.generator)?;

        if lockup_info.ntrn_rewards == Uint128::zero() {
            // Weighted lockup balance (using terraswap LP units to calculate as pool's total weighted balance is calculated on terraswap LP deposits summed over each deposit tx)
            let weighted_lockup_balance =
                calculate_weight(lockup_info.lp_units_locked, duration, config)?;

            // Calculate ASTRO Lockdrop rewards for the lockup position
            lockup_info.ntrn_rewards = calculate_astro_incentives_for_lockup(
                weighted_lockup_balance,
                pool_info.weighted_amount,
                pool_info.incentives_share,
                state.total_incentives_share,
                config.lockdrop_incentives,
            )?;

            LOCKUP_INFO.save(deps.storage, lockup_key, &lockup_info)?;
        };

        let lockup_astro_rewards = lockup_info.ntrn_rewards;

        // Save updated Lockup state
        total_astro_rewards = total_astro_rewards.checked_add(lockup_astro_rewards)?;
    }

    Ok(total_astro_rewards)
}
