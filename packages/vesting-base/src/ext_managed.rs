use crate::error::{ext_unsupported_err, ContractError};
use crate::handlers::get_vesting_token;
use crate::msg::{ExecuteMsgManaged, QueryMsgManaged};
use crate::state::{vesting_info, vesting_state, CONFIG};
use astroport::asset::AssetInfoExt;
use cosmwasm_std::{attr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128};

/// Contains the managed extension check and routing of the message.
pub(crate) fn handle_execute_managed_msg(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsgManaged,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if !config.extensions.managed {
        return Err(ext_unsupported_err("managed").into());
    }

    match msg {
        ExecuteMsgManaged::RemoveVestingAccounts {
            vesting_accounts,
            clawback_account,
        } => remove_vesting_accounts(deps, env, info, vesting_accounts, clawback_account),
    }
}

/// Contains the managed extension check and routing of the message.
pub(crate) fn handle_query_managed_msg(
    deps: Deps,
    _env: Env,
    _msg: QueryMsgManaged,
) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    if !config.extensions.managed {
        return Err(ext_unsupported_err("managed"));
    }

    // empty handler kept for uniformity with other extensions
    unimplemented!()
}

#[allow(clippy::too_many_arguments)]
fn remove_vesting_accounts(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    vesting_accounts: Vec<String>,
    clawback_account: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }
    let vesting_token = get_vesting_token(&config)?;

    let mut response = Response::new();

    let clawback_address = deps.api.addr_validate(&clawback_account)?;

    // For each vesting account, calculate the amount of tokens to claw back (unclaimed + still
    // vesting), transfer the required amount to the owner, remove the vesting information
    // from the storage, and decrease the total granted metric.
    for vesting_account in vesting_accounts {
        let account_address = deps.api.addr_validate(&vesting_account)?;

        let config = CONFIG.load(deps.storage)?;
        let vesting_info = vesting_info(config.extensions.historical);
        if let Some(account_info) = vesting_info.may_load(deps.storage, account_address.clone())? {
            let mut total_granted_for_user = Uint128::zero();
            for sch in account_info.schedules {
                if let Some(end_point) = sch.end_point {
                    total_granted_for_user =
                        total_granted_for_user.checked_add(end_point.amount)?;
                } else {
                    total_granted_for_user =
                        total_granted_for_user.checked_add(sch.start_point.amount)?;
                }
            }

            let amount_to_claw_back =
                total_granted_for_user.checked_sub(account_info.released_amount)?;

            let transfer_msg = vesting_token
                .with_balance(amount_to_claw_back)
                .into_msg(&deps.querier, clawback_address.clone())?;
            response = response.add_message(transfer_msg);

            vesting_state(config.extensions.historical).update::<_, ContractError>(
                deps.storage,
                env.block.height,
                |s| {
                    // Here we choose the "forget about everything" strategy. E.g., if we granted a user
                    // 300 tokens, and they claimed 150 tokens, the vesting state is
                    // { total_granted: 300, total_released: 150 }.
                    // If after that we remove the user's vesting account, we set the vesting state to
                    // { total_granted: 0, total_released: 0 }.
                    //
                    // If we decided to set it to { total_granted: 150, total_released: 150 }., the
                    // .total_released value of the vesting state would not be equal to the sum of the
                    // .released_amount values of all registered accounts.
                    let mut state = s.ok_or(ContractError::AmountIsNotAvailable {})?;
                    state.total_granted =
                        state.total_granted.checked_sub(total_granted_for_user)?;
                    state.total_released = state
                        .total_released
                        .checked_sub(account_info.released_amount)?;
                    Ok(state)
                },
            )?;
            vesting_info.remove(deps.storage, account_address, env.block.height)?;
        }
    }

    Ok(response.add_attributes(vec![
        attr("action", "remove_vesting_accounts"),
        attr("sender", &info.sender),
    ]))
}
