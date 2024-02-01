use crate::msg::InstantiateMsg;
use cosmwasm_std::{entry_point, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;
use vesting_base_pcl::builder::VestingBaseBuilder;
use vesting_base_pcl::error::ContractError;
use vesting_base_pcl::handlers::{execute as base_execute, query as base_query};
use vesting_base_pcl::msg::{ExecuteMsg, QueryMsg};

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
        .build(
            deps,
            msg.owner,
            msg.token_info_manager,
            msg.xyk_vesting_lp_contract,
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
    base_execute(deps, env, info, msg)
}

/// Exposes all the queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    base_query(deps, env, msg)
}
