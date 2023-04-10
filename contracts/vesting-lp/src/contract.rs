use crate::msg::QueryMsg;
use astroport::vesting::{ExecuteMsg, InstantiateMsg, QueryMsg as QueryBase, VestingInfo};
use cosmwasm_std::{
    entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
    Uint128,
};
use cw_storage_plus::Strategy;
use vesting_base::{error::ContractError, state::BaseVesting};

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
    let vest_app = BaseVesting::new(Strategy::EveryBlock);
    vest_app.instantiate(deps, env, info, msg)
}

/// Exposes execute functions available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let vest_app = BaseVesting::new(Strategy::EveryBlock);
    vest_app.execute(deps, env, info, msg)
}

/// Exposes all the queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let vest_app = BaseVesting::new(Strategy::EveryBlock);
    match msg {
        QueryMsg::Config {} => vest_app.query(deps, env, QueryBase::Config {}),
        QueryMsg::VestingAccount { address } => {
            vest_app.query(deps, env, QueryBase::VestingAccount { address })
        }
        QueryMsg::VestingAccounts {
            start_after,
            limit,
            order_by,
        } => vest_app.query(
            deps,
            env,
            QueryBase::VestingAccounts {
                start_after,
                limit,
                order_by,
            },
        ),
        QueryMsg::AvailableAmount { address } => {
            vest_app.query(deps, env, QueryBase::AvailableAmount { address })
        }
        QueryMsg::Timestamp {} => vest_app.query(deps, env, QueryBase::Timestamp {}),
        QueryMsg::VestingManagers {} => vest_app.query(deps, env, QueryBase::VestingManagers {}),
        QueryMsg::UnclaimedAmountAtHeight { address, height } => Ok(to_binary(
            &query_unclaimed_amount_at_height(&vest_app, deps, address, height)?,
        )?),
        QueryMsg::UnclaimedTotalAmountAtHeight { height } => Ok(to_binary(
            &query_total_unclaimed_amount_at_height(&vest_app, deps, height)?,
        )?),
    }
}

/// Returns the available amount of distributed and yet to be claimed tokens for a specific vesting recipient at certain height.
///
/// * **address** vesting recipient for which to return the available amount of tokens to claim.
///
/// * **height** the height we querying unclaimed amount for
pub fn query_unclaimed_amount_at_height(
    base_app: &BaseVesting,
    deps: Deps,
    address: String,
    height: u64,
) -> StdResult<Uint128> {
    let address = deps.api.addr_validate(&address)?;

    let maybe_info = base_app
        .vesting_info
        .may_load_at_height(deps.storage, &address, height)?;
    match &maybe_info {
        Some(info) => compute_unclaimed_amount(info),
        None => Ok(Uint128::zero()),
    }
}

/// Returns the available amount of distributed and yet to be claimed tokens for all the recipients at certain height.
///
/// * **height** the height we querying unclaimed amount for
pub fn query_total_unclaimed_amount_at_height(
    base_app: &BaseVesting,
    deps: Deps,
    height: u64,
) -> StdResult<Uint128> {
    let maybe_state = base_app
        .vesting_state
        .may_load_at_height(deps.storage, height)?;
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
