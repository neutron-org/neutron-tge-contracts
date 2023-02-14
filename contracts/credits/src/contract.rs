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
use crate::msg::{
    AllocationResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    WithdrawableAmountResponse,
};
use crate::state::{Allocation, Config, Schedule, ALLOCATIONS, CONFIG};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:credits";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const TOKEN_NAME: &str = "CNTRN";
const TOKEN_SYMBOL: &str = "cuntrn";
const TOKEN_DECIMALS: u8 = 6; // TODO: correct?
const DEPOSITED_SYMBOL: &str = "untrn";

// Zero cliff for vesting. TODO: change?
const VESTING_CLIFF: u64 = 0;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let mut config = Config {
        dao_address: deps.api.addr_validate(&msg.dao_address)?,
        airdrop_address: None,
        lockdrop_address: None,
    };

    if let Some(addr) = msg.airdrop_address {
        config.airdrop_address = Some(deps.api.addr_validate(&addr)?);
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
        } => execute_update_config(deps, env, info, airdrop_address, lockdrop_address),
        ExecuteMsg::AddVesting {
            address,
            amount,
            start_time,
            duration,
        } => execute_add_vesting(deps, env, info, address, amount, start_time, duration),
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, amount)
        }
        ExecuteMsg::Withdraw {} => execute_withdraw(deps, env, info),
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
) -> Result<Response, Cw20ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.dao_address {
        return Err(Cw20ContractError::Unauthorized {});
    }

    config.airdrop_address = Some(deps.api.addr_validate(&airdrop_address)?);
    config.lockdrop_address = Some(deps.api.addr_validate(&lockdrop_address)?);

    Ok(Response::default())
}

pub fn execute_add_vesting(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    address: String,
    amount: Uint128,
    start_time: u64,
    duration: u64,
) -> Result<Response, Cw20ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender
        != config
            .airdrop_address
            .ok_or_else(|| StdError::generic_err("uninitialized"))?
    {
        return Err(Cw20ContractError::Unauthorized {});
    }

    let vested_to = deps.api.addr_validate(&address)?;

    ALLOCATIONS.update(
        deps.storage,
        &vested_to,
        |o: Option<Allocation>| -> Result<Allocation, Cw20ContractError> {
            match o {
                Some(_) => Err(Cw20ContractError::Std(StdError::generic_err(
                    "cannot add vesting two times",
                ))),
                None => Ok(Allocation {
                    allocated_amount: amount,
                    withdrawn_amount: Uint128::zero(),
                    schedule: Schedule {
                        start_time,
                        cliff: VESTING_CLIFF,
                        duration,
                    },
                }),
            }
        },
    )?;

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
                .lockdrop_address
                .ok_or_else(|| StdError::generic_err("uninitialized"))?
    {
        return Err(Cw20ContractError::Unauthorized {});
    }

    ::cw20_base::contract::execute_transfer(deps, env, info, recipient, amount)
}

pub fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, Cw20ContractError> {
    let owner = info.sender.clone();
    let mut allocation: Allocation = ALLOCATIONS.load(deps.storage, &owner)?;
    let withdrawable_amount = compute_withdrawable_amount(
        allocation.allocated_amount,
        allocation.withdrawn_amount,
        &allocation.schedule,
        env.block.time.seconds(),
    )?;

    if withdrawable_amount.is_zero() {
        return Err(Cw20ContractError::Std(StdError::generic_err(
            "nothing to claim yet",
        )));
    }

    allocation.withdrawn_amount += withdrawable_amount;
    ALLOCATIONS.save(deps.storage, &owner, &allocation)?;

    burn_and_send(deps, env, info, owner, withdrawable_amount)
}

// execute_burn is for rewards from lockdrop only, skips vesting
// assume that lockdrop account will call this and then send NTRN's to reward receiver by themselves
pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
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
    // mint in 1:1 proportion to locked ntrn funds
    let untrn_amount = try_find_untrns(info.funds.clone())?;

    let config = CONFIG.load(deps.storage)?;
    let recipient = config.dao_address.to_string();

    ::cw20_base::contract::execute_mint(deps, env, info, recipient, untrn_amount)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::WithdrawableAmount { address } => {
            to_binary(&query_withdrawable_amount(deps, env, address)?)
        }
        QueryMsg::Allocation { address } => to_binary(&query_allocation(deps, address)?),
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
        dao_address: config.dao_address,
        airdrop_address: config.airdrop_address,
        lockdrop_address: config.lockdrop_address,
    })
}

fn query_withdrawable_amount(
    deps: Deps,
    env: Env,
    address: String,
) -> StdResult<WithdrawableAmountResponse> {
    let owner = deps.api.addr_validate(&address)?;
    let allocation: Allocation = ALLOCATIONS.load(deps.storage, &owner)?;
    Ok(WithdrawableAmountResponse {
        amount: compute_withdrawable_amount(
            allocation.allocated_amount,
            allocation.withdrawn_amount,
            &allocation.schedule,
            env.block.time.seconds(),
        )?,
    })
}

fn query_allocation(deps: Deps, address: String) -> StdResult<AllocationResponse> {
    let owner = deps.api.addr_validate(&address)?;
    let allocation = ALLOCATIONS.load(deps.storage, &owner)?;
    Ok(AllocationResponse { allocation })
}

fn try_find_untrns(funds: Vec<Coin>) -> Result<Uint128, Cw20ContractError> {
    let token = funds.first().ok_or_else(|| {
        Cw20ContractError::Std(StdError::generic_err(format!(
            "no untrn funds supplied to lock: {funds:?}"
        )))
    })?;
    if token.denom != DEPOSITED_SYMBOL {
        return Err(Cw20ContractError::Std(StdError::generic_err(
            "need untrn supply to lock",
        )));
    }

    Ok(token.amount)
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

/// Compute the withdrawable based on the current timestamp and the vesting schedule
///
/// The withdrawable amount is vesting amount minus the amount already withdrawn.
pub fn compute_withdrawable_amount(
    allocated_amount: Uint128,
    withdrawn_amount: Uint128,
    vest_schedule: &Schedule,
    current_time: u64, // in seconds
) -> StdResult<Uint128> {
    let f = |schedule: &Schedule| {
        // Before the end of cliff period, no token will be vested/unlocked
        if current_time < schedule.start_time + schedule.cliff {
            Uint128::zero()
            // After the end of cliff, tokens vest/unlock linearly between start time and end time
        } else if current_time < schedule.start_time + schedule.duration {
            allocated_amount.multiply_ratio(current_time - schedule.start_time, schedule.duration)
            // After end time, all tokens are fully vested/unlocked
        } else {
            allocated_amount
        }
    };

    let vested_amount = f(vest_schedule);

    vested_amount
        .checked_sub(withdrawn_amount)
        .map_err(|overflow_err| overflow_err.into())
}

#[cfg(test)]
mod tests {
    use crate::contract::instantiate;
    use crate::msg::InstantiateMsg;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::{Coin, DepsMut, Env, MessageInfo};

    fn simple_instantiate(deps: DepsMut, funds: Option<Vec<Coin>>) -> (MessageInfo, Env) {
        do_instantiate(
            deps,
            "dao_address".to_string(),
            Some("airdrop_address".to_string()),
            Some("lockdrop_address".to_string()),
            funds,
        )
    }

    fn do_instantiate(
        mut deps: DepsMut,
        dao_address: String,
        airdrop_address: Option<String>,
        lockdrop_address: Option<String>,
        funds: Option<Vec<Coin>>,
    ) -> (MessageInfo, Env) {
        let instantiate_msg = InstantiateMsg {
            dao_address,
            airdrop_address,
            lockdrop_address,
        };
        let info = mock_info("creator", &funds.unwrap_or_default());
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
            let (_info, _env) = do_instantiate(
                deps.as_mut(),
                "dao_address".to_string(),
                Some("airdrop_address".to_string()),
                Some("lockdrop_address".to_string()),
                None,
            );
            let config = query_config(deps.as_ref()).unwrap();
            assert_eq!(config.dao_address, "dao_address".to_string());
            assert_eq!(
                config.lockdrop_address,
                Some(Addr::unchecked("lockdrop_address".to_string()))
            );
            assert_eq!(
                config.airdrop_address,
                Some(Addr::unchecked("airdrop_address".to_string()))
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
            let (_info, _env) =
                do_instantiate(deps.as_mut(), "dao_address".to_string(), None, None, None);
            let config = query_config(deps.as_ref()).unwrap();
            assert_eq!(config.dao_address, "dao_address".to_string());
            assert_eq!(config.lockdrop_address, None);
            assert_eq!(config.airdrop_address, None);
        }
    }

    mod update_config {}

    mod add_vesting {
        use crate::contract::tests::simple_instantiate;
        use crate::contract::{execute_add_vesting, VESTING_CLIFF};
        use crate::state::{Schedule, ALLOCATIONS};
        use cosmwasm_std::testing::{mock_dependencies, mock_info};
        use cosmwasm_std::{Addr, StdError, Uint128};
        use cw20_base::ContractError;
        use cw20_base::ContractError::Std;

        #[test]
        fn adds_vesting_for_account_with_correct_settings() {
            let mut deps = mock_dependencies();
            let (_info, env) = simple_instantiate(deps.as_mut(), None);
            let airdrop_info = mock_info("airdrop_address", &[]);

            let res = execute_add_vesting(
                deps.as_mut(),
                env,
                airdrop_info,
                "address".to_string(),
                Uint128::new(100),
                15,
                1000,
            );
            assert!(res.is_ok());

            let allocation = ALLOCATIONS
                .load(&deps.storage, &Addr::unchecked("address"))
                .unwrap();
            assert_eq!(allocation.allocated_amount, Uint128::new(100));
            assert_eq!(allocation.withdrawn_amount, Uint128::new(0));
            assert_eq!(
                allocation.schedule,
                Schedule {
                    start_time: 15,
                    cliff: VESTING_CLIFF,
                    duration: 1000
                }
            );
        }

        #[test]
        fn non_airdrop_addresses_cannot_set_vesting() {
            let mut deps = mock_dependencies();
            let (_info, env) = simple_instantiate(deps.as_mut(), None);
            let airdrop_info = mock_info("non_airdrop_address", &[]);

            let res = execute_add_vesting(
                deps.as_mut(),
                env,
                airdrop_info,
                "address".to_string(),
                Uint128::new(100),
                15,
                1000,
            );
            assert_eq!(res, Err(ContractError::Unauthorized {}));
        }

        #[test]
        fn cannot_add_vesting_twice_to_same_address() {
            let mut deps = mock_dependencies();
            let (_info, env) = simple_instantiate(deps.as_mut(), None);
            let airdrop_info = mock_info("airdrop_address", &[]);

            let res = execute_add_vesting(
                deps.as_mut(),
                env.clone(),
                airdrop_info.clone(),
                "address".to_string(),
                Uint128::new(100),
                15,
                1000,
            );
            assert!(res.is_ok());

            let res = execute_add_vesting(
                deps.as_mut(),
                env,
                airdrop_info,
                "address".to_string(),
                Uint128::new(100),
                15,
                1000,
            );
            assert_eq!(
                res,
                Err(Std(StdError::generic_err("cannot add vesting two times")))
            );
        }
    }

    mod transfer {
        // use super::*;

        #[test]
        fn basic() {}
    }

    mod burn_all {}

    mod burn {}

    mod transfer_from {}

    mod mint {
        use crate::contract::tests::simple_instantiate;
        use crate::contract::{execute_mint, DEPOSITED_SYMBOL};
        use cosmwasm_std::testing::{mock_dependencies, mock_info};
        use cosmwasm_std::{Addr, Coin, StdError, Uint128};
        use cw20_base::state::{BALANCES, TOKEN_INFO};
        use cw20_base::ContractError;

        #[test]
        fn does_not_work_without_funds_sent() {
            let mut deps = mock_dependencies();
            let (_info, env) = simple_instantiate(deps.as_mut(), None);
            let dao_info = mock_info("dao_address", &[]);

            let res = execute_mint(deps.as_mut(), env, dao_info);
            assert_eq!(
                res,
                Err(ContractError::Std(StdError::generic_err(
                    "no untrn funds supplied to lock: []"
                )))
            );
        }

        #[test]
        fn non_dao_cannot_mint() {
            let mut deps = mock_dependencies();
            let (_info, env) = simple_instantiate(deps.as_mut(), None);

            let funds = vec![Coin::new(500, DEPOSITED_SYMBOL)];
            let non_dao_info = mock_info("non dao", &funds);
            let res = execute_mint(deps.as_mut(), env, non_dao_info);
            assert_eq!(res, Err(ContractError::Unauthorized {}));
        }

        #[test]
        fn works_with_ntrn_funds() {
            let mut deps = mock_dependencies();
            let (_info, env) = simple_instantiate(deps.as_mut(), None);

            let funds = vec![Coin::new(500, DEPOSITED_SYMBOL)];
            let dao_info = mock_info("dao_address", &funds);
            let res = execute_mint(deps.as_mut(), env, dao_info);
            assert!(res.is_ok());

            let config = TOKEN_INFO.load(&deps.storage).unwrap();
            assert_eq!(config.total_supply, Uint128::new(500));

            let balance = BALANCES
                .load(&deps.storage, &Addr::unchecked("dao_address"))
                .unwrap();
            assert_eq!(balance, Uint128::new(500));
        }
    }

    mod compute_withdrawable_amount {
        use crate::contract::{compute_withdrawable_amount, VESTING_CLIFF};
        use crate::state::Schedule;
        use cosmwasm_std::Uint128;

        #[test]
        fn works_before_start_time() {
            let now: u64 = 0;
            let schedule = Schedule {
                start_time: 10,
                cliff: VESTING_CLIFF,
                duration: 2592000, // 30 days
            };
            let result =
                compute_withdrawable_amount(Uint128::new(100), Uint128::new(0), &schedule, now);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Uint128::zero());
        }

        #[test]
        fn works_after_start_time() {
            // 0.5 time passed
            let now: u64 = 1296000;
            let schedule = Schedule {
                start_time: 0,
                cliff: VESTING_CLIFF,
                duration: 2592000, // 30 days
            };
            let result =
                compute_withdrawable_amount(Uint128::new(100), Uint128::new(0), &schedule, now);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Uint128::new(50));

            // 0.75 time passed
            let now_2: u64 = 1944000;
            let result =
                compute_withdrawable_amount(Uint128::new(100), Uint128::new(0), &schedule, now_2);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Uint128::new(75));

            // all time passed
            let now_3: u64 = 3000000;
            let result =
                compute_withdrawable_amount(Uint128::new(100), Uint128::new(0), &schedule, now_3);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Uint128::new(100));

            // 0.5 passed and has already withdrawn funds
            let now_3: u64 = 1296000;
            let result =
                compute_withdrawable_amount(Uint128::new(100), Uint128::new(25), &schedule, now_3);
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Uint128::new(25));
        }
    }
}
