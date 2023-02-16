use ::cw20_base::ContractError as Cw20ContractError;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Uint128,
};
use cw2::set_contract_version;
use cw20::BalanceResponse;
use cw20_base::state as Cw20State;
use cw20_base::state::BALANCES;
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
const TOKEN_DECIMALS: u8 = 6;
const DEPOSITED_SYMBOL: &str = "untrn";

// Zero cliff for vesting. Before the schedule.start_time + schedule.cliff vesting does not start.
// TODO: change?
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
        when_withdrawable: msg.when_withdrawable,
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
        ExecuteMsg::BurnFrom { owner, amount } => execute_burn_from(deps, env, info, owner, amount),
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

    CONFIG.save(deps.storage, &config)?;

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
                .lockdrop_address // TODO: why we have lockdrop_address access here? Since we do not have funds on lockdrop balance
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
    let config = CONFIG.load(deps.storage)?;
    if config.when_withdrawable > env.block.time {
        return Err(Cw20ContractError::Std(StdError::generic_err(
            "too early to claim",
        )));
    }

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

    // because we have lockdrop rewards that skip vesting, we can get withdrawable amount greater than the current balance
    // so we need to withdraw not more than the current balance
    let actual_balance = BALANCES.load(deps.storage, &owner)?;
    let to_withdraw = withdrawable_amount.min(actual_balance);

    allocation.withdrawn_amount += to_withdraw;
    ALLOCATIONS.save(deps.storage, &owner, &allocation)?;

    burn_and_send(deps, env, info, to_withdraw)
}

// execute_burn is for airdrop account that will burn through all unclaimed tokens
pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, Cw20ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender
        != config
            .airdrop_address
            .ok_or_else(|| StdError::generic_err("uninitialized"))?
    {
        return Err(Cw20ContractError::Unauthorized {});
    }

    burn_and_send(deps, env, info, amount)
}

pub fn execute_burn_from(
    deps: DepsMut,
    env: Env,
    mut info: MessageInfo,
    owner: String,
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

    // use it as analog of burn_from, but we skip allowance step since we only allow it for lockdrop address
    info.sender = deps.api.addr_validate(&owner)?;

    burn_and_send(deps, env, info, amount)
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
    let recipient = config
        .airdrop_address
        .ok_or_else(|| StdError::generic_err("uninitialized"))?;

    ::cw20_base::contract::execute_mint(deps, env, info, recipient.to_string(), untrn_amount)
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
        QueryMsg::BalanceAtHeight { address, height } => {
            to_binary(&query_balance_at_height(deps, address, height)?)
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
        when_withdrawable: config.when_withdrawable,
    })
}

fn query_balance_at_height(deps: Deps, address: String, height: u64) -> StdResult<BalanceResponse> {
    let balance = BALANCES
        .may_load_at_height(deps.storage, &deps.api.addr_validate(&address)?, height)?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

fn query_withdrawable_amount(
    deps: Deps,
    env: Env,
    address: String,
) -> StdResult<WithdrawableAmountResponse> {
    let owner = deps.api.addr_validate(&address)?;
    let allocation: Allocation = ALLOCATIONS.load(deps.storage, &owner)?;
    let withdrawable_amount = compute_withdrawable_amount(
        allocation.allocated_amount,
        allocation.withdrawn_amount,
        &allocation.schedule,
        env.block.time.seconds(),
    )?;
    // because we have lockdrop rewards that skip vesting, we can get withdrawable amount greater than the current balance
    // so we need to withdraw not more than the current balance
    let actual_balance = BALANCES.load(deps.storage, &owner)?;
    let amount = withdrawable_amount.min(actual_balance);

    Ok(WithdrawableAmountResponse { amount })
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

// burns cuntrns and send untrns to the sender
fn burn_and_send(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, Cw20ContractError> {
    let to_address = info.sender.to_string();
    let burn_response = ::cw20_base::contract::execute_burn(deps, env, info, amount)?;
    let send = BankMsg::Send {
        to_address,
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
    use crate::contract::{execute_add_vesting, instantiate};
    use crate::msg::InstantiateMsg;
    use cosmwasm_std::testing::{mock_env, mock_info};
    use cosmwasm_std::{Coin, DepsMut, Env, MessageInfo, Timestamp, Uint128};

    fn _do_simple_instantiate(deps: DepsMut, funds: Option<Vec<Coin>>) -> (MessageInfo, Env) {
        _do_instantiate(
            deps,
            "dao_address".to_string(),
            Some("airdrop_address".to_string()),
            Some("lockdrop_address".to_string()),
            funds,
            Timestamp::from_seconds(0),
        )
    }

    fn _do_instantiate(
        mut deps: DepsMut,
        dao_address: String,
        airdrop_address: Option<String>,
        lockdrop_address: Option<String>,
        funds: Option<Vec<Coin>>,
        when_withdrawable: Timestamp,
    ) -> (MessageInfo, Env) {
        let instantiate_msg = InstantiateMsg {
            dao_address,
            airdrop_address,
            lockdrop_address,
            when_withdrawable,
        };
        let info = mock_info("creator", &funds.unwrap_or_default());
        let env = mock_env();
        let res = instantiate(deps.branch(), env.clone(), info.clone(), instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        (info, env)
    }

    fn _do_add_vesting(
        deps: DepsMut,
        env: Env,
        address: String,
        amount: Uint128,
        start_time: u64,
        duration: u64,
    ) {
        let airdrop_info = mock_info("airdrop_address", &[]);

        let res = execute_add_vesting(
            deps,
            env,
            airdrop_info,
            address,
            amount,
            start_time,
            duration,
        );
        assert!(res.is_ok());
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
            let (_info, _env) = _do_instantiate(
                deps.as_mut(),
                "dao_address".to_string(),
                Some("airdrop_address".to_string()),
                Some("lockdrop_address".to_string()),
                None,
                Timestamp::from_seconds(0),
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
            let (_info, _env) = _do_instantiate(
                deps.as_mut(),
                "dao_address".to_string(),
                None,
                None,
                None,
                Timestamp::from_seconds(0),
            );
            let config = query_config(deps.as_ref()).unwrap();
            assert_eq!(config.dao_address, "dao_address".to_string());
            assert_eq!(config.lockdrop_address, None);
            assert_eq!(config.airdrop_address, None);
        }
    }

    mod update_config {
        use crate::contract::execute_update_config;
        use crate::contract::tests::_do_simple_instantiate;
        use crate::state::CONFIG;
        use cosmwasm_std::testing::{mock_dependencies, mock_info};
        use cosmwasm_std::Addr;
        use cw20_base::ContractError;

        #[test]
        fn update_config_works() {
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);
            let dao_info = mock_info("dao_address", &[]);
            let res = execute_update_config(
                deps.as_mut(),
                env,
                dao_info,
                "air".to_string(),
                "lock".to_string(),
            );
            assert!(res.is_ok());
            let config = CONFIG.load(&deps.storage).unwrap();
            assert_eq!(config.airdrop_address, Some(Addr::unchecked("air")));
            assert_eq!(config.lockdrop_address, Some(Addr::unchecked("lock")));
        }

        #[test]
        fn only_admin_can_update_config() {
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);
            let somebody_info = mock_info("somebody", &[]);
            let res = execute_update_config(
                deps.as_mut(),
                env,
                somebody_info,
                "airdrop".to_string(),
                "lockdrop".to_string(),
            );
            assert_eq!(res, Err(ContractError::Unauthorized {}));
        }
    }

    mod add_vesting {
        use crate::contract::tests::_do_simple_instantiate;
        use crate::contract::{execute_add_vesting, VESTING_CLIFF};
        use crate::state::{Schedule, ALLOCATIONS};
        use cosmwasm_std::testing::{mock_dependencies, mock_info};
        use cosmwasm_std::{Addr, StdError, Uint128};
        use cw20_base::ContractError;
        use cw20_base::ContractError::Std;

        #[test]
        fn adds_vesting_for_account_with_correct_settings() {
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);
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
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);
            let non_airdrop_info = mock_info("non_airdrop_address", &[]);

            let res = execute_add_vesting(
                deps.as_mut(),
                env,
                non_airdrop_info,
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
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);
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
        use crate::contract::tests::_do_simple_instantiate;
        use crate::contract::{execute_mint, execute_transfer, DEPOSITED_SYMBOL};
        use cosmwasm_std::testing::{mock_dependencies, mock_info};
        use cosmwasm_std::OverflowOperation::Sub;
        use cosmwasm_std::{coins, Addr, OverflowError, StdError, Uint128};
        use cw20_base::state::BALANCES;
        use cw20_base::ContractError;
        use cw20_base::ContractError::Std;

        #[test]
        fn works_from_airdrop_and_lockdrop() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            // mint
            let dao_info = mock_info("dao_address", &coins(1_000_000_000, DEPOSITED_SYMBOL));
            let res = execute_mint(deps.as_mut(), env.clone(), dao_info);
            assert!(res.is_ok());

            let airdrop_info = mock_info("airdrop_address", &[]);
            let res = execute_transfer(
                deps.as_mut(),
                env,
                airdrop_info,
                "somebody".to_string(),
                Uint128::new(1_000),
            );
            assert!(res.is_ok());
            let balance = BALANCES
                .load(&deps.storage, &Addr::unchecked("somebody"))
                .unwrap();
            assert_eq!(balance, Uint128::new(1_000));

            let airdrop_balance = BALANCES
                .load(&deps.storage, &Addr::unchecked("airdrop_address"))
                .unwrap();
            assert_eq!(airdrop_balance, Uint128::new(1_000_000_000 - 1_000));
            // TODO: add test that lockdrop address has access to transfer (if we need this permission really)
        }

        #[test]
        fn fails_when_try_non_existent_funds() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            let airdrop_info = mock_info("airdrop_address", &[]);
            let res = execute_transfer(
                deps.as_mut(),
                env,
                airdrop_info,
                "somebody".to_string(),
                Uint128::new(1_000),
            );
            assert_eq!(
                res,
                Err(Std(StdError::overflow(OverflowError {
                    operation: Sub,
                    operand1: "0".to_string(),
                    operand2: "1000".to_string()
                })))
            );
        }

        #[test]
        fn not_authorized_from_others() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            let airdrop_info = mock_info("somebody", &[]);
            let res = execute_transfer(
                deps.as_mut(),
                env,
                airdrop_info,
                "somebody".to_string(),
                Uint128::new(1_000),
            );
            assert_eq!(res, Err(ContractError::Unauthorized {}));
        }
    }

    mod withdraw {
        use crate::contract::tests::{_do_add_vesting, _do_instantiate, _do_simple_instantiate};
        use crate::contract::{
            execute_burn_from, execute_mint, execute_transfer, execute_withdraw, DEPOSITED_SYMBOL,
        };
        use crate::state::ALLOCATIONS;
        use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
        use cosmwasm_std::{coins, Addr, BankMsg, StdError, Timestamp, Uint128};
        use cw20_base::state::{BALANCES, TOKEN_INFO};
        use cw20_base::ContractError;

        #[test]
        fn withdraws_all_vested_tokens_correctly() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, mut env) = _do_simple_instantiate(deps.as_mut(), None);

            // mint
            let dao_info = mock_info("dao_address", &coins(1_000_000_000, DEPOSITED_SYMBOL));
            let res = execute_mint(deps.as_mut(), env.clone(), dao_info);
            assert!(res.is_ok());

            // transfer to `somebody`
            let airdrop_info = mock_info("airdrop_address", &[]);
            let res = execute_transfer(
                deps.as_mut(),
                env.clone(),
                airdrop_info,
                "somebody".to_string(),
                Uint128::new(100),
            );
            assert!(res.is_ok());

            // vest
            _do_add_vesting(
                deps.as_mut(),
                env.clone(),
                "somebody".to_string(),
                Uint128::new(100),
                env.block.time.seconds(),
                1000,
            );

            let somebody_info = mock_info("somebody", &[]);

            // at this point `somebody` has vested 100 NTRNs

            // pass 3/4 vesting duration (750 seconds)
            env.block.time = Timestamp::from_seconds(env.block.time.seconds() + 750);

            // check that we burn cuntrn's and send untrn's exactly 100/3*4 = 75
            let res = execute_withdraw(deps.as_mut(), env.clone(), somebody_info);
            assert!(res.is_ok());
            let msgs = res.unwrap().messages;
            assert_eq!(msgs.len(), 1);
            assert_eq!(
                msgs.first().unwrap().msg,
                BankMsg::Send {
                    to_address: "somebody".to_string(),
                    amount: coins(75, DEPOSITED_SYMBOL)
                }
                .into()
            );
            let allocation = ALLOCATIONS
                .load(&deps.storage, &Addr::unchecked("somebody"))
                .unwrap();
            assert_eq!(allocation.allocated_amount, Uint128::new(100));
            assert_eq!(allocation.withdrawn_amount, Uint128::new(75));

            let balance = BALANCES
                .load(&deps.storage, &Addr::unchecked("somebody"))
                .unwrap();
            assert_eq!(balance, Uint128::new(100 - 75));

            let token_info = TOKEN_INFO.load(&deps.storage).unwrap();
            assert_eq!(token_info.total_supply, Uint128::new(1_000_000_000 - 75));

            // now let's check that if we distribute rewards and skip all vesting, we still successfully withdraw what's left

            // pass all vesting duration (> 1000 seconds)
            env.block.time = Timestamp::from_seconds(env.block.time.seconds() + 1250);

            // distribute rewards for that account
            let lockdrop_info = mock_info("lockdrop_address", &[]);
            let res = execute_burn_from(
                deps.as_mut(),
                env.clone(),
                lockdrop_info,
                "somebody".to_string(),
                Uint128::new(10),
            );
            assert!(res.is_ok());

            // after sending 10 to account, we only have 25-10=15 left

            // withdraw
            let somebody_info = mock_info("somebody", &[]);
            let res = execute_withdraw(deps.as_mut(), env, somebody_info);
            assert!(res.is_ok());

            // check that we burn cuntrn's and send untrn's exactly 15
            let msgs = res.unwrap().messages;
            assert_eq!(msgs.len(), 1);
            assert_eq!(
                msgs.first().unwrap().msg,
                BankMsg::Send {
                    to_address: "somebody".to_string(),
                    amount: coins(15, DEPOSITED_SYMBOL)
                }
                .into()
            );
            let allocation = ALLOCATIONS
                .load(&deps.storage, &Addr::unchecked("somebody"))
                .unwrap();
            assert_eq!(allocation.allocated_amount, Uint128::new(100));
            assert_eq!(allocation.withdrawn_amount, Uint128::new(90)); // because we sent 10 as rewards that skipped vesting

            let balance = BALANCES
                .load(&deps.storage, &Addr::unchecked("somebody"))
                .unwrap();
            assert!(balance.is_zero());
        }

        #[test]
        fn does_not_withdraw_until_ready() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, env) = _do_instantiate(
                deps.as_mut(),
                "dao_address".to_string(),
                None,
                None,
                None,
                mock_env().block.time.plus_seconds(1_000_000),
            );
            let somebody_info = mock_info("somebody", &[]);
            let res = execute_withdraw(deps.as_mut(), env, somebody_info);
            assert_eq!(
                res,
                Err(ContractError::Std(StdError::generic_err(
                    "too early to claim"
                ))),
            );
        }

        #[test]
        fn does_not_withdraw_if_no_tokens_vested_yet() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            // check
            let somebody_info = mock_info("somebody", &[]);
            let res = execute_withdraw(deps.as_mut(), env, somebody_info);
            assert_eq!(
                res,
                Err(ContractError::Std(StdError::not_found(
                    "credits::state::Allocation"
                )))
            );
        }
    }

    mod burn {
        use crate::contract::tests::_do_simple_instantiate;
        use crate::contract::{execute_burn, execute_mint, DEPOSITED_SYMBOL};
        use cosmwasm_std::testing::{mock_dependencies, mock_info};
        use cosmwasm_std::{coins, Addr, BankMsg, Uint128};
        use cw20_base::state::{BALANCES, TOKEN_INFO};
        use cw20_base::ContractError;

        #[test]
        fn works_with_correct_params_for_airdrop() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            // mint
            let minted_balance = 1_000_000_000;
            let dao_info = mock_info("dao_address", &coins(minted_balance, DEPOSITED_SYMBOL));
            let res = execute_mint(deps.as_mut(), env.clone(), dao_info);
            assert!(res.is_ok());

            // burn amount
            let airdrop_info = mock_info("airdrop_address", &[]);
            let res = execute_burn(deps.as_mut(), env, airdrop_info, Uint128::new(10000));
            assert!(res.is_ok());

            let msgs = res.unwrap().messages;
            assert_eq!(msgs.len(), 1);
            assert_eq!(
                msgs.first().unwrap().msg,
                BankMsg::Send {
                    to_address: "airdrop_address".to_string(),
                    amount: coins(10_000, DEPOSITED_SYMBOL)
                }
                .into()
            );

            let balance = BALANCES
                .load(&deps.storage, &Addr::unchecked("airdrop_address"))
                .unwrap();
            assert_eq!(balance, Uint128::new(minted_balance - 10_000));

            let token_info = TOKEN_INFO.load(&deps.storage).unwrap();
            assert_eq!(
                token_info.total_supply,
                Uint128::new(minted_balance - 10_000)
            );
        }

        #[test]
        fn unauthorized_for_non_airdrop_addresses() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            // burn amount
            let airdrop_info = mock_info("non_airdrop_address", &[]);
            let res = execute_burn(deps.as_mut(), env, airdrop_info, Uint128::new(10000));
            assert_eq!(res, Err(ContractError::Unauthorized {}))
        }
    }

    mod burn_from {
        use crate::contract::tests::_do_simple_instantiate;
        use crate::contract::{execute_burn_from, execute_mint, DEPOSITED_SYMBOL};
        use cosmwasm_std::testing::{mock_dependencies, mock_info};
        use cosmwasm_std::{coins, Addr, BankMsg, Uint128};
        use cw20_base::state::{BALANCES, TOKEN_INFO};

        #[test]
        fn works_properly_with_airdrop_account() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            // mint
            let minted_balance = 1_000_000_000;
            let dao_info = mock_info("dao_address", &coins(minted_balance, DEPOSITED_SYMBOL));
            let res = execute_mint(deps.as_mut(), env.clone(), dao_info);
            assert!(res.is_ok());

            // burn_from
            let lockdrop_info = mock_info("lockdrop_address", &[]);
            let res = execute_burn_from(
                deps.as_mut(),
                env,
                lockdrop_info,
                "airdrop_address".to_string(),
                Uint128::new(20_000),
            );
            assert!(res.is_ok());

            let msgs = res.unwrap().messages;
            assert_eq!(msgs.len(), 1);
            assert_eq!(
                msgs.first().unwrap().msg,
                BankMsg::Send {
                    to_address: "airdrop_address".to_string(),
                    amount: coins(20_000, DEPOSITED_SYMBOL)
                }
                .into()
            );

            let balance = BALANCES
                .load(&deps.storage, &Addr::unchecked("airdrop_address"))
                .unwrap();
            assert_eq!(balance, Uint128::new(minted_balance - 20_000));

            let token_info = TOKEN_INFO.load(&deps.storage).unwrap();
            assert_eq!(
                token_info.total_supply,
                Uint128::new(minted_balance - 20_000)
            );
        }
    }

    mod transfer_from {
        use crate::contract::tests::_do_simple_instantiate;
        use crate::contract::{
            execute_increase_allowance, execute_mint, execute_transfer_from, DEPOSITED_SYMBOL,
        };
        use cosmwasm_std::testing::{mock_dependencies, mock_info};
        use cosmwasm_std::{coins, Addr, Uint128};
        use cw20_base::state::BALANCES;
        use cw20_base::ContractError;

        #[test]
        fn works_with_allowance_set() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            // mint
            let minted_balance = 1_000_000_000;
            let dao_info = mock_info("dao_address", &coins(minted_balance, DEPOSITED_SYMBOL));
            let res = execute_mint(deps.as_mut(), env.clone(), dao_info);
            assert!(res.is_ok());

            // set allowance
            let airdrop_info = mock_info("airdrop_address", &[]);
            let res = execute_increase_allowance(
                deps.as_mut(),
                env.clone(),
                airdrop_info,
                "lockdrop_address".to_string(),
                Uint128::new(100),
                None,
            );

            assert!(res.is_ok());

            // transfer_from
            let lockdrop_info = mock_info("lockdrop_address", &[]);
            let res = execute_transfer_from(
                deps.as_mut(),
                env,
                lockdrop_info,
                "airdrop_address".to_string(),
                "recipient_address".to_string(),
                Uint128::new(50),
            );
            assert!(res.is_ok());

            let recipient_balance = BALANCES
                .load(&deps.storage, &Addr::unchecked("recipient_address"))
                .unwrap();
            assert_eq!(recipient_balance, Uint128::new(50));

            let airdrop_balance = BALANCES
                .load(&deps.storage, &Addr::unchecked("airdrop_address"))
                .unwrap();
            assert_eq!(airdrop_balance, Uint128::new(minted_balance - 50));
        }

        #[test]
        fn does_not_transfer_without_allowance() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            // mint
            let minted_balance = 1_000_000_000;
            let dao_info = mock_info("dao_address", &coins(minted_balance, DEPOSITED_SYMBOL));
            let res = execute_mint(deps.as_mut(), env.clone(), dao_info);
            assert!(res.is_ok());

            // transfer_from
            let other_info = mock_info("lockdrop_address", &[]);
            let res = execute_transfer_from(
                deps.as_mut(),
                env,
                other_info,
                "airdrop_address".to_string(),
                "recipient_address".to_string(),
                Uint128::new(50),
            );
            assert_eq!(res, Err(ContractError::NoAllowance {}));
        }

        #[test]
        fn only_lockdrop_can_transfer_from() {
            // instantiate
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            // set allowance
            let airdrop_info = mock_info("airdrop_address", &[]);
            let res = execute_increase_allowance(
                deps.as_mut(),
                env.clone(),
                airdrop_info,
                "lockdrop_address".to_string(),
                Uint128::new(100),
                None,
            );
            assert!(res.is_ok());

            let non_lockdrop_info = mock_info("non_lockdrop_address", &[]);
            let res = execute_transfer_from(
                deps.as_mut(),
                env,
                non_lockdrop_info,
                "airdrop_address".to_string(),
                "recipient_address".to_string(),
                Uint128::new(50),
            );

            assert_eq!(res, Err(ContractError::Unauthorized {}));
        }
    }

    mod mint {
        use crate::contract::tests::_do_simple_instantiate;
        use crate::contract::{execute_mint, DEPOSITED_SYMBOL};
        use cosmwasm_std::testing::{mock_dependencies, mock_info};
        use cosmwasm_std::{Addr, Coin, StdError, Uint128};
        use cw20_base::state::{BALANCES, TOKEN_INFO};
        use cw20_base::ContractError;

        #[test]
        fn does_not_work_without_funds_sent() {
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);
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
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            let funds = vec![Coin::new(500, DEPOSITED_SYMBOL)];
            let non_dao_info = mock_info("non dao", &funds);
            let res = execute_mint(deps.as_mut(), env, non_dao_info);
            assert_eq!(res, Err(ContractError::Unauthorized {}));
        }

        #[test]
        fn works_with_ntrn_funds() {
            let mut deps = mock_dependencies();
            let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

            let funds = vec![Coin::new(500, DEPOSITED_SYMBOL)];
            let dao_info = mock_info("dao_address", &funds);
            let res = execute_mint(deps.as_mut(), env, dao_info);
            assert!(res.is_ok());

            let config = TOKEN_INFO.load(&deps.storage).unwrap();
            assert_eq!(config.total_supply, Uint128::new(500));

            // sends on balance to airdrop_address
            let balance = BALANCES
                .load(&deps.storage, &Addr::unchecked("airdrop_address"))
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
