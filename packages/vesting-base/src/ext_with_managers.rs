use crate::error::{ext_unsupported_err, ContractError};
use crate::msg::{ExecuteMsgWithManagers, QueryMsgWithManagers};
use crate::state::{CONFIG, VESTING_MANAGERS};
use cosmwasm_std::{
    attr, to_json_binary, Addr, Attribute, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult,
};

/// Contains the with_managers extension check and routing of the message.
pub(crate) fn handle_execute_with_managers_msg(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsgWithManagers,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if !config.extensions.with_managers {
        return Err(ext_unsupported_err("with_managers").into());
    }

    match msg {
        ExecuteMsgWithManagers::AddVestingManagers { managers } => {
            add_vesting_managers(deps, env, info, managers)
        }
        ExecuteMsgWithManagers::RemoveVestingManagers { managers } => {
            remove_vesting_managers(deps, env, info, managers)
        }
    }
}

/// Contains the with_managers extension check and routing of the message.
pub(crate) fn handle_query_managers_msg(
    deps: Deps,
    _env: Env,
    msg: QueryMsgWithManagers,
) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    if !config.extensions.with_managers {
        return Err(ext_unsupported_err("with_managers"));
    }

    match msg {
        QueryMsgWithManagers::VestingManagers {} => to_json_binary(&query_vesting_managers(deps)?),
    }
}

/// Adds new vesting managers, which have a permission to add/remove vesting schedule
///
/// * **managers** list of accounts to be added to the whitelist.
fn add_vesting_managers(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    managers: Vec<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }
    let mut attrs: Vec<Attribute> = vec![];
    for m in managers {
        let ma = deps.api.addr_validate(&m)?;
        if !VESTING_MANAGERS.has(deps.storage, ma.clone()) {
            VESTING_MANAGERS.save(deps.storage, ma, &())?;
            attrs.push(attr("vesting_manager", &m))
        }
    }
    Ok(Response::new()
        .add_attribute("action", "add_vesting_managers")
        .add_attributes(attrs))
}

/// Removes new vesting managers from the whitelist
///
/// * **managers** list of accounts to be removed from the whitelist.
fn remove_vesting_managers(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    managers: Vec<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }
    let mut attrs: Vec<Attribute> = vec![];
    for m in managers {
        let ma = deps.api.addr_validate(&m)?;
        if VESTING_MANAGERS.has(deps.storage, ma.clone()) {
            VESTING_MANAGERS.remove(deps.storage, ma);
            attrs.push(attr("vesting_manager", &m))
        }
    }
    Ok(Response::new()
        .add_attribute("action", "remove_vesting_managers")
        .add_attributes(attrs))
}

/// Returns a list of vesting schedules using a [`VestingAccountsResponse`] object.
fn query_vesting_managers(deps: Deps) -> StdResult<Vec<Addr>> {
    let managers = VESTING_MANAGERS
        .keys(deps.storage, None, None, Order::Ascending)
        .collect::<Result<Vec<Addr>, StdError>>()?;
    Ok(managers)
}
