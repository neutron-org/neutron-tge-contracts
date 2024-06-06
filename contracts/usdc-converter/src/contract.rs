use crate::error::ContractError;
use crate::msg::{CallbackMsg, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{Config, CONFIG};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::pair::{
    Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg, QueryMsg as PairQueryMsg,
};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, to_json_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

pub(crate) const CONTRACT_NAME: &str = "crates.io:neutron-usdc-converter";
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const NTRN_DENOM: &str = "untrn";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        axl_pool: deps.api.addr_validate(&msg.axl_pool)?,
        axl_usdc_denom: msg.axl_usdc_denom,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("axl_pool", config.axl_pool)
        .add_attribute("axl_usdc_denom", config.axl_usdc_denom))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Callback(msg) => handle_callback(deps, env, info, msg),
    }
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let axl_pool_info: PairInfo = deps
        .querier
        .query_wasm_smart(config.axl_pool.to_string(), &PairQueryMsg::Pair {})?;
    // check that the received cw20 token is the USDC.axl<>NTRN pool LP token
    if info
        .sender
        .to_string()
        .ne(&axl_pool_info.liquidity_token.to_string())
    {
        return Err(StdError::generic_err(
            "only USDC.axl<>NTRN pool LP tokens are supported for conversion",
        )
        .into());
    }

    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::ConvertAndStake {
            transmuter_pool,
            noble_pool,
            noble_usdc_denom,
            provide_liquidity_slippage_tolerance,
        } => execute_convert_and_stake(
            deps,
            env,
            cw20_msg.sender,
            cw20_msg.amount,
            axl_pool_info.liquidity_token.to_string(),
            transmuter_pool,
            noble_pool,
            noble_usdc_denom,
            provide_liquidity_slippage_tolerance,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_convert_and_stake(
    deps: DepsMut,
    env: Env,
    caller: String,
    amount_to_convert: Uint128,
    axl_pool_lp_token: String,
    transmuter_pool: String,
    noble_pool: String,
    noble_usdc_denom: String,
    provide_liquidity_slippage_tolerance: Decimal,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let ntrn_balance: Uint128 = deps
        .querier
        .query_balance(env.contract.address.to_string(), NTRN_DENOM.to_string())?
        .amount;
    let axl_usdc_balance: Uint128 = deps
        .querier
        .query_balance(env.contract.address.to_string(), config.axl_usdc_denom)?
        .amount;

    deps.api.debug(&format!(
        "WASMDEBUG: got {} LP tokens for conversion",
        amount_to_convert
    ));
    deps.api.debug(&format!(
        "WASMDEBUG: initial ntrn balance: {}",
        ntrn_balance
    ));
    deps.api.debug(&format!(
        "WASMDEBUG: initial USDC.axl balance: {}",
        axl_usdc_balance
    ));

    Ok(Response::new()
        .add_messages([
            // withdraw USDC.axl<>NTRN liquidity
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: axl_pool_lp_token.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Send {
                    contract: config.axl_pool.to_string(),
                    amount: amount_to_convert,
                    msg: to_json_binary(&PairCw20HookMsg::WithdrawLiquidity { assets: vec![] })?,
                })?,
                funds: vec![],
            }),
            // invoke the next conversion step
            CallbackMsg::SwapCallback {
                transmuter_pool: transmuter_pool.clone(),
                axl_usdc_prev_balance: axl_usdc_balance,
                ntrn_prev_balance: ntrn_balance,
                provide_liquidity_slippage_tolerance,
                caller,
                noble_pool: noble_pool.clone(),
                noble_usdc_denom,
            }
            .to_cosmos_msg(&env)?,
        ])
        .add_attribute("action", "convert_and_stake")
        .add_attribute("axl_lp_amount_for_operation", amount_to_convert)
        .add_attribute("transmuter_pool", transmuter_pool)
        .add_attribute("noble_pool", noble_pool)
        .add_attribute(
            "provide_liquidity_slippage_tolerance",
            provide_liquidity_slippage_tolerance.to_string(),
        ))
}

fn handle_callback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> Result<Response, ContractError> {
    // Only the contract itself can call callbacks
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("callbacks cannot be invoked externally").into());
    }

    match msg {
        CallbackMsg::SwapCallback {
            transmuter_pool,
            axl_usdc_prev_balance,
            ntrn_prev_balance,
            provide_liquidity_slippage_tolerance,
            caller,
            noble_pool,
            noble_usdc_denom,
        } => handle_swap_callback(
            deps,
            env,
            transmuter_pool,
            axl_usdc_prev_balance,
            ntrn_prev_balance,
            provide_liquidity_slippage_tolerance,
            caller,
            noble_pool,
            noble_usdc_denom,
        ),
        CallbackMsg::StakeCallback {
            noble_usdc_prev_balance,
            ntrn_to_provide,
            provide_liquidity_slippage_tolerance,
            caller,
            noble_pool,
            noble_usdc_denom,
        } => handle_stake_callback(
            deps,
            env,
            noble_usdc_prev_balance,
            ntrn_to_provide,
            provide_liquidity_slippage_tolerance,
            caller,
            noble_pool,
            noble_usdc_denom,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_swap_callback(
    deps: DepsMut,
    env: Env,
    transmuter_pool: String,
    axl_usdc_prev_balance: Uint128,
    ntrn_prev_balance: Uint128,
    provide_liquidity_slippage_tolerance: Decimal,
    caller: String,
    noble_pool: String,
    noble_usdc_denom: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    deps.api.debug(&format!(
        "WASMDEBUG: ntrn balance after withdrawal: {}",
        deps.querier
            .query_balance(env.contract.address.to_string(), NTRN_DENOM.to_string())?
            .amount
    ));
    deps.api.debug(&format!(
        "WASMDEBUG: USDC.axl balance after withdrawal: {}",
        deps.querier
            .query_balance(
                env.contract.address.to_string(),
                config.axl_usdc_denom.clone()
            )?
            .amount
    ));

    let ntrn_withdrawn: Uint128 = deps
        .querier
        .query_balance(env.contract.address.to_string(), NTRN_DENOM.to_string())?
        .amount
        .checked_sub(ntrn_prev_balance)?;
    let axl_usdc_withdrawn: Uint128 = deps
        .querier
        .query_balance(
            env.contract.address.to_string(),
            config.axl_usdc_denom.clone(),
        )?
        .amount
        .checked_sub(axl_usdc_prev_balance)?;
    let noble_usdc_balance: Uint128 = deps
        .querier
        .query_balance(env.contract.address.to_string(), noble_usdc_denom.clone())?
        .amount;

    deps.api.debug(&format!(
        "WASMDEBUG: initial Noble USDC balance: {}",
        noble_usdc_balance
    ));

    Ok(Response::new()
        .add_messages([
            // swap USDC.axl to USDC
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: transmuter_pool,
                msg: to_json_binary(&PairExecuteMsg::Swap {
                    ask_asset_info: None,
                    belief_price: None,
                    max_spread: None,
                    to: None,
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: config.axl_usdc_denom.clone(),
                        },
                        amount: axl_usdc_withdrawn,
                    },
                })?,
                funds: vec![Coin {
                    denom: config.axl_usdc_denom,
                    amount: axl_usdc_withdrawn,
                }],
            }),
            // invoke the next conversion step
            CallbackMsg::StakeCallback {
                noble_usdc_prev_balance: noble_usdc_balance,
                ntrn_to_provide: ntrn_withdrawn,
                provide_liquidity_slippage_tolerance,
                caller,
                noble_pool,
                noble_usdc_denom,
            }
            .to_cosmos_msg(&env)?,
        ])
        .add_attribute("action", "swap_callback")
        .add_attribute("ntrn_withdrawn", ntrn_withdrawn)
        .add_attribute("axl_usdc_withdrawn", axl_usdc_withdrawn))
}

#[allow(clippy::too_many_arguments)]
fn handle_stake_callback(
    deps: DepsMut,
    env: Env,
    noble_usdc_prev_balance: Uint128,
    ntrn_to_provide: Uint128,
    provide_liquidity_slippage_tolerance: Decimal,
    caller: String,
    noble_pool: String,
    noble_usdc_denom: String,
) -> Result<Response, ContractError> {
    deps.api.debug(&format!(
        "WASMDEBUG: Noble USDC balance after swap: {}",
        deps.querier
            .query_balance(env.contract.address.to_string(), noble_usdc_denom.clone())?
            .amount
    ));

    let noble_usdc_swapped: Uint128 = deps
        .querier
        .query_balance(env.contract.address.to_string(), noble_usdc_denom.clone())?
        .amount
        .checked_sub(noble_usdc_prev_balance)?;

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: noble_pool,
            msg: to_json_binary(&PairExecuteMsg::ProvideLiquidity {
                assets: vec![
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: NTRN_DENOM.to_string(),
                        },
                        amount: ntrn_to_provide,
                    },
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: noble_usdc_denom.clone(),
                        },
                        amount: noble_usdc_swapped,
                    },
                ],
                slippage_tolerance: Some(provide_liquidity_slippage_tolerance),
                auto_stake: Some(true),
                receiver: Some(caller),
            })?,
            funds: vec![
                Coin {
                    denom: NTRN_DENOM.to_string(),
                    amount: ntrn_to_provide,
                },
                Coin {
                    denom: noble_usdc_denom,
                    amount: noble_usdc_swapped,
                },
            ],
        }))
        .add_attribute("action", "stake_callback")
        .add_attribute("noble_usdc_staked", noble_usdc_swapped)
        .add_attribute("ntrn_staked", ntrn_to_provide))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    // Set contract to version to latest
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}
