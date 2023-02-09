use ::cw20_base::ContractError as Cw20ContractError;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128,
};
use cw2::set_contract_version;
use cw20_base::state::{MinterData, TokenInfo, TOKEN_INFO};
use cw_utils::Expiration;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{Config, CONFIG};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:credits";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const TOKEN_NAME: &str = "CNTRN";
const TOKEN_SYMBOL: &str = "cntrn";
const TOKEN_DECIMALS: u8 = 8; // TODO: correct?
const DEPOSITED_SYMBOL: &str = "untrn";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        when_claimable: msg.when_claimable,
        dao_address: deps.api.addr_validate(&msg.dao_address)?,
        airdrop_address: deps.api.addr_validate(&msg.airdrop_address)?,
        sale_address: deps.api.addr_validate(&msg.sale_contract_address)?,
        lockdrop_address: deps.api.addr_validate(&msg.lockdrop_address)?,
    };
    CONFIG.save(deps.storage, &config)?;

    // store token info
    let info = TokenInfo {
        name: TOKEN_NAME.to_string(),
        symbol: TOKEN_SYMBOL.to_string(),
        decimals: TOKEN_DECIMALS,
        total_supply: Uint128::zero(),
        mint: Some(MinterData {
            minter: config.dao_address,
            cap: None,
        }),
    };
    TOKEN_INFO.save(deps.storage, &info)?;

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
        ExecuteMsg::Burn {} => execute_burn(deps, env, info),
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
        ExecuteMsg::Mint {} => execute_mint(deps, env, info),
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

    ::cw20_base::contract::execute_transfer(deps, env, info, recipient, amount)
}

pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, Cw20ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if env.block.time < config.when_claimable {
        return Err(Cw20ContractError::Std(StdError::generic_err(format!(
            "cannot claim until {}",
            config.when_claimable
        ))));
    }

    let sender = info.sender.clone();

    // burn all balance
    let balance = cw20_base::state::BALANCES
        .may_load(deps.storage, &sender)?
        .unwrap_or_default();

    let burn_response = ::cw20_base::contract::execute_burn(deps, env, info, balance)?;
    let send = BankMsg::Send {
        to_address: sender.to_string(),
        amount: vec![Coin::new(balance.u128(), DEPOSITED_SYMBOL)],
    };

    Ok(burn_response.add_message(send))
}

pub fn execute_increase_allowance(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    spender: String,
    amount: Uint128,
    expires: Option<Expiration>,
) -> Result<Response, Cw20ContractError> {
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

    ::cw20_base::allowances::execute_transfer_from(deps, env, info, owner, recipient, amount)
}

pub fn execute_mint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, Cw20ContractError> {
    // mint in 1:1 proportion to locked untrn funds
    let untrn_amount = try_find_untrns(info.clone().funds)?;

    let config = CONFIG.load(deps.storage)?;

    ::cw20_base::contract::execute_mint(
        deps,
        env,
        info,
        config.dao_address.to_string(),
        untrn_amount,
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps, _env: Env, _msg: QueryMsg) -> StdResult<Binary> {
    unimplemented!()
}

fn try_find_untrns(funds: Vec<Coin>) -> Result<Uint128, Cw20ContractError> {
    let token = funds.first().ok_or_else(|| {
        Cw20ContractError::Std(StdError::generic_err("no untrn's supplied to lock"))
    })?;
    if token.denom != DEPOSITED_SYMBOL {
        return Err(Cw20ContractError::Std(StdError::generic_err(
            "no untrn's supplied to lock",
        )));
    }

    Ok(token.amount)
}

#[cfg(test)]
mod tests {}
