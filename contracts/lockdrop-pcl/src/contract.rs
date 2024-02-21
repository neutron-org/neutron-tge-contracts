use std::cmp::min;
use std::ops::Sub;
use std::str::FromStr;

use astroport::asset::{Asset, AssetInfo};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::cosmwasm_ext::IntegerToDecimal;
use astroport::incentives::{ExecuteMsg as IncentivesExecuteMsg, QueryMsg as IncentivesQueryMsg};
use astroport::pair::ExecuteMsg::ProvideLiquidity;
use astroport::restricted_vector::RestrictedVector;
use astroport::DecimalCheckedOps;
use astroport_periphery::utils::Decimal256CheckedOps;
use cosmwasm_std::{
    attr, coins, entry_point, to_json_binary, Addr, BankMsg, Binary, CosmosMsg, Decimal,
    Decimal256, Deps, DepsMut, Empty, Env, MessageInfo, Order, Response, StdError, StdResult,
    Uint128, Uint256, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};

use crate::raw_queries::{raw_balance, raw_incentives_deposit};
use astroport_periphery::lockdrop::{
    LockupInfoV2 as LockdropXYKLockupInfoV2, PoolType as LockdropXYKPoolType,
    UserInfo as LockdropXYKUserInfo,
};
use astroport_periphery::lockdrop_pcl::{
    CallbackMsg, Config, ExecuteMsg, InstantiateMsg, LockUpInfoResponse, LockUpInfoSummary,
    LockupInfo, MigrateMsg, PoolInfo, PoolType, QueryMsg, State, StateResponse, UpdateConfigMsg,
    UserInfo, UserInfoResponse, UserInfoWithListResponse,
};

use crate::state::{
    ASSET_POOLS, CONFIG, LOCKUP_INFO, OWNERSHIP_PROPOSAL, STATE, TOTAL_USER_LOCKUP_AMOUNT,
    USER_INFO,
};

const AIRDROP_REWARDS_MULTIPLIER: &str = "1.0";

pub const UNTRN_DENOM: &str = "untrn";

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "neutron_lockdrop_pcl";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters packed in the `msg` variable.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

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

    let config = Config {
        owner: msg
            .owner
            .map(|v| deps.api.addr_validate(&v))
            .transpose()?
            .unwrap_or(info.sender),
        xyk_lockdrop_contract: deps.api.addr_validate(&msg.xyk_lockdrop_contract)?,
        credits_contract: deps.api.addr_validate(&msg.credits_contract)?,
        auction_contract: deps.api.addr_validate(&msg.auction_contract)?,
        incentives: deps.api.addr_validate(&msg.incentives)?,
        lockdrop_incentives: msg.lockdrop_incentives,
        lockup_rewards_info: msg.lockup_rewards_info,
    };
    CONFIG.save(deps.storage, &config)?;

    // Initialize NTRN/ATOM pool
    let pool_info = PoolInfo {
        lp_token: deps.api.addr_validate(&msg.atom_token)?,
        amount_in_lockups: Default::default(),
        incentives_share: msg.atom_incentives_share,
        weighted_amount: msg.atom_weighted_amount,
        incentives_rewards_per_share: RestrictedVector::default(),
        is_staked: true,
    };
    ASSET_POOLS.save(deps.storage, PoolType::ATOM, &pool_info, env.block.height)?;

    // Initialize NTRN/USDC pool
    let pool_info = PoolInfo {
        lp_token: deps.api.addr_validate(&msg.usdc_token)?,
        amount_in_lockups: Default::default(),
        incentives_share: msg.usdc_incentives_share,
        weighted_amount: msg.usdc_weighted_amount,
        incentives_rewards_per_share: RestrictedVector::default(),
        is_staked: true,
    };
    ASSET_POOLS.save(deps.storage, PoolType::USDC, &pool_info, env.block.height)?;

    STATE.save(deps.storage, &State::default())?;
    Ok(Response::default())
}

/// Exposes all the execute functions available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
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
        ExecuteMsg::UpdateConfig { new_config } => handle_update_config(deps, info, new_config),
        ExecuteMsg::MigrateXYKLiquidity {
            pool_type,
            user_address_raw,
            duration,
            user_info,
            lockup_info,
        } => handle_migrate_xyk_liquidity(
            deps,
            env,
            info,
            pool_type,
            user_address_raw,
            duration,
            user_info,
            lockup_info,
        ),
    }
}

/// Handles callback.
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
            prev_reward_balances,
        } => update_pool_on_dual_rewards_claim(deps, env, pool_type, prev_reward_balances),
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
        CallbackMsg::FinishLockupMigrationCallback {
            pool_type,
            user_address,
            duration,
            lp_token,
            staked_lp_token_amount,
            user_info,
            lockup_info,
        } => callback_finish_lockup_migration(
            deps,
            env,
            pool_type,
            user_address,
            duration,
            lp_token,
            staked_lp_token_amount,
            user_info,
            lockup_info,
        ),
    }
}

/// Exposes all the queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::State {} => to_json_binary(&query_state(deps)?),
        QueryMsg::Pool { pool_type } => to_json_binary(&query_pool(deps, pool_type)?),
        QueryMsg::UserInfo { address } => to_json_binary(&query_user_info(deps, env, address)?),
        QueryMsg::UserInfoWithLockupsList { address } => {
            to_json_binary(&query_user_info_with_lockups_list(deps, env, address)?)
        }
        QueryMsg::LockUpInfo {
            user_address,
            pool_type,
            duration,
        } => to_json_binary(&query_lockup_info(
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
        } => to_json_binary(&query_user_lockup_total_at_height(
            deps,
            pool_type,
            deps.api.addr_validate(&user_address)?,
            height,
        )?),
        QueryMsg::QueryLockupTotalAtHeight { pool_type, height } => {
            to_json_binary(&query_lockup_total_at_height(deps, pool_type, height)?)
        }
    }
}

/// Used for contract migration.
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

    if let Some(incentives) = new_config.incentives_address {
        // If incentives is set, we check is any LP tokens are currently staked before updating incentives address
        for pool_type in ASSET_POOLS
            .keys(deps.storage, None, None, Order::Ascending)
            .collect::<Result<Vec<PoolType>, StdError>>()?
        {
            let pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;
            if pool_info.is_staked {
                return Err(StdError::generic_err(format!(
                    "{:?} astro LP tokens already staked. Unstake them before updating incentives",
                    pool_type
                )));
            }
        }

        config.incentives = deps.api.addr_validate(&incentives)?;
        attributes.push(attr("new_incentives", incentives))
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attributes(attributes))
}

#[allow(clippy::too_many_arguments)]
/// Creates a lockup position based on the provided parameters and XYK lockdrop contract's state. No
/// validation is performed on the lockup parameters, they are expected to be valid ones because the
/// only valid sender of the message is the XYK lockdrop contract which has already validated them
/// in the beginning of the TGE.
///
/// Exactly two **Coin**s are expected to be attached to the message as funds. These **Coin**s are
/// used in ProvideLiquidity message sent to the PCL pool, the minted LP tokens are staked to the
/// incentives.
///
/// Liquidity migration process consists of several sequential messages invocation and from this
/// contract's point of view it mostly mimics (and clones code of) the **IncreaseLockupFor** exec
/// handler of the XYK lockdrop contract called for each lockup position. So, for this contract's
/// state, liquidity migration looks like lockups creation just like it happened in the beginning
/// of the token generation event (TGE).
pub fn handle_migrate_xyk_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pool_type: LockdropXYKPoolType,
    user_address_raw: String,
    duration: u64,
    user_info: LockdropXYKUserInfo,
    lockup_info: LockdropXYKLockupInfoV2,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.xyk_lockdrop_contract {
        return Err(StdError::generic_err(
            "only the XYK lockdrop contract is authorized to call the liquidity migration handler",
        ));
    }
    if info.funds.len() != 2 {
        return Err(StdError::generic_err(
            "exactly two assets are expected to be attached to the message",
        ));
    }

    // determine the PCL pool info and the current staked lp token amount
    let pool_info = ASSET_POOLS.load(deps.storage, pool_type.into())?;
    let astroport_pool: String = deps
        .querier
        .query_wasm_smart::<MinterResponse>(
            pool_info.lp_token.to_string(),
            &cw20::Cw20QueryMsg::Minter {},
        )?
        .minter;
    let staked_lp_token_amount = deps.querier.query_wasm_smart::<Uint128>(
        config.incentives.to_string(),
        &IncentivesQueryMsg::Deposit {
            lp_token: pool_info.lp_token.to_string(),
            user: env.contract.address.to_string(),
        },
    )?;

    // provide the transferred liquidity to the PCL pool
    let mut cosmos_msgs: Vec<CosmosMsg<Empty>> = vec![CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: astroport_pool.to_string(),
        funds: info.funds.clone(),
        msg: to_json_binary(&ProvideLiquidity {
            assets: info
                .funds
                .iter()
                .map(|f| Asset {
                    info: AssetInfo::NativeToken {
                        denom: f.denom.clone(),
                    },
                    amount: f.amount,
                })
                .collect(),
            slippage_tolerance: None,
            auto_stake: Some(true),
            receiver: None,
        })?,
    })];
    // invoke callback that creates a lockup entry for the provided liquidity
    cosmos_msgs.push(
        CallbackMsg::FinishLockupMigrationCallback {
            pool_type: pool_type.into(),
            user_address: deps.api.addr_validate(&user_address_raw)?,
            duration,
            lp_token: pool_info.lp_token.to_string(),
            staked_lp_token_amount,
            user_info,
            lockup_info,
        }
        .to_cosmos_msg(&env)?,
    );

    Ok(Response::default().add_messages(cosmos_msgs))
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
    let lockup_info = LOCKUP_INFO.load(deps.storage, lockup_key)?;

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
        let incentives = &config.incentives;

        // QUERY :: Check if there are any pending staking rewards
        let pending_rewards_response: Vec<Asset> = deps.querier.query_wasm_smart(
            incentives,
            &IncentivesQueryMsg::PendingRewards {
                lp_token: astroport_lp_token.to_string(),
                user: env.contract.address.to_string(),
            },
        )?;

        if pending_rewards_response
            .iter()
            .any(|asset| !asset.amount.is_zero())
        {
            let prev_pending_rewards_balances: Vec<Asset> = pending_rewards_response
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
                contract_addr: incentives.to_string(),
                funds: vec![],
                msg: to_json_binary(&IncentivesExecuteMsg::ClaimRewards {
                    lp_tokens: vec![astroport_lp_token.to_string()],
                })?,
            }));

            cosmos_msgs.push(
                CallbackMsg::UpdatePoolOnDualRewardsClaim {
                    pool_type,
                    prev_reward_balances: prev_pending_rewards_balances,
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
        msg: to_json_binary(&Cw20ExecuteMsg::BurnFrom {
            owner: user_addr.to_string(),
            amount: claimable_vested_amount.checked_add(unvested_tokens_amount.amount)?,
        })?,
        funds: vec![],
    }))
}

/// Updates contract state after dual staking rewards are claimed from the incentives contract. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **pool_type** is an object of type [`PoolType`]. LiquidPool type - USDC or ATOM
///
/// * **prev_pending_rewards_balances** is a vector of type [`Asset`]. Contract's Incentives reward token balance before claim.
pub fn update_pool_on_dual_rewards_claim(
    deps: DepsMut,
    env: Env,
    pool_type: PoolType,
    prev_pending_rewards_balances: Vec<Asset>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;
    let incentives = &config.incentives;
    let astroport_lp_token = pool_info.lp_token.clone();

    let lp_balance: Uint128 = deps.querier.query_wasm_smart(
        incentives,
        &IncentivesQueryMsg::Deposit {
            lp_token: astroport_lp_token.to_string(),
            user: env.contract.address.to_string(),
        },
    )?;

    // Increment claimed rewards per LP share
    for prev_balance in prev_pending_rewards_balances {
        let current_balance = prev_balance
            .info
            .query_pool(&deps.querier, env.contract.address.clone())?;
        let received_amount = current_balance.checked_sub(prev_balance.amount)?;

        pool_info.incentives_rewards_per_share.update(
            &prev_balance.info,
            Decimal::from_ratio(received_amount, lp_balance),
        )?;
    }

    // SAVE UPDATED STATE OF THE POOL
    ASSET_POOLS.save(deps.storage, pool_type, &pool_info, env.block.height)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "update_incentives_dual_rewards"),
        attr("pool_type", format!("{:?}", pool_type)),
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
    let mut lockup_info = LOCKUP_INFO.load(deps.storage, lockup_key)?;

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
    let incentives = &config.incentives;

    // Calculate Astro LP share for the lockup position
    let astroport_lp_amount: Uint128 = {
        let balance: Uint128 = if pool_info.is_staked {
            deps.querier.query_wasm_smart(
                incentives,
                &IncentivesQueryMsg::Deposit {
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

    let mut pending_reward_assets: Vec<Asset> = vec![];
    // If Astro LP tokens are staked with Astro incentives
    if pool_info.is_staked {
        // If this LP token is getting incentives
        // Calculate claimable staking rewards for this lockup
        let pending_rewards: Vec<Asset> = deps.querier.query_wasm_smart(
            incentives,
            &IncentivesQueryMsg::PendingRewards {
                lp_token: astroport_lp_token.to_string(),
                user: env.contract.address.to_string(),
            },
        )?;
        for reward in pending_rewards {
            let incentives_rewards_per_share = pool_info
                .incentives_rewards_per_share
                .load(&reward.info)
                .unwrap_or_default();
            if incentives_rewards_per_share.is_zero() {
                continue;
            };

            let total_lockup_rewards = incentives_rewards_per_share
                .checked_mul(astroport_lp_amount.to_decimal())?
                .to_uint_floor();
            let debt = lockup_info
                .incentives_debt
                .load(&reward.info)
                .unwrap_or_default();
            let pending_reward = total_lockup_rewards.checked_sub(debt)?;

            if !pending_reward.is_zero() {
                pending_reward_assets.push(Asset {
                    info: reward.info.clone(),
                    amount: pending_reward,
                });
            }

            lockup_info
                .incentives_debt
                .update(&reward.info, total_lockup_rewards.checked_sub(debt)?)?;
        }

        // If this is a void transaction (no state change), then return error.
        // Void tx scenario = ASTRO already claimed, 0 pending ASTRO staking reward, 0 pending rewards, not unlocking LP tokens in this tx
        if !withdraw_lp_stake && user_info.ntrn_transferred && pending_reward_assets.is_empty() {
            return Err(StdError::generic_err("No rewards available to claim!"));
        }

        // If claimable staking rewards > 0, claim them
        for pending_reward in pending_reward_assets {
            cosmos_msgs.push(pending_reward.into_msg(user_address.clone())?);
        }

        //  COSMOSMSG :: If LP Tokens are staked, we unstake the amount which needs to be returned to the user
        if withdraw_lp_stake {
            cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: incentives.to_string(),
                funds: vec![],
                msg: to_json_binary(&IncentivesExecuteMsg::Withdraw {
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
            msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
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

#[allow(clippy::too_many_arguments)]
/// Completes the liquidity migration process by making all necessary state updates for the lockup
/// position.
pub fn callback_finish_lockup_migration(
    deps: DepsMut,
    env: Env,
    pool_type: PoolType,
    user_address: Addr,
    duration: u64,
    lp_token: String,
    staked_lp_token_amount: Uint128,
    user_info: LockdropXYKUserInfo,
    lockup_info: LockdropXYKLockupInfoV2,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let staked_lp_token_amount_diff = deps
        .querier
        .query_wasm_smart::<Uint128>(
            config.incentives.to_string(),
            &IncentivesQueryMsg::Deposit {
                lp_token,
                user: env.contract.address.to_string(),
            },
        )?
        .sub(staked_lp_token_amount);

    let user_info: UserInfo = user_info.into();
    let lockup_info: LockupInfo =
        LockupInfo::from_xyk_lockup_info(lockup_info, staked_lp_token_amount_diff);
    let mut pool_info = ASSET_POOLS.load(deps.storage, pool_type)?;
    let config = CONFIG.load(deps.storage)?;

    pool_info.weighted_amount = pool_info.weighted_amount.checked_add(calculate_weight(
        staked_lp_token_amount_diff,
        duration,
        &config,
    )?)?;
    pool_info.amount_in_lockups = pool_info
        .amount_in_lockups
        .checked_add(staked_lp_token_amount_diff)?;

    let lockup_key = (pool_type, &user_address, duration);
    LOCKUP_INFO.save(deps.storage, lockup_key, &lockup_info)?;

    TOTAL_USER_LOCKUP_AMOUNT.update(
        deps.storage,
        (pool_type, &user_address),
        env.block.height,
        |lockup_amount| -> StdResult<Uint128> {
            if let Some(la) = lockup_amount {
                Ok(la.checked_add(staked_lp_token_amount_diff)?)
            } else {
                Ok(staked_lp_token_amount_diff)
            }
        },
    )?;

    ASSET_POOLS.save(deps.storage, pool_type, &pool_info, env.block.height)?;
    USER_INFO.save(deps.storage, &user_address, &user_info)?;

    Ok(Response::default())
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
    let mut claimable_incentives_debt: RestrictedVector<AssetInfo, Uint128> = Default::default();
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

            for v in lockup_info.claimable_incentives_debt.inner_ref().iter() {
                claimable_incentives_debt.update(&v.0, v.1)?;
            }
            lockup_infos.push(lockup_info);
        }
    }

    Ok(UserInfoResponse {
        total_ntrn_rewards: total_astro_rewards,
        ntrn_transferred: user_info.ntrn_transferred,
        lockup_infos,
        claimable_incentives_debt,
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
    let mut lockup_info = LOCKUP_INFO.load(deps.storage, lockup_key)?;

    let lockup_astroport_lp_units_opt: Option<Uint128>;
    let astroport_lp_token_opt: Addr;
    let mut claimable_incentives_rewards_debt: RestrictedVector<AssetInfo, Uint128> =
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
                raw_incentives_deposit(
                    deps.querier,
                    &config.incentives,
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
            let incentives = &config.incentives;
            // QUERY :: Check if there are any pending staking rewards
            let pending_rewards: Vec<Asset> = deps.querier.query_wasm_smart(
                incentives,
                &IncentivesQueryMsg::PendingRewards {
                    lp_token: astroport_lp_token.to_string(),
                    user: env.contract.address.to_string(),
                },
            )?;

            // Calculate claimable staking rewards for this lockup
            for reward in pending_rewards {
                let incentives_rewards_per_share = pool_info.incentives_rewards_per_share.update(
                    &reward.info,
                    Decimal::from_ratio(reward.amount, pool_astroport_lp_units),
                )?;

                let debt = incentives_rewards_per_share
                    .checked_mul_uint128(lockup_astroport_lp_units)?
                    .checked_sub(
                        lockup_info
                            .incentives_debt
                            .inner_ref()
                            .iter()
                            .find_map(|a| if reward.info == a.0 { Some(a.1) } else { None })
                            .unwrap_or_default(),
                    )?;

                claimable_incentives_rewards_debt.update(&reward.info, debt)?;
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
        incentives_debt: lockup_info.incentives_debt,
        claimable_incentives_debt: claimable_incentives_rewards_debt,
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
        let mut lockup_info = LOCKUP_INFO.load(deps.storage, lockup_key)?;

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
