#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128};
use cw2::set_contract_version;
// TODO: use correct crate - local or remote?
use ::cw20_base::ContractError as Cw20ContractError;
use cw20_base::contract::create_accounts;
use cw20_base::state::{MinterData, TOKEN_INFO, TokenInfo};
use cw_utils::Expiration;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{Config, CONFIG};

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
    // TODO: call instantiate on cw20 contract
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        when_claimable: msg.when_claimable,
        dao_address: deps.api.addr_validate(&msg.dao_address)?,
        airdrop_address: deps.api.addr_validate(&msg.airdrop_address)?,
        sale_address: deps.api.addr_validate(&msg.sale_contract_address)?,
        lockdrop_address: deps.api.addr_validate(&msg.lockdrop_address)?,
    };
    CONFIG.save(deps.storage, &config)?;

    // create initial accounts
    let total_supply = Uint128::zero();

    if let Some(limit) = msg.get_cap() {
        if total_supply > limit {
            return Err(StdError::generic_err("Initial supply greater than cap").into());
        }
    }

    let mint = Some(MinterData {
        minter: config.dao_address,
        cap: None,
    });

    // store token info
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply,
        mint,
    };
    TOKEN_INFO.save(deps.storage, &data)?;

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
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, amount)
        }
        ExecuteMsg::Burn { amount } => execute_burn(deps, env, info, amount),
        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_increase_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_decrease_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => execute_transfer_from(deps, env, info, owner, recipient, amount),
        ExecuteMsg::Mint { } => execute_mint(deps, env, info),
    }
}

#[entry_point]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

pub fn execute_transfer(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, Cw20ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.airdrop_address
        && info.sender != config.sale_address
        && info.sender != config.lockdrop_address
    {
        return Err(Cw20ContractError::Unauthorized {});
    }
    // todo: check
    ::cw20_base::contract::execute_transfer(deps, env, info, recipient, amount)
}

pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    // TODO: just burn everything, no amount required?
    amount: Uint128,
) -> Result<Response, Cw20ContractError> {
    // TODO: implement
    ::cw20_base::contract::execute_burn(deps, env, info, amount)
}

pub fn execute_increase_allowance(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    spender: String,
    amount: Uint128,
    expires: Option<Expiration>,
) -> Result<Response, Cw20ContractError> {
    // TODO: check
    ::cw20_base::allowances::execute_increase_allowance(deps, env, info, spender, amount, expires)
}

pub fn execute_decrease_allowance(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    spender: String,
    amount: Uint128,
    expires: Option<Expiration>,
) -> Result<Response, Cw20ContractError> {
    // TODO: check
    ::cw20_base::allowances::execute_decrease_allowance(deps, env, info, spender, amount, expires)
}

pub fn execute_transfer_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    recipient: String,
    amount: Uint128,
) -> Result<Response, Cw20ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.lockdrop_address {
        return Err(Cw20ContractError::Unauthorized {});
    }
    // todo: check
    ::cw20_base::allowances::execute_transfer_from(deps, env, info, owner, recipient, amount)
}

pub fn execute_mint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, Cw20ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.dao_address {
        return Err(Cw20ContractError::Unauthorized {});
    }

    // mint in 1:1 proportion to locked untrn funds
    let ntrn_amount = try_find_ntrns(info.clone().funds)?;

    ::cw20_base::contract::execute_mint(deps, env, info, config.dao_address.to_string(), ntrn_amount)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    unimplemented!()
}

#[cfg(test)]
mod tests {}

fn try_find_ntrns(funds: Vec<Coin>) -> Result<Uint128, Cw20ContractError> {
    let token = funds.first().ok_or_else(|| Cw20ContractError::Std(StdError::generic_err("no untrn's supplied to lock")))?;
    // TODO: if we supply untrn's then we mint in uCntrns???
    if &token.denom != "untrn" {
        return Err(Cw20ContractError::Std(StdError::generic_err("no untrn's supplied to lock")));
    }

    Ok(token.amount)
}
