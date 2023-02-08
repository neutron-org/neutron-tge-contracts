#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128};
use cw2::set_contract_version;
use cw20_base::contract as cw20_base;
// TODO: use correct crate - local or remote?
use ::cw20_base::ContractError as Cw20ContractError;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, MigrateMsg, InstantiateMsg, QueryMsg};
use crate::state::{CONFIG, Config};


// version info for migration info
const CONTRACT_NAME: &str = "crates.io:credits";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = Config {
        when_claimable: msg.when_claimable,
        dao_address: deps.api.addr_validate(&msg.dao_address)?,
        airdrop_address: deps.api.addr_validate(&msg.airdrop_address)?,
        sale_address: deps.api.addr_validate(&msg.sale_contract_address)?,
        lockdrop_address: deps.api.addr_validate(&msg.lockdrop_address)?,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, Cw20ContractError> {
    match msg {
        ExecuteMsg::Transfer { recipient, amount} => execute_transfer(deps, env, info, recipient, amount),
    }
}

#[entry_point]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

pub fn execute_transfer(deps: DepsMut, env: Env, info: MessageInfo, recipient: String, amount: Uint128) -> Result<Response, Cw20ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.airdrop_address && info.sender != config.sale_address && info.sender != config.lockdrop_address {
        return Err(Cw20ContractError::Unauthorized {});
    }

    cw20_base::execute_transfer(deps, env, info, recipient, amount)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    unimplemented!()
}

#[cfg(test)]
mod tests {}
