use crate::error::{ext_unsupported_err, ContractError};
use crate::msg::{ExecuteMsgHistorical, QueryMsgHistorical};
use crate::state::{vesting_info, vesting_state, CONFIG};
use crate::types::VestingInfo;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128,
};

/// Contains the historical extension check and routing of the message.
pub(crate) fn handle_execute_historical_msg(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsgHistorical,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if !config.extensions.historical {
        return Err(ext_unsupported_err("historical").into());
    }

    unimplemented!()
}

/// Contains the historical extension check and routing of the message.
pub(crate) fn handle_query_historical_msg(
    deps: Deps,
    _env: Env,
    msg: QueryMsgHistorical,
) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    if !config.extensions.historical {
        return Err(ext_unsupported_err("historical"));
    }

    match msg {
        QueryMsgHistorical::UnclaimedAmountAtHeight { address, height } => {
            to_binary(&query_unclaimed_amount_at_height(deps, address, height)?)
        }
        QueryMsgHistorical::UnclaimedTotalAmountAtHeight { height } => {
            to_binary(&query_total_unclaimed_amount_at_height(deps, height)?)
        }
    }
}

/// Returns the available amount of distributed and yet to be claimed tokens for a specific vesting recipient at certain height.
///
/// * **address** vesting recipient for which to return the available amount of tokens to claim.
///
/// * **height** the height we querying unclaimed amount for
fn query_unclaimed_amount_at_height(
    deps: Deps,
    address: String,
    height: u64,
) -> StdResult<Uint128> {
    let address = deps.api.addr_validate(&address)?;

    let config = CONFIG.load(deps.storage)?;
    let maybe_info = vesting_info(config.extensions.historical).may_load_at_height(
        deps.storage,
        address,
        height,
    )?;
    match &maybe_info {
        Some(info) => compute_unclaimed_amount(info),
        None => Ok(Uint128::zero()),
    }
}

/// Returns the available amount of distributed and yet to be claimed tokens for all the recipients at certain height.
///
/// * **height** the height we querying unclaimed amount for
fn query_total_unclaimed_amount_at_height(deps: Deps, height: u64) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let maybe_state =
        vesting_state(config.extensions.historical).may_load_at_height(deps.storage, height)?;
    match &maybe_state {
        Some(info) => Ok(info.total_granted.checked_sub(info.total_released)?),
        None => Ok(Uint128::zero()),
    }
}

/// Computes the amount of distributed and yet unclaimed tokens for a specific vesting recipient at certain height.
/// Returns the computed amount if the operation is successful.
///
/// * **vesting_info** vesting schedules for which to compute the amount of tokens
/// that are vested and can be claimed by the recipient.
fn compute_unclaimed_amount(vesting_info: &VestingInfo) -> StdResult<Uint128> {
    let mut available_amount: Uint128 = Uint128::zero();
    for sch in &vesting_info.schedules {
        if let Some(end_point) = &sch.end_point {
            available_amount = available_amount.checked_add(end_point.amount)?;
        } else {
            available_amount = available_amount.checked_add(sch.start_point.amount)?;
        }
    }

    available_amount
        .checked_sub(vesting_info.released_amount)
        .map_err(StdError::from)
}
