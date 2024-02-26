use crate::msg::{Cw20HookMsg, ExecuteMsg, InstantiateMsg};
use crate::state::XYK_VESTING_LP_CONTRACT;
use astroport::asset::token_asset_info;
use cosmwasm_std::{
    entry_point, from_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
    Storage, Uint128,
};
use cw2::set_contract_version;
use cw20::Cw20ReceiveMsg;
use vesting_base::error::ContractError;
use vesting_base::handlers::{execute as base_execute, query as base_query};
use vesting_base::handlers::{get_vesting_token, register_vesting_accounts};
use vesting_base::msg::{ExecuteMsg as BaseExecute, QueryMsg};
use vesting_base::state::{vesting_info, vesting_state, CONFIG, VESTING_MANAGERS};
use vesting_base::types::{Config, Extensions, VestingInfo};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "neutron-vesting-lp-pcl";
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
    let owner = deps.api.addr_validate(&msg.owner)?;
    CONFIG.save(
        deps.storage,
        &Config {
            owner,
            vesting_token: Option::from(msg.vesting_token),
            token_info_manager: deps.api.addr_validate(&msg.token_info_manager)?,

            extensions: Extensions {
                historical: true,
                managed: false,
                with_managers: true,
            },
        },
    )?;
    for m in msg.vesting_managers.iter() {
        let ma = deps.api.addr_validate(m)?;
        VESTING_MANAGERS.save(deps.storage, ma, &())?;
    }
    XYK_VESTING_LP_CONTRACT.save(
        deps.storage,
        &deps
            .api
            .addr_validate(&msg.xyk_vesting_lp_contract.clone())?,
    )?;
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
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        _ => {
            let base_msg: BaseExecute = msg.into();
            base_execute(deps, env, info, base_msg)
        }
    }
}

/// Receives a message of type [`Cw20HookMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** CW20 message to process.
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let vesting_token = get_vesting_token(&config)?;
    let sender = info.sender;

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::RegisterVestingAccounts { vesting_accounts } => {
            if !is_sender_whitelisted(
                deps.storage,
                &config,
                &deps.api.addr_validate(sender.as_str())?,
            ) || token_asset_info(sender.clone()) != vesting_token
            {
                return Err(ContractError::Unauthorized {});
            }
            register_vesting_accounts(deps, vesting_accounts, cw20_msg.amount, env.block.height)
        }
        Cw20HookMsg::MigrateXYKLiquidity {
            user_address_raw,
            user_vesting_info,
        } => {
            if !is_sender_xyk_vesting_lp(deps.storage, &deps.api.addr_validate(sender.as_str())?) {
                return Err(ContractError::Unauthorized {});
            }
            handle_migrate_xyk_liquidity(deps, env, user_address_raw, user_vesting_info)
        }
    }
}

fn is_sender_whitelisted(store: &mut dyn Storage, config: &Config, sender: &Addr) -> bool {
    if *sender == config.owner {
        return true;
    }
    let xyk_vesting_lp_contract = XYK_VESTING_LP_CONTRACT.load(store).unwrap();
    if *sender == xyk_vesting_lp_contract {
        return true;
    }
    if VESTING_MANAGERS.has(store, sender.clone()) {
        return true;
    }

    false
}

fn is_sender_xyk_vesting_lp(store: &mut dyn Storage, sender: &Addr) -> bool {
    let xyk_vesting_lp_contract = XYK_VESTING_LP_CONTRACT.load(store).unwrap();
    if *sender == xyk_vesting_lp_contract {
        return true;
    }

    false
}

fn handle_migrate_xyk_liquidity(
    deps: DepsMut,
    env: Env,
    user_addr: Addr,
    user_vesting_info: VestingInfo,
) -> Result<Response, ContractError> {
    let height = env.block.height;
    let config = CONFIG.load(deps.storage)?;

    let mut to_deposit = Uint128::zero();
    for sch in &user_vesting_info.schedules {
        let amount = if let Some(end_point) = &sch.end_point {
            end_point.amount
        } else {
            sch.start_point.amount
        };
        to_deposit = to_deposit.checked_add(amount)?;
    }

    let vesting_info = vesting_info(config.extensions.historical);

    vesting_info.save(deps.storage, user_addr, &user_vesting_info, height)?;

    vesting_state(config.extensions.historical).update::<_, ContractError>(
        deps.storage,
        height,
        |s| {
            let mut state = s.unwrap_or_default();
            state.total_granted = state.total_granted.checked_add(to_deposit)?;
            Ok(state)
        },
    )?;

    Ok(Response::default())
}

/// Exposes all the queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    base_query(deps, env, msg)
}
