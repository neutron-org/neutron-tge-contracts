use crate::msg::{CallbackMsg, ExecuteMsg, InstantiateMsg, MigrateMsg};
use crate::state::{XykToClMigrationConfig, XYK_TO_CL_MIGRATION_CONFIG};
use astroport::asset::{native_asset, PairInfo};
use astroport::pair::{
    Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg, QueryMsg as PairQueryMsg,
};
use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Response, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};
use vesting_base::builder::VestingBaseBuilder;
use vesting_base::error::ContractError;
use vesting_base::handlers::execute as base_execute;
use vesting_base::handlers::query as base_query;
use vesting_base::msg::{ExecuteMsg as BaseExecute, QueryMsg};
use vesting_base::state::{vesting_info, vesting_state, CONFIG};
use vesting_base::types::{
    VestingAccountResponse, VestingInfo, VestingSchedule, VestingSchedulePoint,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "neutron-vesting-lp";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters packed in the `msg` variable.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    VestingBaseBuilder::default()
        .historical()
        .with_managers(msg.vesting_managers)
        .build(deps, msg.owner, msg.token_info_manager)?;
    Ok(Response::default())
}

/// Exposes execute functions available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::MigrateLiquidityToPCLPool { user } => {
            execute_migrate_liquidity(deps, info, env, None, user)
        }
        ExecuteMsg::Callback(msg) => _handle_callback(deps, env, info, msg),
        _ => {
            let base_msg: BaseExecute = msg.into();
            base_execute(deps, env, info, base_msg)
        }
    }
}

/// Exposes all the queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    base_query(deps, env, msg)
}

/// Manages contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    XYK_TO_CL_MIGRATION_CONFIG.save(
        deps.storage,
        &XykToClMigrationConfig {
            max_slippage: msg.max_slippage,
            ntrn_denom: msg.ntrn_denom,
            xyk_pair: deps.api.addr_validate(msg.xyk_pair.as_str())?,
            paired_denom: msg.paired_denom,
            cl_pair: deps.api.addr_validate(msg.cl_pair.as_str())?,
            new_lp_token: deps.api.addr_validate(msg.new_lp_token.as_str())?,
            pcl_vesting: deps.api.addr_validate(msg.pcl_vesting.as_str())?,
            dust_threshold: msg.dust_threshold,
        },
    )?;

    Ok(Response::default())
}

fn execute_migrate_liquidity(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    slippage_tolerance: Option<Decimal>,
    user: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let migration_config: XykToClMigrationConfig = XYK_TO_CL_MIGRATION_CONFIG.load(deps.storage)?;
    let address = match user {
        Some(val) => deps.api.addr_validate(&val)?,
        None => info.sender,
    };
    let info = vesting_info(config.extensions.historical).load(deps.storage, address.clone())?;
    let mut resp = Response::default();
    let user = VestingAccountResponse { address, info };

    // get pairs LP token addresses
    let pair_info: PairInfo = deps
        .querier
        .query_wasm_smart(migration_config.xyk_pair.clone(), &PairQueryMsg::Pair {})?;

    // query max available amounts to be withdrawn from pool
    let max_available_amount = {
        let resp: BalanceResponse = deps.querier.query_wasm_smart(
            pair_info.liquidity_token.clone(),
            &Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?;
        resp.balance
    };

    if max_available_amount.is_zero() {
        return Ok(resp);
    }

    let user_share = compute_share(&user.info)?;
    let user_amount = if user_share < migration_config.dust_threshold {
        if !user_share.is_zero() {
            resp = resp.add_message(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: pair_info.liquidity_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: user.address.to_string(),
                    amount: user_share,
                })?,
                funds: vec![],
            }));
        }

        Uint128::zero()
    } else {
        user_share
    };

    if let Some(slippage_tolerance) = slippage_tolerance {
        if slippage_tolerance.gt(&migration_config.max_slippage) {
            return Err(ContractError::MigrationError {});
        }
    }

    let slippage_tolerance = slippage_tolerance.unwrap_or(migration_config.max_slippage);

    resp = resp.add_message(
        CallbackMsg::MigrateLiquidityToClPair {
            xyk_pair: migration_config.xyk_pair.clone(),
            xyk_lp_token: pair_info.liquidity_token.clone(),
            amount: user_amount,
            slippage_tolerance,
            cl_pair: migration_config.cl_pair.clone(),
            ntrn_denom: migration_config.ntrn_denom.clone(),
            paired_asset_denom: migration_config.paired_denom.clone(),
            user,
        }
        .to_cosmos_msg(&env)?,
    );

    Ok(resp)
}

fn _handle_callback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> Result<Response, ContractError> {
    // Only the contract itself can call callbacks
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    match msg {
        CallbackMsg::MigrateLiquidityToClPair {
            xyk_pair,
            xyk_lp_token,
            amount,
            slippage_tolerance,
            cl_pair,
            ntrn_denom,
            paired_asset_denom,
            user,
        } => migrate_liquidity_to_cl_pair_callback(
            deps,
            env,
            xyk_pair,
            xyk_lp_token,
            amount,
            slippage_tolerance,
            cl_pair,
            ntrn_denom,
            paired_asset_denom,
            user,
        ),
        CallbackMsg::ProvideLiquidityToClPairAfterWithdrawal {
            ntrn_denom,
            ntrn_init_balance,
            paired_asset_denom,
            paired_asset_init_balance,
            cl_pair,
            slippage_tolerance,
            user,
        } => provide_liquidity_to_cl_pair_after_withdrawal_callback(
            deps,
            env,
            ntrn_denom,
            ntrn_init_balance,
            paired_asset_denom,
            paired_asset_init_balance,
            cl_pair,
            slippage_tolerance,
            user,
        ),
        CallbackMsg::PostMigrationVestingReschedule {
            user,
            init_balance_pcl_lp,
        } => post_migration_vesting_reschedule_callback(deps, env, &user, init_balance_pcl_lp),
    }
}

#[allow(clippy::too_many_arguments)]
fn migrate_liquidity_to_cl_pair_callback(
    deps: DepsMut,
    env: Env,
    xyk_pair: Addr,
    xyk_lp_token: Addr,
    amount: Uint128,
    slippage_tolerance: Decimal,
    cl_pair: Addr,
    ntrn_denom: String,
    paired_asset_denom: String,
    user: VestingAccountResponse,
) -> Result<Response, ContractError> {
    let ntrn_init_balance = deps
        .querier
        .query_balance(env.contract.address.to_string(), ntrn_denom.clone())?
        .amount;
    let paired_asset_init_balance = deps
        .querier
        .query_balance(env.contract.address.to_string(), paired_asset_denom.clone())?
        .amount;

    let mut msgs: Vec<CosmosMsg> = vec![];

    // push message to withdraw liquidity from the xyk pair
    if !amount.is_zero() {
        msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: xyk_lp_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: xyk_pair.to_string(),
                amount,
                msg: to_binary(&PairCw20HookMsg::WithdrawLiquidity { assets: vec![] })?,
            })?,
            funds: vec![],
        }))
    }
    let config = CONFIG.load(deps.storage)?;
    vesting_state(config.extensions.historical).update::<_, ContractError>(
        deps.storage,
        env.block.height,
        |s| {
            let mut state = s.unwrap_or_default();
            state.total_released = state.total_released.checked_add(amount)?;
            Ok(state)
        },
    )?;
    // push the next migration step as a callback message
    msgs.push(
        CallbackMsg::ProvideLiquidityToClPairAfterWithdrawal {
            ntrn_denom,
            ntrn_init_balance,
            paired_asset_denom,
            paired_asset_init_balance,
            cl_pair,
            slippage_tolerance,
            user,
        }
        .to_cosmos_msg(&env)?,
    );

    Ok(Response::default().add_messages(msgs))
}

#[allow(clippy::too_many_arguments)]
fn provide_liquidity_to_cl_pair_after_withdrawal_callback(
    deps: DepsMut,
    env: Env,
    ntrn_denom: String,
    ntrn_init_balance: Uint128,
    paired_asset_denom: String,
    paired_asset_init_balance: Uint128,
    cl_pair_address: Addr,
    slippage_tolerance: Decimal,
    user: VestingAccountResponse,
) -> Result<Response, ContractError> {
    let ntrn_balance_after_withdrawal = deps
        .querier
        .query_balance(env.contract.address.to_string(), ntrn_denom.clone())?
        .amount;
    let paired_asset_balance_after_withdrawal = deps
        .querier
        .query_balance(env.contract.address.to_string(), paired_asset_denom.clone())?
        .amount;

    // calc amount of assets that's been withdrawn
    let withdrawn_ntrn_amount = ntrn_balance_after_withdrawal.checked_sub(ntrn_init_balance)?;
    let withdrawn_paired_asset_amount =
        paired_asset_balance_after_withdrawal.checked_sub(paired_asset_init_balance)?;

    let mut msgs: Vec<CosmosMsg> = vec![];

    let migration_config: XykToClMigrationConfig = XYK_TO_CL_MIGRATION_CONFIG.load(deps.storage)?;

    let balance_response: BalanceResponse = deps.querier.query_wasm_smart(
        migration_config.new_lp_token,
        &Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;
    let current_balance = balance_response.balance;

    if !withdrawn_ntrn_amount.is_zero() && !withdrawn_paired_asset_amount.is_zero() {
        msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cl_pair_address.to_string(),
            msg: to_binary(&PairExecuteMsg::ProvideLiquidity {
                assets: vec![
                    native_asset(ntrn_denom.clone(), withdrawn_ntrn_amount),
                    native_asset(paired_asset_denom.clone(), withdrawn_paired_asset_amount),
                ],
                slippage_tolerance: Some(slippage_tolerance),
                auto_stake: None,
                receiver: None,
            })?,
            funds: vec![
                Coin::new(withdrawn_ntrn_amount.into(), ntrn_denom),
                Coin::new(withdrawn_paired_asset_amount.into(), paired_asset_denom),
            ],
        }))
    }

    msgs.push(
        CallbackMsg::PostMigrationVestingReschedule {
            user,
            init_balance_pcl_lp: current_balance,
        }
        .to_cosmos_msg(&env)?,
    );

    Ok(Response::default().add_messages(msgs))
}

fn post_migration_vesting_reschedule_callback(
    deps: DepsMut,
    env: Env,
    user: &VestingAccountResponse,
    init_balance_pcl_lp: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let migration_config: XykToClMigrationConfig = XYK_TO_CL_MIGRATION_CONFIG.load(deps.storage)?;
    let balance_response: BalanceResponse = deps.querier.query_wasm_smart(
        &migration_config.new_lp_token,
        &Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;
    let current_balance = balance_response.balance.checked_sub(init_balance_pcl_lp)?;

    let schedule = user.info.schedules.last().unwrap();

    let new_end_point = match &schedule.end_point {
        Some(end_point) => Option::from(VestingSchedulePoint {
            time: end_point.time,
            amount: current_balance,
        }),
        None => None,
    };

    let new_schedule = VestingSchedule {
        start_point: VestingSchedulePoint {
            time: schedule.start_point.time,
            amount: Uint128::zero(),
        },
        end_point: new_end_point,
    };

    let vesting_info = vesting_info(config.extensions.historical);

    vesting_info.save(
        deps.storage,
        user.address.clone(),
        &VestingInfo {
            schedules: vec![],
            released_amount: Uint128::zero(),
        },
        env.block.height,
    )?;
    if !current_balance.is_zero() {
        let msgs = vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: migration_config.new_lp_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: migration_config.pcl_vesting.to_string(),
                amount: current_balance,
                msg: to_binary(&vesting_lp_pcl::msg::Cw20HookMsg::MigrateXYKLiquidity {
                    user_address_raw: user.address.clone(),
                    user_vesting_info: VestingInfo {
                        schedules: vec![new_schedule],
                        released_amount: Uint128::zero(),
                    },
                })?,
            })?,
        })];

        Ok(Response::new().add_messages(msgs))
    } else {
        Ok(Response::new())
    }
}

fn compute_share(vesting_info: &VestingInfo) -> StdResult<Uint128> {
    let mut available_amount: Uint128 = Uint128::zero();
    for sch in &vesting_info.schedules {
        if let Some(end_point) = &sch.end_point {
            available_amount = available_amount.checked_add(end_point.amount)?
        }
    }

    Ok(available_amount.checked_sub(vesting_info.released_amount)?)
}
