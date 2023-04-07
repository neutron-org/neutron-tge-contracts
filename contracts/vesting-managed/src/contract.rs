use crate::msg::InstantiateMsg;
use cosmwasm_std::{entry_point, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;
use vesting_base::{
    builder::VestingBaseBuilder,
    error::ContractError,
    handlers::{execute as base_execute, query as base_query},
    msg::{ExecuteMsg, QueryMsg},
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "neutron-managed-vesting";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

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
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    VestingBaseBuilder::default()
        .managed()
        .build(deps, msg.owner, msg.token_info_manager)?;
    Ok(Response::default())
}

/// Exposes execute functions available in the contract.
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    base_execute(deps, env, info, msg)
}

/// Exposes all the queries available in the contract.
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    base_query(deps, env, msg)
}
