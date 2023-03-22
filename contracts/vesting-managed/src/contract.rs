use cosmwasm_std::{Addr, Binary, Deps, DepsMut, entry_point, Env, MessageInfo, Response, StdError, StdResult, SubMsg, Uint128};
use cw_storage_plus::{SnapshotItem, SnapshotMap, Strategy};
use cw_utils::must_pay;
use astroport::asset::AssetInfoExt;

use astroport::asset::AssetInfo;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::vesting::{InstantiateMsg, QueryMsg, VestingInfo, VestingState};
use vesting_base::{error::ContractError, state::BaseVesting};
use vesting_base::state::{CONFIG, Config, OWNERSHIP_PROPOSAL};

use crate::msg::{ExecuteMsg};

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
            let config = CONFIG.load(deps.storage)?;

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
                .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
                .map_err(Into::into)
        }
        ExecuteMsg::RemoveVestingAccounts { vesting_accounts } => {
            remove_vesting_accounts(deps, vesting_accounts, vest_app.vesting_state, vest_app.vesting_info)
        }
    }
}

fn remove_vesting_accounts(
    deps: DepsMut,
    vesting_accounts: Vec<String>,
    vesting_state: SnapshotItem<'static, VestingState>,
    vesting_info: SnapshotMap<'static, &'static Addr, VestingInfo>
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut response = Response::new();

    // For each vesting account, calculate the amount of tokens to claw back (unclaimed + still
    // vesting), remove the vesting information from the storage, and decrease the total granted
    // metric.
    for vesting_account in vesting_accounts {
        let mut amount_to_claw_back = Uint128::zero();

        let account_address = deps.api.addr_validate(&vesting_account)?;
        if let Some(account_info) =
            vesting_info.may_load(deps.storage, &account_address)?
        {
            for sch in account_info.schedules {
                amount_to_claw_back = amount_to_claw_back.checked_add(sch.start_point.amount)?;
                if let Some(end_point) = sch.end_point {
                    amount_to_claw_back = amount_to_claw_back.checked_add(end_point.amount)?;
                }
            }

            amount_to_claw_back = amount_to_claw_back.checked_sub(account_info.released_amount)?;

            let transfer_msg = config.vesting_token.with_balance(amount_to_claw_back).into_msg(
                &deps.querier,
                config.owner.clone(),
            )?;
            response = response.add_submessage(SubMsg::new(transfer_msg));

            vesting_info
                .remove(deps.storage, &info.sender, env.block.height)?;
            vesting_state
                .update::<_, ContractError>(deps.storage, env.block.height, |s| {
                    let mut state = s.ok_or(ContractError::AmountIsNotAvailable {})?;
                    state.total_granted = state.total_granted.checked_sub(amount_to_claw_back)?;
                    Ok(state)
                })?;
        }
    }

    Ok(Response::default())
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

