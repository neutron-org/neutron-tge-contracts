use cosmwasm_std::{
    attr, entry_point, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, SubMsg, Uint128,
};
use cw_storage_plus::{SnapshotItem, SnapshotMap, Strategy};
use cw_utils::must_pay;

use astroport::asset::AssetInfo;
use astroport::asset::AssetInfoExt;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::vesting::{InstantiateMsg, QueryMsg, VestingInfo, VestingState};
use vesting_base::state::Config;
use vesting_base::{error::ContractError, state::BaseVesting};

use crate::msg::ExecuteMsg;

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
    let vest_app = BaseVesting::new(Strategy::Never);
    vest_app.instantiate(deps, env, info, msg)
}

/// Exposes execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::Claim { recipient, amount }** Claims vested tokens and transfers them to the vesting recipient.
///
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes it
/// depending on the received template.
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let vest_app = BaseVesting::new(Strategy::Never);

    match msg {
        ExecuteMsg::Claim { recipient, amount } => {
            vest_app.claim(deps, env, info, recipient, amount)
        }
        ExecuteMsg::Receive(msg) => vest_app.receive_cw20(deps, env, info, msg),
        ExecuteMsg::RegisterVestingAccounts { vesting_accounts } => {
            let config = vest_app.config.load(deps.storage)?;

            match &config.vesting_token {
                AssetInfo::NativeToken { denom } if info.sender == config.owner => {
                    let amount = must_pay(&info, denom)?;
                    vest_app.register_vesting_accounts(
                        deps,
                        vesting_accounts,
                        amount,
                        env.block.height,
                    )
                }
                _ => Err(ContractError::Unauthorized {}),
            }
        }
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config: Config = vest_app.config.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                &vest_app.ownership_proposal,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = vest_app.config.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, &vest_app.ownership_proposal)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => claim_ownership(
            deps,
            info,
            env,
            &vest_app.ownership_proposal,
            |deps, new_owner| {
                vest_app
                    .config
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.owner = new_owner;
                        Ok(v)
                    })?;

                Ok(())
            },
        )
        .map_err(Into::into),
        ExecuteMsg::RemoveVestingAccounts {
            vesting_accounts,
            clawback_account,
        } => {
            let config = vest_app.config.load(deps.storage)?;
            remove_vesting_accounts(
                deps,
                info,
                env,
                config,
                vesting_accounts,
                vest_app.vesting_state,
                vest_app.vesting_info,
                clawback_account,
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn remove_vesting_accounts(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    config: Config,
    vesting_accounts: Vec<String>,
    vesting_state: SnapshotItem<'static, VestingState>,
    vesting_info: SnapshotMap<'static, &'static Addr, VestingInfo>,
    clawback_account: String,
) -> Result<Response, ContractError> {
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let mut response = Response::new();

    let clawback_address = deps.api.addr_validate(&clawback_account)?;

    // For each vesting account, calculate the amount of tokens to claw back (unclaimed + still
    // vesting), transfer the required amount to the owner, remove the vesting information
    // from the storage, and decrease the total granted metric.
    for vesting_account in vesting_accounts {
        let account_address = deps.api.addr_validate(&vesting_account)?;

        if let Some(account_info) = vesting_info.may_load(deps.storage, &account_address)? {
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

            let transfer_msg = config
                .vesting_token
                .with_balance(amount_to_claw_back)
                .into_msg(&deps.querier, clawback_address.clone())?;
            response = response.add_submessage(SubMsg::new(transfer_msg));

            vesting_state.update::<_, ContractError>(deps.storage, env.block.height, |s| {
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
                state.total_granted = state.total_granted.checked_sub(total_granted_for_user)?;
                state.total_released = state
                    .total_released
                    .checked_sub(account_info.released_amount)?;
                Ok(state)
            })?;
            vesting_info.remove(deps.storage, &account_address.clone(), env.block.height)?;
        }
    }

    Ok(response.add_attributes(vec![
        attr("action", "remove_vesting_accounts"),
        attr("sender", &info.sender),
    ]))
}

/// Exposes all the queries available in the contract.
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns the contract configuration in an object of type [`Config`].
///
/// * **QueryMsg::VestingAccount { address }** Returns information about the vesting schedules that have a specific vesting recipient.
///
/// * **QueryMsg::VestingAccounts {
///             start_after,
///             limit,
///             order_by,
///         }** Returns a list of vesting schedules together with their vesting recipients.
///
/// * **QueryMsg::AvailableAmount { address }** Returns the available amount of tokens that can be claimed by a specific vesting recipient.
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let vest_app = BaseVesting::new(Strategy::Never);
    vest_app.query(deps, env, msg)
}
