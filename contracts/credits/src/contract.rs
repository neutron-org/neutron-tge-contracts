use ::cw20_base::ContractError as Cw20ContractError;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Uint128,
};
use cw2::set_contract_version;
use cw20_base::state as Cw20State;
use cw_utils::Expiration;

use crate::error::ContractError;
use crate::msg::{ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
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

    let mut config = Config {
        when_claimable: msg.when_claimable,
        dao_address: deps.api.addr_validate(&msg.dao_address)?,
        airdrop_address: None,
        sale_address: None,
        lockdrop_address: None,
    };

    if let Some(addr) = msg.airdrop_address {
        config.airdrop_address = Some(deps.api.addr_validate(&addr)?);
    }
    if let Some(addr) = msg.sale_address {
        config.sale_address = Some(deps.api.addr_validate(&addr)?);
    }
    if let Some(addr) = msg.lockdrop_address {
        config.lockdrop_address = Some(deps.api.addr_validate(&addr)?);
    }
    CONFIG.save(deps.storage, &config)?;

    // store token info
    let info = Cw20State::TokenInfo {
        name: TOKEN_NAME.to_string(),
        symbol: TOKEN_SYMBOL.to_string(),
        decimals: TOKEN_DECIMALS,
        total_supply: Uint128::zero(),
        mint: Some(Cw20State::MinterData {
            minter: config.dao_address,
            cap: None,
        }),
    };
    Cw20State::TOKEN_INFO.save(deps.storage, &info)?;

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
        ExecuteMsg::UpdateConfig {
            airdrop_address,
            lockdrop_address,
            sale_address,
        } => execute_update_config(
            deps,
            env,
            info,
            airdrop_address,
            lockdrop_address,
            sale_address,
        ),
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, amount)
        }
        ExecuteMsg::BurnAll {} => execute_burn_all(deps, env, info),
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
        ExecuteMsg::Mint {} => execute_mint(deps, env, info),
    }
}

#[entry_point]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    airdrop_address: String,
    lockdrop_address: String,
    sale_address: String,
) -> Result<Response, Cw20ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.dao_address {
        return Err(Cw20ContractError::Unauthorized {});
    }

    config.airdrop_address = Some(deps.api.addr_validate(&airdrop_address)?);
    config.lockdrop_address = Some(deps.api.addr_validate(&lockdrop_address)?);
    config.sale_address = Some(deps.api.addr_validate(&sale_address)?);

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

    if info.sender
        != config
            .airdrop_address
            .ok_or_else(|| StdError::generic_err("uninitialized"))?
        && info.sender
            != config
                .sale_address
                .ok_or_else(|| StdError::generic_err("uninitialized"))?
        && info.sender
            != config
                .lockdrop_address
                .ok_or_else(|| StdError::generic_err("uninitialized"))?
    {
        return Err(Cw20ContractError::Unauthorized {});
    }

    ::cw20_base::contract::execute_transfer(deps, env, info, recipient, amount)
}

pub fn execute_burn_all(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, Cw20ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let sender = info.sender.clone();

    if too_early(&env, &config) {
        return Err(Cw20ContractError::Std(StdError::generic_err(format!(
            "cannot claim until {}",
            config.when_claimable
        ))));
    }
    let balance = cw20_base::state::BALANCES
        .may_load(deps.storage, &sender)?
        .unwrap_or_default();

    burn_and_send(deps, env, info, sender, balance)
}

pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128, // used only for airdrop address
) -> Result<Response, Cw20ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let sender = info.sender.clone();

    if sender
        != config
            .lockdrop_address
            .ok_or_else(|| StdError::generic_err("uninitialized"))?
    {
        return Err(Cw20ContractError::Unauthorized {});
    }

    burn_and_send(deps, env, info, sender, amount)
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
    if info.sender
        != config
            .lockdrop_address
            .ok_or_else(|| StdError::generic_err("uninitialized"))?
    {
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
    let recipient = config.dao_address.to_string();

    ::cw20_base::contract::execute_mint(deps, env, info, recipient, untrn_amount)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Balance { address } => {
            to_binary(&::cw20_base::contract::query_balance(deps, address)?)
        }
        QueryMsg::TokenInfo {} => to_binary(&::cw20_base::contract::query_token_info(deps)?),
        QueryMsg::Minter {} => to_binary(&::cw20_base::contract::query_minter(deps)?),
        QueryMsg::Allowance { owner, spender } => to_binary(
            &::cw20_base::allowances::query_allowance(deps, owner, spender)?,
        ),
        QueryMsg::AllAllowances {
            owner,
            start_after,
            limit,
        } => to_binary(&::cw20_base::enumerable::query_owner_allowances(
            deps,
            owner,
            start_after,
            limit,
        )?),
        QueryMsg::AllSpenderAllowances {
            spender,
            start_after,
            limit,
        } => to_binary(&::cw20_base::enumerable::query_spender_allowances(
            deps,
            spender,
            start_after,
            limit,
        )?),
        QueryMsg::AllAccounts { start_after, limit } => to_binary(
            &::cw20_base::enumerable::query_all_accounts(deps, start_after, limit)?,
        ),
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        when_claimable: config.when_claimable,
        dao_address: config.dao_address,
        airdrop_address: config.airdrop_address,
        sale_address: config.sale_address,
        lockdrop_address: config.lockdrop_address,
    })
}

fn try_find_untrns(funds: Vec<Coin>) -> Result<Uint128, Cw20ContractError> {
    let token = funds.first().ok_or_else(|| {
        Cw20ContractError::Std(StdError::generic_err("no untrn's supplied to lock"))
    })?;
    if token.denom != DEPOSITED_SYMBOL {
        return Err(Cw20ContractError::Std(StdError::generic_err(
            "no untrns supplied to lock",
        )));
    }

    Ok(token.amount)
}

fn too_early(env: &Env, config: &Config) -> bool {
    env.block.time < config.when_claimable
}

fn burn_and_send(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    amount: Uint128,
) -> Result<Response, Cw20ContractError> {
    let burn_response = ::cw20_base::contract::execute_burn(deps, env, info, amount)?;
    let send = BankMsg::Send {
        to_address: sender.to_string(),
        amount: vec![Coin::new(amount.u128(), DEPOSITED_SYMBOL)],
    };

    Ok(burn_response.add_message(send))
}

#[cfg(test)]
mod tests {
    use crate::contract::instantiate;
    use crate::msg::InstantiateMsg;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::{DepsMut, Env, MessageInfo, Timestamp};

    fn do_instantiate(
        mut deps: DepsMut,
        when_claimable: Timestamp,
        dao_address: String,
        airdrop_address: Option<String>,
        sale_address: Option<String>,
        lockdrop_address: Option<String>,
    ) -> (MessageInfo, Env) {
        let instantiate_msg = InstantiateMsg {
            when_claimable,
            dao_address,
            airdrop_address,
            sale_address,
            lockdrop_address,
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let res = instantiate(deps.branch(), env.clone(), info.clone(), instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        (info, env)
    }

    mod instantiate {
        use super::*;
        use crate::contract::{query_config, TOKEN_DECIMALS, TOKEN_NAME, TOKEN_SYMBOL};
        use cosmwasm_std::testing::mock_dependencies;
        use cosmwasm_std::{Addr, Uint128};
        use cw20_base::contract::{query_minter, query_token_info};
        use cw20_base::enumerable::query_all_accounts;

        #[test]
        fn basic() {
            let mut deps = mock_dependencies();
            let timestamp = Timestamp::default();
            let (_info, _env) = do_instantiate(
                deps.as_mut(),
                timestamp,
                "dao_address".to_string(),
                Some("airdrop_address".to_string()),
                Some("sale_address".to_string()),
                Some("lockdrop_address".to_string()),
            );
            let config = query_config(deps.as_ref()).unwrap();
            assert_eq!(config.when_claimable, timestamp);
            assert_eq!(config.dao_address, "dao_address".to_string());
            assert_eq!(
                config.lockdrop_address,
                Some(Addr::unchecked("lockdrop_address".to_string()))
            );
            assert_eq!(
                config.airdrop_address,
                Some(Addr::unchecked("airdrop_address".to_string()))
            );
            assert_eq!(
                config.sale_address,
                Some(Addr::unchecked("sale_address".to_string()))
            );

            // no accounts since we don't mint anything
            assert_eq!(
                query_all_accounts(deps.as_ref(), None, None)
                    .unwrap()
                    .accounts
                    .len(),
                0
            );
            // minter is dao account
            assert_eq!(
                query_minter(deps.as_ref()).unwrap().unwrap().minter,
                "dao_address".to_string()
            );

            // Write TOKEN_INFO
            let token_info = query_token_info(deps.as_ref()).unwrap();
            assert_eq!(token_info.decimals, TOKEN_DECIMALS);
            assert_eq!(token_info.name, TOKEN_NAME);
            assert_eq!(token_info.symbol, TOKEN_SYMBOL);
            assert_eq!(token_info.total_supply, Uint128::zero());
        }

        #[test]
        fn works_without_initial_addresses() {
            let mut deps = mock_dependencies();
            let timestamp = Timestamp::default();
            let (_info, _env) = do_instantiate(
                deps.as_mut(),
                timestamp,
                "dao_address".to_string(),
                None,
                None,
                None,
            );
            let config = query_config(deps.as_ref()).unwrap();
            assert_eq!(config.when_claimable, timestamp);
            assert_eq!(config.dao_address, "dao_address".to_string());
            assert_eq!(config.lockdrop_address, None);
            assert_eq!(config.airdrop_address, None);
            assert_eq!(config.sale_address, None);
        }
    }

    mod transfer {
        // use super::*;

        #[test]
        fn basic() {}
    }
}
