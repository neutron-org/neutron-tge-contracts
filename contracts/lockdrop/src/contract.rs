use std::cmp::min;
use std::collections::HashMap;
use std::convert::TryInto;
use std::str::FromStr;

use astroport::asset::{Asset, AssetInfo};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::cosmwasm_ext::IntegerToDecimal;
use astroport::generator::{
    ExecuteMsg as GenExecuteMsg, PendingTokenResponse, QueryMsg as GenQueryMsg, RewardInfoResponse,
};
use astroport::restricted_vector::RestrictedVector;
use astroport::DecimalCheckedOps;
use astroport_periphery::utils::Decimal256CheckedOps;
use cosmwasm_std::{
    attr, coins, entry_point, from_binary, to_binary, Addr, BankMsg, Binary, Coin, CosmosMsg,
    Decimal, Decimal256, Deps, DepsMut, Env, MessageInfo, Order, Response, StdError, StdResult,
    Uint128, Uint256, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg};

use crate::raw_queries::{raw_balance, raw_generator_deposit};
use astroport_periphery::lockdrop::{
    CallbackMsg, Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, LockUpInfoResponse,
    LockUpInfoSummary, LockupInfoV2, MigrateExecuteMsg, MigrateMsg, MigrationState, PoolInfo,
    PoolInfoV2, PoolType, QueryMsg, State, StateResponse, UpdateConfigMsg, UserInfoResponse,
    UserInfoWithListResponse,
};

use crate::state::{
    CompatibleLoader, ASSET_POOLS, ASSET_POOLS_V2, CONFIG, LOCKUP_INFO, MIGRATION_MAX_SLIPPAGE,
    MIGRATION_STATUS, MIGRATION_USERS_COUNTER, MIGRATION_USERS_DEFAULT_LIMIT, OWNERSHIP_PROPOSAL,
    STATE, TOTAL_USER_LOCKUP_AMOUNT, USER_INFO,
};

const AIRDROP_REWARDS_MULTIPLIER: &str = "1.0";

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

    if msg.lockup_rewards_info.is_empty() {
        return Err(StdError::generic_err("Invalid lockup rewards info"));
    }
    for lr_info in &msg.lockup_rewards_info {
        if lr_info.duration == 0 {
            return Err(StdError::generic_err(
                "Invalid Lockup info rewards duration",
            ));
        }
    }

    if msg.max_positions_per_user < MIN_POSITIONS_PER_USER {
        return Err(StdError::generic_err(
            "The maximum number of locked positions per user cannot be lower than a minimum acceptable value."
        ));
    }

    let config = Config {
        owner: msg
            .owner
            .map(|v| deps.api.addr_validate(&v))
            .transpose()?
            .unwrap_or(info.sender),
        token_info_manager: deps.api.addr_validate(&msg.token_info_manager)?,
        credits_contract: deps.api.addr_validate(&msg.credits_contract)?,
        auction_contract: deps.api.addr_validate(&msg.auction_contract)?,
        generator: None,
        init_timestamp: msg.init_timestamp,
        lock_window: msg.lock_window,
        withdrawal_window: msg.withdrawal_window,
        min_lock_duration: msg.min_lock_duration,
        max_lock_duration: msg.max_lock_duration,
        lockdrop_incentives: Uint128::zero(),
        max_positions_per_user: msg.max_positions_per_user,
        lockup_rewards_info: msg.lockup_rewards_info,
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
///     pool_type,
///     incentives_share,
/// }** Facilitates addition of new Pool (axlrUSDC/NTRN or ATOM/NTRN) whose LP tokens can then be locked in the lockdrop contract.
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
    let migration_state: MigrationState = MIGRATION_STATUS.load(deps.storage)?;
    if migration_state != MigrationState::Completed {
        match msg {
            ExecuteMsg::MigrateFromXykToCl(..) => {}
            ExecuteMsg::Callback(..) => {}
            _ => {
                return Err(StdError::generic_err(
                    "Contract is in migration state. Please wait for migration to complete.",
                ))
            }
        }
    }
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::ClaimRewardsAndOptionallyUnlock {
            pool_type,
            duration,
            withdraw_lp_stake,
        } => handle_claim_rewards_and_unlock_for_lockup(
            deps,
            env,
            info,
            pool_type,
            duration,
            withdraw_lp_stake,
        ),
        ExecuteMsg::Callback(msg) => _handle_callback(deps, env, info, msg),
        ExecuteMsg::MigrateFromXykToCl(msg) => _handle_migrate(deps, env, info, msg),
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
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;
            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;
                Ok(())
            })
        }
        ExecuteMsg::IncreaseNTRNIncentives {} => handle_increasing_ntrn_incentives(deps, env, info),
        ExecuteMsg::IncreaseLockupFor {
            user_address,
            pool_type,
            amount,
            duration,
        } => handle_increase_lockup(deps, env, info, user_address, pool_type, duration, amount),
        ExecuteMsg::WithdrawFromLockup {
            user_address,
            pool_type,
            duration,
            amount,
        } => {
            handle_withdraw_from_lockup(deps, env, info, user_address, pool_type, duration, amount)
        }
        ExecuteMsg::UpdateConfig { new_config } => handle_update_config(deps, info, new_config),
        ExecuteMsg::SetTokenInfo {
            usdc_token,
            atom_token,
            generator,
        } => handle_set_token_info(deps, env, info, usdc_token, atom_token, generator),
    }
}

fn _handle_migrate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: MigrateExecuteMsg,
) -> StdResult<Response> {
    match msg {
        MigrateExecuteMsg::MigrateLiquidity { slippage_tolerance } => {
            migrate_liquidity(deps, env, info, slippage_tolerance)
        }
        MigrateExecuteMsg::MigrateUsers { limit } => migrate_users(deps, env, info, limit),
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
    let cw20_sender_addr = deps.api.addr_validate(&cw20_msg.sender)?;
    // CHECK :: Tokens sent > 0
    if cw20_msg.amount == Uint128::zero() {
        return Err(StdError::generic_err(
            "Number of tokens sent should be > 0 ",
        ));
    }

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::InitializePool {
            pool_type,
            incentives_share,
        } => handle_initialize_pool(
            deps,
            env,
            info,
            pool_type,
            cw20_sender_addr,
            incentives_share,
            cw20_msg.amount,
        ),
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
            pool_type,
            prev_ntrn_balance,
            prev_proxy_reward_balances,
        } => update_pool_on_dual_rewards_claim(
            deps,
            env,
            pool_type,
            prev_ntrn_balance,
            prev_proxy_reward_balances,
        ),
        CallbackMsg::WithdrawUserLockupRewardsCallback {
            pool_type,
            user_address,
            duration,
            withdraw_lp_stake,
        } => callback_withdraw_user_rewards_for_lockup_optional_withdraw(
            deps,
            env,
            pool_type,
            user_address,
            duration,
            withdraw_lp_stake,
        ),
        CallbackMsg::MigratePairStep1 {
            pool_type,
            slippage_tolerance,
        } => migrate_pair_step_1(deps, info, env, pool_type, slippage_tolerance),
        CallbackMsg::MigratePairStep2 {
            pool_type,
            current_ntrn_balance,
            slippage_tolerance,
            reward_amount,
        } => migrate_pair_step_2(
            deps,
            info,
            env,
            pool_type,
            current_ntrn_balance,
            slippage_tolerance,
            reward_amount,
        ),
        CallbackMsg::MigratePairStep3 { pool_type } => {
            migrate_pair_step_3(deps, info, env, pool_type)
        }
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
///
/// * **QueryUserLockupTotalAtHeight {
///         pool_type: PoolType,
///         user_address: String,
///         height: u64,
///     }** Returns locked amount of LP tokens for the specified user for the specified pool at a specific height.
///
/// * **QueryLockupTotalAtHeight {
///         pool_type: PoolType,
///         height: u64,
///     }** Returns a total amount of LP tokens for the specified pool at a specific height.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let migration_state: MigrationState = MIGRATION_STATUS.load(deps.storage)?;
    if migration_state != MigrationState::Completed {
        match msg {
            QueryMsg::QueryUserLockupTotalAtHeight { .. }
            | QueryMsg::QueryLockupTotalAtHeight { .. } => {
                return Err(StdError::generic_err(
                    "Contract is in migration state. Please wait for migration to complete.",
                ))
            }
            _ => {}
        }
    }
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::Pool { pool_type } => to_binary(&query_pool(deps, pool_type)?),
        QueryMsg::UserInfo { address } => to_binary(&query_user_info(deps, env, address)?),
        QueryMsg::UserInfoWithLockupsList { address } => {
            to_binary(&query_user_info_with_lockups_list(deps, env, address)?)
        }
        QueryMsg::LockUpInfo {
            user_address,
            pool_type,
            duration,
        } => to_binary(&query_lockup_info(
            deps,
            &env,
            &user_address,
            pool_type,
            duration,
        )?),
        QueryMsg::QueryUserLockupTotalAtHeight {
            pool_type,
            user_address,
            height,
        } => to_binary(&query_user_lockup_total_at_height(
            deps,
            pool_type,
            deps.api.addr_validate(&user_address)?,
            height,
        )?),
        QueryMsg::QueryLockupTotalAtHeight { pool_type, height } => {
            to_binary(&query_lockup_total_at_height(deps, pool_type, height)?)
        }
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
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> StdResult<Response> {
    let mut attrs = vec![attr("action", "migrate")];

    ASSET_POOLS_V2.save(
        deps.storage,
        PoolType::ATOM,
        &PoolInfoV2 {
            lp_token: deps.api.addr_validate(&msg.new_atom_token)?,
            amount_in_lockups: Uint128::zero(),
        },
    )?;
    ASSET_POOLS_V2.save(
        deps.storage,
        PoolType::USDC,
        &PoolInfoV2 {
            lp_token: deps.api.addr_validate(&msg.new_usdc_token)?,
            amount_in_lockups: Uint128::zero(),
        },
    )?;
    MIGRATION_MAX_SLIPPAGE.save(deps.storage, &msg.max_slippage)?;

    attrs.push(attr("new_atom_token", msg.new_atom_token));
    attrs.push(attr("new_usdc_token", msg.new_usdc_token));

    MIGRATION_STATUS.save(deps.storage, &MigrationState::MigrateLiquidity)?;

    Ok(Response::default().add_attributes(attrs))
}

fn migrate_liquidity(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    slippage_tolerance: Option<Decimal>,
) -> StdResult<Response> {
    let migration_state = MIGRATION_STATUS.load(deps.storage)?;

    if migration_state != MigrationState::MigrateLiquidity {
        return Err(StdError::generic_err(
            "Migration is not in the correct state",
        ));
    }

    let max_slippage = MIGRATION_MAX_SLIPPAGE.load(deps.storage)?;
    if slippage_tolerance.unwrap_or_default() > max_slippage {
        return Err(StdError::generic_err("Slippage tolerance is too high"));
    }

    let attrs = vec![attr("action", "migrate_liquidity")];
    let msgs = vec![
        CallbackMsg::MigratePairStep1 {
            pool_type: PoolType::ATOM,
            slippage_tolerance,
        }
        .to_cosmos_msg(&env)?,
        CallbackMsg::MigratePairStep1 {
            pool_type: PoolType::USDC,
            slippage_tolerance,
        }
        .to_cosmos_msg(&env)?,
    ];
    Ok(Response::new().add_messages(msgs).add_attributes(attrs))
}

fn get_lp_token_pool_addr(deps: Deps, lp_token_addr: &Addr) -> StdResult<String> {
    let minter_response: cw20::MinterResponse = deps
        .querier
        .query_wasm_smart(lp_token_addr.to_string(), &cw20::Cw20QueryMsg::Minter {})?;
    Ok(minter_response.minter)
}

fn get_reward_amount(
    deps: Deps,
    env: &Env,
    astroport_lp_token: &String,
    generator: &String,
) -> StdResult<Uint128> {
    let rwi: RewardInfoResponse = deps.querier.query_wasm_smart(
        generator,
        &GenQueryMsg::RewardInfo {
            lp_token: astroport_lp_token.to_string(),
        },
    )?;

    let reward_token_balance = deps
        .querier
        .query_balance(
            env.contract.address.clone(),
            rwi.base_reward_token.to_string(),
        )?
        .amount;

    Ok(reward_token_balance)
}

fn migrate_pair_step_1(
    deps: DepsMut,
    _info: MessageInfo,
    env: Env,
    pool_type: PoolType,
    slippage_tolerance: Option<Decimal>,
) -> StdResult<Response> {
    let config: Config = CONFIG.load(deps.storage)?;
    let generator = config
        .generator
        .as_ref()
        .ok_or_else(|| StdError::generic_err("Generator address hasn't set yet!"))?;

    //get current ntrn contract balance
    let current_ntrn_balance = deps
        .querier
        .query_balance(&env.contract.address, UNTRN_DENOM)?
        .amount;

    let mut attrs = vec![
        attr("action", "migrate_pair_step_1"),
        attr("pool_type", pool_type),
    ];
    let mut msgs = vec![];
    let pool: PoolInfo = ASSET_POOLS.load(deps.storage, pool_type)?;
    let pool_addr = get_lp_token_pool_addr(deps.as_ref(), &pool.lp_token)?;

    let reward_amount = get_reward_amount(
        deps.as_ref(),
        &env,
        &pool.lp_token.to_string(),
        &generator.to_string(),
    )?;

    //unstake from generator
    attrs.push(attr(
        "unstake_from_generator",
        pool.amount_in_lockups.to_string(),
    ));
    msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: generator.to_string(),
        funds: vec![],
        msg: to_binary(&astroport::generator::ExecuteMsg::Withdraw {
            lp_token: pool.lp_token.to_string(),
            amount: pool.amount_in_lockups,
        })?,
    }));

    //withdraw lp tokens from pool
    msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pool.lp_token.to_string(),
        funds: vec![],
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: pool_addr,
            amount: pool.amount_in_lockups,
            msg: to_binary(&astroport::pair::Cw20HookMsg::WithdrawLiquidity { assets: vec![] })?,
        })?,
    }));
    attrs.push(attr(
        "withdraw_from_pool_amount",
        pool.amount_in_lockups.to_string(),
    ));

    //next step
    msgs.push(
        CallbackMsg::MigratePairStep2 {
            pool_type,
            current_ntrn_balance,
            slippage_tolerance,
            reward_amount,
        }
        .to_cosmos_msg(&env)?,
    );

    Ok(Response::new().add_messages(msgs).add_attributes(attrs))
}

fn migrate_pair_step_2(
    deps: DepsMut,
    _info: MessageInfo,
    env: Env,
    pool_type: PoolType,
    prev_ntrn_balance: Uint128,
    slippage_tolerance: Option<Decimal>,
    prev_reward_amount: Uint128,
) -> StdResult<Response> {
    let config: Config = CONFIG.load(deps.storage)?;
    let generator = config
        .generator
        .as_ref()
        .ok_or_else(|| StdError::generic_err("Generator address hasn't set yet!"))?;
    let mut attrs = vec![
        attr("action", "migrate_pair_step_2"),
        attr("pool_type", pool_type),
    ];
    let mut pool_info: PoolInfo = ASSET_POOLS.load(deps.storage, pool_type)?;
    let new_pool_info: PoolInfoV2 = ASSET_POOLS_V2.load(deps.storage, pool_type)?;
    let new_pool_addr = get_lp_token_pool_addr(deps.as_ref(), &new_pool_info.lp_token)?;
    let mut msgs = vec![];
    let current_ntrn_balance = deps
        .querier
        .query_balance(&env.contract.address, UNTRN_DENOM)?
        .amount;
    attrs.push(attr(
        "ntrn_balance_change",
        (current_ntrn_balance - prev_ntrn_balance).to_string(),
    ));

    pool_info.generator_ntrn_per_share = pool_info.generator_ntrn_per_share.checked_add({
        let reward_token_balance = get_reward_amount(
            deps.as_ref(),
            &env,
            &pool_info.lp_token.to_string(),
            &generator.to_string(),
        )?;
        attrs.push(attr(
            "reward_token_balance",
            reward_token_balance.to_string(),
        ));
        let base_reward_received = reward_token_balance.checked_sub(prev_reward_amount)?;
        Decimal::from_ratio(base_reward_received, pool_info.amount_in_lockups)
    })?;
    attrs.push(attr(
        "generator_ntrn_per_share",
        pool_info.generator_ntrn_per_share.to_string(),
    ));
    ASSET_POOLS.save(deps.storage, pool_type, &pool_info, env.block.height)?;

    let ntrn_to_new_pool = current_ntrn_balance - prev_ntrn_balance;
    let new_pool_info: astroport::pair::PoolResponse = deps
        .querier
        .query_wasm_smart(&new_pool_addr, &astroport::pair::QueryMsg::Pool {})?;
    let token_denom = new_pool_info
        .assets
        .iter()
        .find_map(|x| match &x.info {
            AssetInfo::NativeToken { denom } if denom != UNTRN_DENOM => Some(denom.clone()),
            _ => None,
        })
        .ok_or_else(|| StdError::generic_err("No second leg of pair found"))?;
    attrs.push(attr("token_denom", token_denom.clone()));
    let token_balance = deps
        .querier
        .query_balance(&env.contract.address, token_denom.as_str())?
        .amount;
    attrs.push(attr("token_balance", token_balance.to_string()));

    let base = Asset {
        amount: ntrn_to_new_pool,
        info: AssetInfo::NativeToken {
            denom: UNTRN_DENOM.to_string(),
        },
    };
    let other = Asset {
        amount: token_balance,
        info: AssetInfo::NativeToken {
            denom: token_denom.to_string(),
        },
    };
    let mut funds = vec![
        Coin {
            denom: UNTRN_DENOM.to_string(),
            amount: ntrn_to_new_pool,
        },
        Coin {
            denom: token_denom,
            amount: token_balance,
        },
    ];

    funds.sort_by(|a, b| a.denom.cmp(&b.denom));
    msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: new_pool_addr,
        funds,
        msg: to_binary(&astroport::pair::ExecuteMsg::ProvideLiquidity {
            assets: vec![base, other],
            slippage_tolerance,
            auto_stake: None,
            receiver: None,
        })?,
    }));

    attrs.push(attr("provide_liquidity", "true"));

    msgs.push(CallbackMsg::MigratePairStep3 { pool_type }.to_cosmos_msg(&env)?);

    Ok(Response::new().add_messages(msgs).add_attributes(attrs))
}

fn migrate_pair_step_3(
    deps: DepsMut,
    _info: MessageInfo,
    env: Env,
    pool_type: PoolType,
) -> StdResult<Response> {
    let config: Config = CONFIG.load(deps.storage)?;
    let mut attrs = vec![
        attr("action", "migrate_pair_step_3"),
        attr("pool_type", pool_type),
    ];
    let mut new_pool_info: PoolInfoV2 = ASSET_POOLS_V2.load(deps.storage, pool_type)?;
    // get current balance
    let balance_response: cw20::BalanceResponse = deps.querier.query_wasm_smart(
        &new_pool_info.lp_token,
        &Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;
    let current_balance = balance_response.balance;
    attrs.push(attr("current_balance", current_balance.to_string()));

    //update pool info
    new_pool_info.amount_in_lockups = current_balance;
    ASSET_POOLS_V2.save(deps.storage, pool_type, &new_pool_info)?;

    // send stake message to generator
    let stake_msgs = stake_messages(
        config,
        env.block.height + 1u64,
        new_pool_info.lp_token,
        current_balance,
    )?;

    MIGRATION_STATUS.save(deps.storage, &MigrationState::MigrateUsers(0u64))?;

    Ok(Response::new()
        .add_messages(stake_msgs)
        .add_attributes(attrs))
}

fn migrate_users(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    limit: Option<u32>,
) -> StdResult<Response> {
    let mut attrs = vec![attr("action", "migrate_users")];
    let migrate_state: MigrationState = MIGRATION_STATUS.load(deps.storage)?;
    let limit = limit.unwrap_or(MIGRATION_USERS_DEFAULT_LIMIT);
    let current_skip = MIGRATION_USERS_COUNTER
        .may_load(deps.storage)?
        .unwrap_or(0u32);

    match migrate_state {
        MigrationState::MigrateUsers(page) => {
            let pool_types: Vec<PoolType> = ASSET_POOLS
                .keys(deps.storage, None, None, Order::Ascending)
                .collect::<Result<Vec<PoolType>, StdError>>()?;

            let mut kfs: HashMap<String, Decimal256> = HashMap::new();
            for pool_type in &pool_types {
                let pool_info: PoolInfo = ASSET_POOLS.load(deps.storage, *pool_type)?;
                let new_pool_info: PoolInfoV2 = ASSET_POOLS_V2.load(deps.storage, *pool_type)?;
                kfs.insert(
                    (*pool_type).into(),
                    Decimal256::from_ratio(
                        new_pool_info.amount_in_lockups,
                        pool_info.amount_in_lockups,
                    ),
                );
            }

            let users: Vec<Addr> = USER_INFO
                .keys(deps.storage, None, None, Order::Ascending)
                .skip(current_skip as usize)
                .take(limit as usize)
                .collect::<Result<Vec<_>, _>>()?;
            if users.is_empty() {
                for pool_type in &pool_types {
                    let mut pool_info: PoolInfo = ASSET_POOLS.load(deps.storage, *pool_type)?;
                    let new_pool_info: PoolInfoV2 =
                        ASSET_POOLS_V2.load(deps.storage, *pool_type)?;
                    pool_info.amount_in_lockups = new_pool_info.amount_in_lockups;
                    pool_info.lp_token = new_pool_info.lp_token;
                    ASSET_POOLS.save(deps.storage, *pool_type, &pool_info, env.block.height)?;
                }
                MIGRATION_STATUS.save(deps.storage, &MigrationState::Completed)?;
                attrs.push(attr("migration_completed", "true"));
            } else {
                attrs.push(attr("users_count", users.len().to_string()));
                //iterate over users
                for user in users {
                    for pool_type in &pool_types {
                        let mut total_lokups = Uint128::zero();
                        let lookup_infos: Vec<(u64, LockupInfoV2)> = LOCKUP_INFO
                            .prefix((*pool_type, &user))
                            .range(deps.storage, None, None, Order::Ascending)
                            .collect::<Result<Vec<(u64, LockupInfoV2)>, StdError>>()?;
                        for (duration, mut lockup_info) in lookup_infos {
                            let info = query_lockup_info(
                                deps.as_ref(),
                                &env.clone(),
                                user.as_ref(),
                                *pool_type,
                                duration,
                            )?;
                            let p: String = (*pool_type).into();
                            let kf = kfs
                                .get(&p)
                                .ok_or_else(|| StdError::generic_err("Can't get kf"))?;
                            lockup_info.generator_proxy_debt = info.generator_proxy_debt;
                            lockup_info.lp_units_locked =
                                (*kf).checked_mul_uint256(lockup_info.lp_units_locked.into())?;
                            LOCKUP_INFO.save(
                                deps.storage,
                                (*pool_type, &user, duration),
                                &lockup_info,
                            )?;
                            total_lokups += lockup_info.lp_units_locked;
                        }
                        TOTAL_USER_LOCKUP_AMOUNT.update(
                            deps.storage,
                            (*pool_type, &user),
                            env.block.height,
                            |lockup_amount| -> StdResult<Uint128> {
                                if let Some(la) = lockup_amount {
                                    Ok(la.checked_add(total_lokups)?)
                                } else {
                                    Ok(total_lokups)
                                }
                            },
                        )?;
                    }
                }
                MIGRATION_USERS_COUNTER.save(deps.storage, &(current_skip + limit as u32))?;
                MIGRATION_STATUS.save(deps.storage, &MigrationState::MigrateUsers(page + 1u64))?;
            }

            Ok(Response::default().add_attributes(attrs))
        }
        _ => Err(StdError::generic_err(
            "Migration is not in MigrateUsers state",
        )),
    }
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
        config.auction_contract = deps.api.addr_validate(&auction)?;
        attributes.push(attr("auction_contract", auction));
    };

    if let Some(generator) = new_config.generator_address {
        // If generator is set, we check is any LP tokens are currently staked before updating generator address
        if config.generator.is_some() {
            for pool_type in ASSET_POOLS
                .keys(deps.storage, None, None, Order::Ascending)
                .collect::<Result<Vec<PoolType>, StdError>>()?
            {
                let pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;
                if pool_info.is_staked {
                    return Err(StdError::generic_err(format!(
                        "{:?} astro LP tokens already staked. Unstake them before updating generator",
                        pool_type
                    )));
                }
            }
        }

        config.generator = Some(deps.api.addr_validate(&generator)?);
        attributes.push(attr("new_generator", generator))
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attributes(attributes))
}

pub fn handle_set_token_info(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    usdc_token: String,
    atom_token: String,
    generator: String,
) -> Result<Response, StdError> {
    let mut config = CONFIG.load(deps.storage)?;

    // CHECK :: Only owner and token info manager can call this function
    if info.sender != config.owner && info.sender != config.token_info_manager {
        return Err(StdError::generic_err("Unauthorized"));
    }

    // POOL INFO :: Initialize new pool
    let pool_info = PoolInfo {
        lp_token: deps.api.addr_validate(&atom_token)?,
        amount_in_lockups: Default::default(),
        incentives_share: Uint128::zero(),
        weighted_amount: Default::default(),
        generator_ntrn_per_share: Default::default(),
        generator_proxy_per_share: RestrictedVector::default(),
        is_staked: false,
    };
    ASSET_POOLS.save(deps.storage, PoolType::ATOM, &pool_info, env.block.height)?;

    // POOL INFO :: Initialize new pool
    let pool_info = PoolInfo {
        lp_token: deps.api.addr_validate(&usdc_token)?,
        amount_in_lockups: Default::default(),
        incentives_share: Uint128::zero(),
        weighted_amount: Default::default(),
        generator_ntrn_per_share: Default::default(),
        generator_proxy_per_share: RestrictedVector::default(),
        is_staked: false,
    };
    ASSET_POOLS.save(deps.storage, PoolType::USDC, &pool_info, env.block.height)?;

    config.generator = Some(deps.api.addr_validate(&generator)?);
    CONFIG.save(deps.storage, &config)?;

    let attributes = vec![
        attr("action", "update_config"),
        attr("usdc_token", usdc_token),
        attr("atom_token", atom_token),
        attr("generator", generator),
    ];

    Ok(Response::new().add_attributes(attributes))
}

/// Facilitates increasing NTRN incentives that are to be distributed as Lockdrop participation reward. Returns a default object of type [`Response`].
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
        >= config.init_timestamp + config.lock_window + config.withdrawal_window
    {
        return Err(StdError::generic_err("Lock window is closed"));
    };

    let incentive = info.funds.iter().find(|c| c.denom == UNTRN_DENOM);
    let amount = if let Some(coin) = incentive {
        coin.amount
    } else {
        return Err(StdError::generic_err(format!(
            "{} is not found",
            UNTRN_DENOM
        )));
    };
    // Anyone can increase ntrn incentives
    config.lockdrop_incentives = config.lockdrop_incentives.checked_add(amount)?;

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new()
        .add_attribute("action", "ntrn_incentives_increased")
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
/// * **pool_type** is an object of type [`PoolType`]. LiquidPool type - USDC or ATOM
///
/// * **cw20_sender_addr** is an object of type [`Addr`]. Address caller cw20 contract
///
/// * **incentives_share** is an object of type [`u64`]. Parameter defining share of total NTRN incentives are allocated for this pool
///
/// * **amount** amount of LP tokens of `pool_type` to be staked in the Generator Contract.
pub fn handle_initialize_pool(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pool_type: PoolType,
    cw20_sender_addr: Addr,
    incentives_share: Uint128,
    amount: Uint128,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    // CHECK ::: Only auction can call this function
    if cw20_sender_addr != config.auction_contract {
        return Err(StdError::generic_err("Unauthorized"));
    }

    // CHECK ::: Is LP Token Pool initialized
    let mut pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;

    if info.sender != pool_info.lp_token {
        return Err(StdError::generic_err("Unknown cw20 token address"));
    }

    // Set Pool Incentives
    pool_info.incentives_share = incentives_share;

    let stake_msgs = stake_messages(
        config,
        env.block.height + 1u64,
        pool_info.lp_token.clone(),
        amount,
    )?;
    pool_info.is_staked = true;

    ASSET_POOLS.save(deps.storage, pool_type, &pool_info, env.block.height)?;

    state.total_incentives_share = state.total_incentives_share.checked_add(incentives_share)?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_messages(stake_msgs)
        .add_attributes(vec![
            attr("action", "initialize_pool"),
            attr("pool_type", format!("{:?}", pool_type)),
            attr("lp_token", info.sender),
            attr("lp_amount", amount),
            attr("incentives_share", incentives_share.to_string()),
        ]))
}

fn stake_messages(
    config: Config,
    height: u64,
    lp_token_address: Addr,
    amount: Uint128,
) -> StdResult<Vec<CosmosMsg>> {
    let mut cosmos_msgs = vec![];

    let generator = config
        .generator
        .as_ref()
        .ok_or_else(|| StdError::generic_err("Generator address hasn't set yet!"))?;

    // TODO: why do we need allowance here, when next message is "send" to a pool
    cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: lp_token_address.to_string(),
        funds: vec![],
        msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
            spender: generator.to_string(),
            amount,
            expires: Some(cw20::Expiration::AtHeight(height)),
        })?,
    }));

    cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: lp_token_address.to_string(),
        funds: vec![],
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: generator.to_string(),
            msg: to_binary(&astroport::generator::Cw20HookMsg::Deposit {})?,
            amount,
        })?,
    }));

    Ok(cosmos_msgs)
}

/// Hook function to increase Lockup position size when any of the supported LP Tokens are sent to the contract by the user. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **user_address_raw** is an object of type [`Addr`]. User we increase lockup position for
///
/// * **pool_type** is an object of type [`PoolType`]. LiquidPool type - USDC or ATOM
///
/// * **duration** is an object of type [`u64`]. Number of seconds the LP token is locked for (lockup period begins post the withdrawal window closure).
///
/// * **amount** is an object of type [`Uint128`]. Number of LP tokens sent by the user.
pub fn handle_increase_lockup(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    user_address_raw: String,
    pool_type: PoolType,
    duration: u64,
    amount: Uint128,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.auction_contract {
        return Err(StdError::generic_err("Unauthorized"));
    }

    if env.block.time.seconds() >= config.init_timestamp + config.lock_window {
        return Err(StdError::generic_err("Lock window is closed"));
    };

    let user_address = deps.api.addr_validate(&user_address_raw)?;

    if !config
        .lockup_rewards_info
        .iter()
        .any(|i| i.duration == duration)
    {
        return Err(StdError::generic_err("invalid duration"));
    }

    // CHECK ::: LP Token supported or not ?
    let mut pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;
    let mut user_info = USER_INFO
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    // CHECK :: Valid Lockup Duration
    if duration > config.max_lock_duration || duration < config.min_lock_duration {
        return Err(StdError::generic_err(format!(
            "Lockup duration needs to be between {} and {}",
            config.min_lock_duration, config.max_lock_duration
        )));
    }

    pool_info.weighted_amount = pool_info
        .weighted_amount
        .checked_add(calculate_weight(amount, duration, &config)?)?;
    pool_info.amount_in_lockups = pool_info.amount_in_lockups.checked_add(amount)?;

    let lockup_key = (pool_type, &user_address, duration);

    let lockup_info =
        match LOCKUP_INFO.compatible_may_load(deps.as_ref(), lockup_key, &config.generator)? {
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
                        + config.lock_window
                        + duration
                        + config.withdrawal_window,
                    generator_ntrn_debt: Uint128::zero(),
                    generator_proxy_debt: Default::default(),
                    withdrawal_flag: false,
                }
            }
        };

    // SAVE UPDATED STATE
    LOCKUP_INFO.save(deps.storage, lockup_key, &lockup_info)?;

    TOTAL_USER_LOCKUP_AMOUNT.update(
        deps.storage,
        (pool_type, &user_address),
        env.block.height,
        |lockup_amount| -> StdResult<Uint128> {
            if let Some(la) = lockup_amount {
                Ok(la.checked_add(amount)?)
            } else {
                Ok(amount)
            }
        },
    )?;

    ASSET_POOLS.save(deps.storage, pool_type, &pool_info, env.block.height)?;
    USER_INFO.save(deps.storage, &user_address, &user_info)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "increase_lockup_position"),
        attr("pool_type", format!("{:?}", pool_type)),
        attr("user", user_address),
        attr("duration", duration.to_string()),
        attr("amount", amount),
    ]))
}

/// Withdraws LP Tokens from an existing Lockup position. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **pool_type** is an object of type [`PoolType`]. LiquidPool type - USDC or ATOM
///
/// * **duration** is an object of type [`u64`]. Duration of the lockup position from which withdrawal is to be made.
///
/// * **amount** is an object of type [`Uint128`]. Number of LP tokens to be withdrawn.
pub fn handle_withdraw_from_lockup(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    user_address: String,
    pool_type: PoolType,
    duration: u64,
    amount: Uint128,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.auction_contract {
        return Err(StdError::generic_err("Unauthorized"));
    }

    if env.block.time.seconds()
        >= config.init_timestamp + config.lock_window + config.withdrawal_window
    {
        return Err(StdError::generic_err("Withdrawal window is closed"));
    };

    // CHECK :: Valid Withdraw Amount
    if amount.is_zero() {
        return Err(StdError::generic_err("Invalid withdrawal request"));
    }

    let mut pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;

    let user_address = deps.api.addr_validate(&user_address)?;

    // Retrieve Lockup position
    let lockup_key = (pool_type, &user_address, duration);
    let mut lockup_info =
        LOCKUP_INFO.compatible_load(deps.as_ref(), lockup_key, &config.generator)?;

    // CHECK :: Has user already withdrawn LP tokens once post the deposit window closure state
    if lockup_info.withdrawal_flag {
        return Err(StdError::generic_err(
            "Withdrawal already happened. No more withdrawals accepted",
        ));
    }

    // Check :: Amount should be within the allowed withdrawal limit bounds
    let max_withdrawal_percent =
        calculate_max_withdrawal_percent_allowed(env.block.time.seconds(), &config);
    let max_withdrawal_allowed = lockup_info
        .lp_units_locked
        .to_decimal()
        .checked_mul(max_withdrawal_percent)?
        .to_uint_floor();
    if amount > max_withdrawal_allowed {
        return Err(StdError::generic_err(format!(
            "Amount exceeds maximum allowed withdrawal limit of {}",
            max_withdrawal_allowed
        )));
    }

    // Update withdrawal flag after the deposit window
    if env.block.time.seconds() >= config.init_timestamp + config.lock_window {
        lockup_info.withdrawal_flag = true;
    }

    // STATE :: RETRIEVE --> UPDATE
    lockup_info.lp_units_locked = lockup_info.lp_units_locked.checked_sub(amount)?;
    pool_info.weighted_amount = pool_info
        .weighted_amount
        .checked_sub(calculate_weight(amount, duration, &config)?)?;
    pool_info.amount_in_lockups = pool_info.amount_in_lockups.checked_sub(amount)?;

    // Remove Lockup position from the list of user positions if Lp_Locked balance == 0
    if lockup_info.lp_units_locked.is_zero() {
        LOCKUP_INFO.remove(deps.storage, lockup_key);
        // decrement number of user's lockup positions
        let mut user_info = USER_INFO
            .may_load(deps.storage, &user_address)?
            .unwrap_or_default();
        user_info.lockup_positions_index -= 1;
        USER_INFO.save(deps.storage, &user_address, &user_info)?;
    } else {
        LOCKUP_INFO.save(deps.storage, lockup_key, &lockup_info)?;
    }
    TOTAL_USER_LOCKUP_AMOUNT.update(
        deps.storage,
        (pool_type, &user_address),
        env.block.height,
        |lockup_amount| -> StdResult<Uint128> {
            if let Some(la) = lockup_amount {
                Ok(la.checked_sub(amount)?)
            } else {
                Ok(Uint128::zero())
            }
        },
    )?;

    // SAVE Updated States
    ASSET_POOLS.save(deps.storage, pool_type, &pool_info, env.block.height)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "withdraw_from_lockup"),
        attr("pool_type", pool_type),
        attr("user_address", user_address),
        attr("duration", duration.to_string()),
        attr("amount", amount),
    ]))
}

/// Calculates maximum % of LP balances deposited that can be withdrawn
/// ## Params
/// * **current_timestamp** is an object of type [`u64`]. Current block timestamp
///
/// * **config** is an object of type [`Config`]. Contract configuration
fn calculate_max_withdrawal_percent_allowed(current_timestamp: u64, config: &Config) -> Decimal {
    let withdrawal_cutoff_init_point = config.init_timestamp + config.lock_window;

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

/// Claims user Rewards for a particular Lockup position. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **pool_type** is an object of type [`PoolType`]. LiquidPool type - USDC or ATOM
///
/// * **duration** is an object of type [`u64`]. Lockup duration (number of weeks).
///
/// * **withdraw_lp_stake** is an object of type [`bool`]. Boolean value indicating if the LP tokens are to be withdrawn or not.
pub fn handle_claim_rewards_and_unlock_for_lockup(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pool_type: PoolType,
    duration: u64,
    withdraw_lp_stake: bool,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;

    if env.block.time.seconds()
        < config.init_timestamp + config.lock_window + config.withdrawal_window
    {
        return Err(StdError::generic_err(
            "Lock/withdrawal window is still open",
        ));
    }

    let user_address = info.sender;

    // CHECK ::: Is LP Token Pool supported or not ?
    let pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;

    let mut user_info = USER_INFO
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    // If user's total NTRN rewards == 0 :: We update all of the user's lockup positions to calculate NTRN rewards and for each alongwith their equivalent Astroport LP Shares
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
    let lockup_key = (pool_type, &user_address, duration);
    let lockup_info = LOCKUP_INFO.compatible_load(deps.as_ref(), lockup_key, &config.generator)?;

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

    let astroport_lp_token = pool_info.lp_token;

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

            let reward_token_balance = deps
                .querier
                .query_balance(
                    env.contract.address.clone(),
                    rwi.base_reward_token.to_string(),
                )?
                .amount;

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
                msg: to_binary(&GenExecuteMsg::ClaimRewards {
                    lp_tokens: vec![astroport_lp_token.to_string()],
                })?,
            }));

            cosmos_msgs.push(
                CallbackMsg::UpdatePoolOnDualRewardsClaim {
                    pool_type,
                    prev_ntrn_balance: reward_token_balance,
                    prev_proxy_reward_balances,
                }
                .to_cosmos_msg(&env)?,
            );
        }
    } else if user_info.ntrn_transferred && !withdraw_lp_stake {
        return Err(StdError::generic_err("No rewards available to claim!"));
    }

    cosmos_msgs.push(
        CallbackMsg::WithdrawUserLockupRewardsCallback {
            pool_type,
            user_address,
            duration,
            withdraw_lp_stake,
        }
        .to_cosmos_msg(&env)?,
    );

    Ok(Response::new().add_messages(cosmos_msgs))
}

/// Claims unvested user's airdrop rewards from the Credits Contract plus part of vested tokens (NTRN Lockdrop rewards amount * AIDROP_REWARDS_MULTIPLIER) Returns a [`CosmosMsg`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **credits_contract** is an object of type [`Addr`]. Address of the Credits Contract.
///
/// * **user_addr** is an object of type [`Addr`]. Address of the user for who claims rewards.
///
/// * **ntrn_lockdrop_rewards** is an object of type [`Addr`]. Amount of Lockdrop rewards in uNTRN.
pub fn claim_airdrop_tokens_with_multiplier_msg(
    deps: Deps,
    credits_contract: Addr,
    user_addr: Addr,
    ntrn_lockdrop_rewards: Uint128,
) -> StdResult<CosmosMsg> {
    // unvested tokens amount
    let unvested_tokens_amount: credits::msg::WithdrawableAmountResponse =
        deps.querier.query_wasm_smart(
            &credits_contract,
            &credits::msg::QueryMsg::WithdrawableAmount {
                address: user_addr.to_string(),
            },
        )?;
    // vested tokens amount
    let vested_tokens_amount: credits::msg::VestedAmountResponse = deps.querier.query_wasm_smart(
        &credits_contract,
        &credits::msg::QueryMsg::VestedAmount {
            address: user_addr.to_string(),
        },
    )?;

    let airdrop_rewards_multiplier = Decimal::from_str(AIRDROP_REWARDS_MULTIPLIER)?;

    // either we claim whole vested amount or NTRN lockdrop rewards
    let claimable_vested_amount = min(
        vested_tokens_amount.amount,
        ntrn_lockdrop_rewards
            .to_decimal()
            .checked_mul(airdrop_rewards_multiplier)?
            .to_uint_floor(),
    );

    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: credits_contract.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::BurnFrom {
            owner: user_addr.to_string(),
            amount: claimable_vested_amount.checked_add(unvested_tokens_amount.amount)?,
        })?,
        funds: vec![],
    }))
}

/// Updates contract state after dual staking rewards are claimed from the generator contract. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **pool_type** is an object of type [`PoolType`]. LiquidPool type - USDC or ATOM
///
/// * **prev_ntrn_balance** is an object of type [`Uint128`]. Contract's NTRN token balance before claim.
///
/// * **prev_proxy_reward_balances** is a vector of type [`Asset`]. Contract's Generator Proxy reward token balance before claim.
pub fn update_pool_on_dual_rewards_claim(
    deps: DepsMut,
    env: Env,
    pool_type: PoolType,
    prev_ntrn_balance: Uint128,
    prev_proxy_reward_balances: Vec<Asset>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;

    let generator = config
        .generator
        .as_ref()
        .ok_or_else(|| StdError::generic_err("Generator hasn't been set yet!"))?;
    let astroport_lp_token = pool_info.lp_token.clone();

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
    // Increment claimed rewards per LP share
    pool_info.generator_ntrn_per_share = pool_info.generator_ntrn_per_share.checked_add({
        let reward_token_balance = deps
            .querier
            .query_balance(
                env.contract.address.clone(),
                rwi.base_reward_token.to_string(),
            )?
            .amount;
        base_reward_received = reward_token_balance.checked_sub(prev_ntrn_balance)?;
        Decimal::from_ratio(base_reward_received, lp_balance)
    })?;

    // Increment claimed Proxy rewards per LP share
    for prev_balance in prev_proxy_reward_balances {
        let current_balance = prev_balance
            .info
            .query_pool(&deps.querier, env.contract.address.clone())?;
        let received_amount = current_balance.checked_sub(prev_balance.amount)?;
        pool_info.generator_proxy_per_share.update(
            &prev_balance.info,
            Decimal::from_ratio(received_amount, lp_balance),
        )?;
    }

    // SAVE UPDATED STATE OF THE POOL
    ASSET_POOLS.save(deps.storage, pool_type, &pool_info, env.block.height)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "update_generator_dual_rewards"),
        attr("pool_type", format!("{:?}", pool_type)),
        attr("NTRN_reward_received", base_reward_received),
        attr(
            "generator_ntrn_per_share",
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
/// * **pool_type** is an object of type [`PoolType`]. LiquidPool type - USDC or ATOM
///
/// * **user_address** is an object of type [`Addr`]. User address who is claiming the rewards / unlocking his lockup position.
///
/// * **duration** is a vector of type [`u64`]. Duration of the lockup for which rewards have been claimed / position unlocked.
///
/// * **withdraw_lp_stake** is an object of type [`bool`]. Boolean value indicating if the ASTRO LP Tokens are to be sent to the user or not.
pub fn callback_withdraw_user_rewards_for_lockup_optional_withdraw(
    deps: DepsMut,
    env: Env,
    pool_type: PoolType,
    user_address: Addr,
    duration: u64,
    withdraw_lp_stake: bool,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;
    let lockup_key = (pool_type, &user_address, duration);
    let mut lockup_info =
        LOCKUP_INFO.compatible_load(deps.as_ref(), lockup_key, &config.generator)?;

    let mut user_info = USER_INFO
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    let mut cosmos_msgs = vec![];
    let mut attributes = vec![
        attr("action", "withdraw_rewards_and_or_unlock"),
        attr("pool_type", format!("{:?}", pool_type)),
        attr("user_address", &user_address),
        attr("duration", duration.to_string()),
    ];

    let astroport_lp_token = pool_info.lp_token.clone();

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
                astroport_lp_token.clone(),
                &Cw20QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                },
            )?;
            res.balance
        };

        (lockup_info
            .lp_units_locked
            .full_mul(balance)
            .checked_div(Uint256::from(pool_info.amount_in_lockups))?)
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

        // Calculate claimable staking rewards for this lockup (ASTRO incentives)
        let total_lockup_astro_rewards = pool_info
            .generator_ntrn_per_share
            .checked_mul(astroport_lp_amount.to_decimal())?
            .to_uint_floor();
        let pending_astro_rewards =
            total_lockup_astro_rewards.checked_sub(lockup_info.generator_ntrn_debt)?;
        lockup_info.generator_ntrn_debt = total_lockup_astro_rewards;

        // If claimable staking rewards > 0, claim them
        if pending_astro_rewards > Uint128::zero() {
            cosmos_msgs.push(CosmosMsg::Bank(BankMsg::Send {
                to_address: user_address.to_string(),
                amount: vec![Coin {
                    denom: rwi.base_reward_token.to_string(),
                    amount: pending_astro_rewards,
                }],
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
                let pending_proxy_reward: Uint128 = total_lockup_proxy_reward.checked_sub(*debt)?;

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
            cosmos_msgs.push(pending_proxy_reward.into_msg(&deps.querier, user_address.clone())?);
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
        pool_info.amount_in_lockups = pool_info
            .amount_in_lockups
            .checked_sub(lockup_info.lp_units_locked)?;
        ASSET_POOLS.save(deps.storage, pool_type, &pool_info, env.block.height)?;

        attributes.push(attr("astroport_lp_unlocked", astroport_lp_amount));
        lockup_info.astroport_lp_transferred = Some(astroport_lp_amount);
        TOTAL_USER_LOCKUP_AMOUNT.update(
            deps.storage,
            (pool_type, &user_address),
            env.block.height,
            |lockup_amount| -> StdResult<Uint128> {
                if let Some(la) = lockup_amount {
                    Ok(la.checked_sub(lockup_info.lp_units_locked)?)
                } else {
                    Ok(Uint128::zero())
                }
            },
        )?;
    }
    LOCKUP_INFO.save(deps.storage, lockup_key, &lockup_info)?;

    // Transfers claimable one time NTRN rewards to the user that the user gets for all his lock
    if !user_info.ntrn_transferred {
        // Calculating how much NTRN user can claim (from total one time reward)
        let total_claimable_ntrn_rewards = user_info.total_ntrn_rewards;
        if total_claimable_ntrn_rewards > Uint128::zero() {
            cosmos_msgs.push(CosmosMsg::Bank(BankMsg::Send {
                to_address: user_address.to_string(),
                amount: coins(total_claimable_ntrn_rewards.u128(), UNTRN_DENOM),
            }))
        }

        // claim airdrop rewards for airdrop participants
        let res: BalanceResponse = deps.querier.query_wasm_smart(
            config.credits_contract.clone(),
            &Cw20QueryMsg::Balance {
                address: user_address.to_string(),
            },
        )?;
        if res.balance > Uint128::zero() {
            cosmos_msgs.push(claim_airdrop_tokens_with_multiplier_msg(
                deps.as_ref(),
                config.credits_contract,
                user_address.clone(),
                total_claimable_ntrn_rewards,
            )?);
        }

        user_info.ntrn_transferred = true;
        attributes.push(attr(
            "total_claimable_ntrn_reward",
            total_claimable_ntrn_rewards,
        ));
        USER_INFO.save(deps.storage, &user_address, &user_info)?;
    }

    Ok(Response::new()
        .add_messages(cosmos_msgs)
        .add_attributes(attributes))
}

/// Returns the contract's State.
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_state(deps: Deps) -> StdResult<StateResponse> {
    let state: State = STATE.load(deps.storage)?;
    Ok(StateResponse {
        total_incentives_share: state.total_incentives_share,
        supported_pairs_list: ASSET_POOLS
            .keys(deps.storage, None, None, Order::Ascending)
            .collect::<Result<Vec<PoolType>, StdError>>()?,
    })
}

/// Returns the pool's State.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **pool_type** is an object of type [`PoolType`]. LiquidPool type - USDC or ATOM
pub fn query_pool(deps: Deps, pool_type: PoolType) -> StdResult<PoolInfo> {
    let pool_info: PoolInfo = ASSET_POOLS.load(deps.storage, pool_type)?;
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
    let user_address = deps.api.addr_validate(&user)?;
    let user_info = USER_INFO
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    let mut total_astro_rewards = Uint128::zero();
    let mut lockup_infos = vec![];

    let mut claimable_generator_astro_debt = Uint128::zero();
    for pool_type in ASSET_POOLS
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<PoolType>, StdError>>()?
    {
        for duration in LOCKUP_INFO
            .prefix((pool_type, &user_address))
            .keys(deps.storage, None, None, Order::Ascending)
            .collect::<Result<Vec<u64>, StdError>>()?
        {
            let lockup_info = query_lockup_info(deps, &env, &user, pool_type, duration)?;
            total_astro_rewards = total_astro_rewards.checked_add(lockup_info.ntrn_rewards)?;
            claimable_generator_astro_debt = claimable_generator_astro_debt
                .checked_add(lockup_info.claimable_generator_astro_debt)?;
            lockup_infos.push(lockup_info);
        }
    }

    Ok(UserInfoResponse {
        total_ntrn_rewards: total_astro_rewards,
        ntrn_transferred: user_info.ntrn_transferred,
        lockup_infos,
        claimable_generator_ntrn_debt: claimable_generator_astro_debt,
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
    let user_address = deps.api.addr_validate(&user)?;
    let user_info = USER_INFO
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    let mut lockup_infos = vec![];

    for pool_type in ASSET_POOLS
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<PoolType>, StdError>>()?
    {
        for duration in LOCKUP_INFO
            .prefix((pool_type, &user_address))
            .keys(deps.storage, None, None, Order::Ascending)
            .collect::<Result<Vec<u64>, StdError>>()?
        {
            lockup_infos.push(LockUpInfoSummary {
                pool_type,
                duration,
            });
        }
    }

    Ok(UserInfoWithListResponse {
        total_ntrn_rewards: user_info.total_ntrn_rewards,
        ntrn_transferred: user_info.ntrn_transferred,
        lockup_infos,
        lockup_positions_index: user_info.lockup_positions_index,
    })
}

/// Returns locked amount of LP tokens for the specified user for the specified pool at a specific height.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **pool_type** is an object of type [`PoolType`].
///
/// * **user** is an object of type [`Addr`].
///
/// * **height** is an object of type [`u64`].
pub fn query_user_lockup_total_at_height(
    deps: Deps,
    pool: PoolType,
    user: Addr,
    height: u64,
) -> StdResult<Option<Uint128>> {
    TOTAL_USER_LOCKUP_AMOUNT.may_load_at_height(deps.storage, (pool, &user), height)
}

/// Returns a total amount of LP tokens for the specified pool at a specific height.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **pool_type** is an object of type [`PoolType`].
///
/// * **height** is an object of type [`u64`].
pub fn query_lockup_total_at_height(
    deps: Deps,
    pool: PoolType,
    height: u64,
) -> StdResult<Option<Uint128>> {
    if let Some(pool) = ASSET_POOLS.may_load_at_height(deps.storage, pool, height)? {
        return Ok(Some(pool.amount_in_lockups));
    }
    Ok(Some(Uint128::zero()))
}

/// Returns summarized details regarding the user
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **user_address** is an object of type [`&str`].
///
/// * **pool_type** is an object of type [`PoolType`]. LiquidPool type - USDC or ATOM
///
/// * **duration** is an object of type [`u64`].
pub fn query_lockup_info(
    deps: Deps,
    env: &Env,
    user_address: &str,
    pool_type: PoolType,
    duration: u64,
) -> StdResult<LockUpInfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;

    let user_address = deps.api.addr_validate(user_address)?;

    let lockup_key = (pool_type, &user_address, duration);
    let mut pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;
    let mut lockup_info = LOCKUP_INFO.compatible_load(deps, lockup_key, &config.generator)?;

    let lockup_astroport_lp_units_opt: Option<Uint128>;
    let astroport_lp_token_opt: Addr;
    let mut claimable_generator_astro_debt = Uint128::zero();
    let mut claimable_generator_proxy_debt: RestrictedVector<AssetInfo, Uint128> =
        RestrictedVector::default();
    if let Some(astroport_lp_transferred) = lockup_info.astroport_lp_transferred {
        lockup_astroport_lp_units_opt = Some(astroport_lp_transferred);
        astroport_lp_token_opt = pool_info.lp_token;
    } else {
        let astroport_lp_token = pool_info.lp_token;
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
                .checked_div(Uint256::from(pool_info.amount_in_lockups))?)
            .try_into()?
        };
        lockup_astroport_lp_units_opt = Some(lockup_astroport_lp_units);
        astroport_lp_token_opt = astroport_lp_token.clone();
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
            pool_info.generator_ntrn_per_share =
                pool_info
                    .generator_ntrn_per_share
                    .checked_add(Decimal::from_ratio(
                        pending_rewards.pending,
                        pool_astroport_lp_units,
                    ))?;

            let total_lockup_astro_rewards = pool_info
                .generator_ntrn_per_share
                .checked_mul(lockup_astroport_lp_units.to_decimal())?
                .to_uint_floor();
            claimable_generator_astro_debt =
                total_lockup_astro_rewards.checked_sub(lockup_info.generator_ntrn_debt)?;

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
        pool_type,
        lp_units_locked: lockup_info.lp_units_locked,
        withdrawal_flag: lockup_info.withdrawal_flag,
        ntrn_rewards: lockup_info.ntrn_rewards,
        generator_ntrn_debt: lockup_info.generator_ntrn_debt,
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
    pool_incentives_share: Uint128,
    total_incentives_share: Uint128,
    total_lockdrop_incentives: Uint128,
) -> StdResult<Uint128> {
    if total_incentives_share.is_zero() || total_weighted_amount.is_zero() {
        Ok(Uint128::zero())
    } else {
        Ok(Decimal256::from_ratio(
            Uint256::from(pool_incentives_share).checked_mul(lockup_weighted_balance)?,
            Uint256::from(total_incentives_share).checked_mul(total_weighted_amount)?,
        )
        .checked_mul_uint256(total_lockdrop_incentives.into())?)
    }
}

/// Returns effective weight for the amount to be used for calculating lockdrop rewards.
/// ## Params
/// * **amount** is an object of type [`Uint128`]. Number of LP tokens.
///
/// * **duration** is an object of type [`u64`]. Number of seconds.
///
/// * **config** is an object of type [`Config`]. Config with weekly multiplier and divider.
fn calculate_weight(amount: Uint128, duration: u64, config: &Config) -> StdResult<Uint256> {
    if let Some(info) = config
        .lockup_rewards_info
        .iter()
        .find(|info| info.duration == duration)
    {
        let lock_weight = Decimal256::one() + info.coefficient;
        Ok(lock_weight.checked_mul_uint256(amount.into())?.into())
    } else {
        Err(StdError::generic_err("invalid duration"))
    }
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

    let mut keys: Vec<(PoolType, u64)> = vec![];

    for pool_type in ASSET_POOLS
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<PoolType>, StdError>>()?
    {
        for duration in LOCKUP_INFO
            .prefix((pool_type, user_address))
            .keys(deps.storage, None, None, Order::Ascending)
            .collect::<Result<Vec<u64>, StdError>>()?
        {
            keys.push((pool_type, duration));
        }
    }
    for (pool_type, duration) in keys {
        let pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;
        let lockup_key = (pool_type, user_address, duration);
        let mut lockup_info =
            LOCKUP_INFO.compatible_load(deps.as_ref(), lockup_key, &config.generator)?;

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
