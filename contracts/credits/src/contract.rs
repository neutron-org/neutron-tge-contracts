use crate::error::ContractError;
use crate::error::ContractError::{AlreadyVested, Cw20Error, NoFundsSupplied, Unauthorized};
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
use cw20_base::state::{BALANCES, TOKEN_INFO};

use crate::msg::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, TotalSupplyResponse, UpdateConfigMsg,
    VestedAmountResponse, WithdrawableAmountResponse,
};
use crate::state::{Allocation, Config, Schedule, ALLOCATIONS, CONFIG};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:credits";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const TOKEN_NAME: &str = "cNTRN";
pub const TOKEN_SYMBOL: &str = "cNTRN";
pub const TOKEN_DECIMALS: u8 = 6;
pub const DEPOSITED_SYMBOL: &str = "untrn";

// Cliff duration in seconds for vesting.
// Before the schedule.start_time + schedule.cliff vesting does not start.
// 0 cliff means no cliff
pub const VESTING_CLIFF: u64 = 0;

/// Instantiates the contract.
/// Configures cw20 token info.
/// Can specify addresses for dao, airdrop and lockdrop contracts.
/// Specifies when all users can start withdraw their vesting funds.
/// Specifies dao contract as a minter.
/// Does not mint any tokens here.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, Cw20ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let dao_address = deps.api.addr_validate(&msg.dao_address)?;
    let config = Config {
        dao_address: dao_address.clone(),
        airdrop_address: None,
        lockdrop_address: None,
        when_withdrawable: None,
    };

    // store token info
    let info = Cw20State::TokenInfo {
        name: TOKEN_NAME.to_string(),
        symbol: TOKEN_SYMBOL.to_string(),
        decimals: TOKEN_DECIMALS,
        total_supply: Uint128::zero(),
        mint: Some(Cw20State::MinterData {
            minter: dao_address,
            cap: None,
        }),
    };

    CONFIG.save(deps.storage, &config)?;
    TOKEN_INFO.save(deps.storage, &info, env.block.height)?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig { config } => execute_update_config(deps, env, info, config),
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
    msg: UpdateConfigMsg,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.dao_address {
        return Err(Unauthorized);
    }

    if let Some(airdrop_address) = msg.airdrop_address {
        config.airdrop_address = Some(deps.api.addr_validate(&airdrop_address)?);
    }

    if let Some(lockdrop_address) = msg.lockdrop_address {
        config.lockdrop_address = Some(deps.api.addr_validate(&lockdrop_address)?);
    }

    if let Some(when_withdrawable) = msg.when_withdrawable {
        config.when_withdrawable = Some(when_withdrawable);
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

/// Adds vesting settings for the specified `address` and `amount` for `duration`.
/// `amount` expected to be equal to amount on a user balance.
/// Returns a default object of type [`Response`].
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **address** is an object of type [`String`]. Address to add vesting to.
///
/// * **amount** is an object of type [`Uint128`]. Amount to be vested.
///
/// * **start_time** is an object of type [`u64`]. Vesting starts after `start_time`. Specified in UNIX time in seconds.
///
/// * **duration** is an object of type [`u64`]. Duration of vesting. Specified in seconds.
pub fn execute_add_vesting(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    address: String,
    amount: Uint128,
    start_time: u64,
    duration: u64,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let airdrop_address = config
        .airdrop_address
        .ok_or(ContractError::AirdropNotConfigured)?;
    if info.sender != airdrop_address {
        return Err(Unauthorized);
    }

    let vested_to = deps.api.addr_validate(&address)?;

    ALLOCATIONS.update(
        deps.storage,
        &vested_to,
        |o: Option<Allocation>| -> Result<Allocation, ContractError> {
            match o {
                Some(_) => Err(AlreadyVested { address }),
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

/// Transfers specified `amount` from sender to the specified `recipient`.
/// Standard cw20 transfer. Allowed to execute only for an airdrop contract.
/// Returns a default object of type [`Response`].
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`]
///
/// * **_env** is an object of type [`Env`]
///
/// * **info** is an object of type [`MessageInfo`]
///
/// * **recipient** is an object of type [`String`]. Address to transfer to.
///
/// * **amount** is an object of type [`Uint128`]. Amount to be transferred.
pub fn execute_transfer(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let airdrop_address = config
        .airdrop_address
        .ok_or(ContractError::AirdropNotConfigured)?;
    if info.sender != airdrop_address {
        return Err(Unauthorized);
    }

    ::cw20_base::contract::execute_transfer(deps, env, info, recipient, amount).map_err(Cw20Error)
}

/// Calculates amount that is already unlocked from vesting for sender,
/// burns this amount of cNTRN tokens and sends 1:1 of untrn tokens proportion to the sender.
///
/// Available to execute only after `config.when_withdrawable` time.
///
/// Returns error if nothing left to withdraw.
///
/// Returns a default object of type [`Response`].
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`]
///
/// * **env** is an object of type [`Env`]
///
/// * **info** is an object of type [`MessageInfo`]
pub fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let when_withdrawable = config
        .when_withdrawable
        .ok_or(ContractError::WhenWithdrawableIsNotConfigured)?;
    if when_withdrawable > env.block.time {
        return Err(ContractError::TooEarlyToClaim);
    }

    let owner = info.sender.clone();
    let mut allocation: Allocation = ALLOCATIONS.load(deps.storage, &owner)?;
    let max_withdrawable_amount = compute_withdrawable_amount(
        allocation.allocated_amount,
        allocation.withdrawn_amount,
        &allocation.schedule,
        env.block.time.seconds(),
    )?;

    if max_withdrawable_amount.is_zero() {
        return Err(ContractError::NoFundsToClaim);
    }

    // Guard against the case where actual balance is smaller than max withdrawable amount.
    // That can happen if user already withdrawn some funds as rewards for lockdrop participation through burn_from (skipping vesting).
    // Example: user had 100 cNTRN on balance, and burned 100 cNTRN through burn_from.
    // Suppose vesting period fully ended. In that case `compute_withdrawable_amount()` will return 100 untrn,
    // although he has 0 on balance.
    let actual_balance = BALANCES.load(deps.storage, &owner)?;
    let to_withdraw = max_withdrawable_amount.min(actual_balance);

    // Check that not zero
    if to_withdraw.is_zero() {
        return Err(ContractError::NoFundsToClaim);
    }

    allocation.withdrawn_amount += to_withdraw;
    ALLOCATIONS.save(deps.storage, &owner, &allocation)?;

    burn_and_send(deps, env, info, to_withdraw)
}

/// Withdraws specified `amount` of tokens -
/// burns cNTRN tokens and sends amount in 1:1 proportion of untrn tokens to the sender (airdrop account).
/// Used by airdrop account for burning unclaimed tokens.
///
/// Only available for an airdrop contract account.
///
/// Returns a default object of type [`Response`].
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **amount** is an object of type [`Uint128`]. Amount to be burned and minted in 1:1 proportion.
pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let airdrop_address = config
        .airdrop_address
        .ok_or(ContractError::AirdropNotConfigured)?;
    if info.sender != airdrop_address {
        return Err(Unauthorized);
    }

    burn_and_send(deps, env, info, amount)
}

/// Withdraws specified `amount` of tokens from specified `owner` -
/// burns cNTRN tokens and sends amount in 1:1 proportion of untrn tokens to the `owner`.
///
/// Used for rewards for lockdrop participation and *skips vesting*.
/// It also does NOT change amounts available for `withdraw` by user.
///
/// Only available for the lockdrop contract account.
///
/// Returns a default object of type [`Response`].
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **owner** is an object of type [`String`]. Address to burn cNTRN tokens from and send untrn tokens to.
///
/// * **amount** is an object of type [`Uint128`]. Amount to be burned and minted in 1:1 proportion.
pub fn execute_burn_from(
    deps: DepsMut,
    env: Env,
    mut info: MessageInfo,
    owner: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let lockdrop_address = config
        .lockdrop_address
        .ok_or(ContractError::LockdropNotConfigured)?;
    if info.sender != lockdrop_address {
        return Err(Unauthorized);
    }

    // burn funds of `owner`, but skip the vesting stage
    info.sender = deps.api.addr_validate(&owner)?;

    burn_and_send(deps, env, info, amount)
}

/// Mints cNTRN tokens in 1:1 proportion to sent untrn ones
/// Uses cw20 standard mint, but only can mint to the airdrop contract balance
/// Returns a default object of type [`Response`].
///
/// Only available to dao contract to call (permission set up in initialization in `TokenInfo.mint.minter` field
///
/// Returns error if no untrn funds were sent.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
pub fn execute_mint(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    // mint in 1:1 proportion to locked untrn tokens
    let untrn_funds = deps
        .querier
        .query_balance(env.clone().contract.address, DEPOSITED_SYMBOL)?;
    if untrn_funds.amount.is_zero() {
        return Err(NoFundsSupplied());
    }

    let config = CONFIG.load(deps.storage)?;
    let recipient = config
        .airdrop_address
        .ok_or(ContractError::AirdropNotConfigured)?;

    ::cw20_base::contract::execute_mint(deps, env, info, recipient.to_string(), untrn_funds.amount)
        .map_err(Cw20Error)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::WithdrawableAmount { address } => {
            to_binary(&query_withdrawable_amount(deps, env, address)?)
        }
        QueryMsg::VestedAmount { address } => to_binary(&query_vested_amount(deps, env, address)?),
        QueryMsg::Allocation { address } => to_binary(&query_allocation(deps, address)?),
        QueryMsg::Balance { address } => {
            to_binary(&::cw20_base::contract::query_balance(deps, address)?)
        }
        QueryMsg::TotalSupplyAtHeight { height } => {
            to_binary(&query_total_supply_at_height(deps, height)?)
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
        } => to_binary(&::cw20_base::enumerable::query_all_allowances(
            deps,
            owner,
            start_after,
            limit,
        )?),
        QueryMsg::AllAccounts { start_after, limit } => to_binary(
            &::cw20_base::enumerable::query_all_accounts(deps, start_after, limit)?,
        ),
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
    }
}

/// Returns current contract config.
/// Returns an object of type [`StdResult<Config>`].
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
pub fn query_config(deps: Deps) -> StdResult<Config> {
    CONFIG.load(deps.storage)
}

/// Returns total token supply at a specified `maybe_height`. If height is not present, returns current total supply.
/// Returns an object of type [`StdResult<TotalSupplyResponse>`].
///
/// Returns `0` if no total supply found (should be impossible).
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **maybe_height** is an object of type [`Option<u64>`].
/// Use `Some(height)` for getting total supply at some height, `None` for getting current total supply.
fn query_total_supply_at_height(
    deps: Deps,
    maybe_height: Option<u64>,
) -> StdResult<TotalSupplyResponse> {
    let total_supply = match maybe_height {
        Some(height) => TOKEN_INFO.may_load_at_height(deps.storage, height)?,
        None => TOKEN_INFO.may_load(deps.storage)?,
    }
    .map(|info| info.total_supply)
    .unwrap_or_default();

    Ok(TotalSupplyResponse { total_supply })
}

/// Returns balance at a specified `maybe_height` for specified `address`. If height is not present, returns current balance.
/// Returns an object of type [`StdResult<BalanceResponse>`].
/// Returns `0` if no balance for such user exists at this height.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
//
/// * **address** is an object of type [`String`]. Address of the user to get balance for.
///
/// * **maybe_height** is an object of type [`Option<u64>`].
/// Use `Some(height)` for getting balance at some height, `None` for getting current balance.
fn query_balance_at_height(
    deps: Deps,
    address: String,
    maybe_height: Option<u64>,
) -> StdResult<BalanceResponse> {
    let balance = match maybe_height {
        Some(height) => {
            BALANCES.may_load_at_height(deps.storage, &deps.api.addr_validate(&address)?, height)?
        }
        None => BALANCES.may_load(deps.storage, &deps.api.addr_validate(&address)?)?,
    }
    .unwrap_or_default();

    Ok(BalanceResponse { balance })
}

/// Returns amount for specified `address` that is available to `withdraw` from balance.
/// Returns an object of type [`StdResult<WithdrawableAmountResponse>`].
/// Returns an error if no vesting was set up or no balance for such user exists.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **address** is an object of type [`String`]. Address of the user we want to query withdrawable amount.
fn query_withdrawable_amount(
    deps: Deps,
    env: Env,
    address: String,
) -> StdResult<WithdrawableAmountResponse> {
    let owner = deps.api.addr_validate(&address)?;
    let allocation: Allocation = ALLOCATIONS.load(deps.storage, &owner)?;
    let max_withdrawable_amount = compute_withdrawable_amount(
        allocation.allocated_amount,
        allocation.withdrawn_amount,
        &allocation.schedule,
        env.block.time.seconds(),
    )?;
    // // because we have lockdrop rewards that skip vesting, we can get withdrawable amount greater than the current balance
    // // so we need to withdraw not more than the current balance
    let actual_balance =
        BALANCES
            .may_load(deps.storage, &owner)?
            .ok_or_else(|| StdError::GenericErr {
                msg: "No balance".to_string(),
            })?;
    let amount = max_withdrawable_amount.min(actual_balance);

    Ok(WithdrawableAmountResponse { amount })
}

/// Returns amount for specified `address` that is still vested and user cannot withdraw yet.
/// It's equal to (user balance) - (user withdrawable amount)
/// Returns an object of type [`StdResult<VestedAmountResponse>`].
/// Returns an error if no vesting was set up or no balance for such user exists.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **address** is an object of type [`String`]. Address of the user we want to query withdrawable amount.
pub fn query_vested_amount(
    deps: Deps,
    env: Env,
    address: String,
) -> StdResult<VestedAmountResponse> {
    let owner = deps.api.addr_validate(&address)?;
    let allocation: Allocation = ALLOCATIONS.load(deps.storage, &owner)?;
    let max_withdrawable_amount = compute_withdrawable_amount(
        allocation.allocated_amount,
        allocation.withdrawn_amount,
        &allocation.schedule,
        env.block.time.seconds(),
    )?;
    // because we have lockdrop rewards that skip vesting, we can get withdrawable amount greater than the current balance
    // so we need to withdraw not more than the current balance
    let actual_balance = BALANCES.load(deps.storage, &owner)?;
    let withdrawable_amount = max_withdrawable_amount.min(actual_balance);
    let amount = actual_balance - withdrawable_amount;

    Ok(VestedAmountResponse { amount })
}

/// Returns current vesting allocation for specified `address`.
/// Note that `allocation.withdrawn_amount` does not take burned rewards from `BurnFrom` into account.
/// That means that `allocation.allocated_amount - allocation.withdrawn_amount` is not always equal to `withdrawable amount`
/// Returns an object of type [`StdResult<Allocation>`].
/// Returns an error if no vesting was set up or no balance for such user exists.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **address** is an object of type [`String`]. Address of the user we want to query withdrawable amount.
fn query_allocation(deps: Deps, address: String) -> StdResult<Allocation> {
    let owner = deps.api.addr_validate(&address)?;
    ALLOCATIONS.load(deps.storage, &owner)
}

// burns cNTRN tokens and send untrn tokens to the sender
fn burn_and_send(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let to_address = info.sender.to_string();
    let burn_response = ::cw20_base::contract::execute_burn(deps, env, info, amount)?;
    let send = BankMsg::Send {
        to_address,
        amount: vec![Coin::new(amount.u128(), DEPOSITED_SYMBOL)],
    };

    Ok(burn_response.add_message(send))
}

/// Compute the max withdrawable amount based on the current timestamp and the vesting schedule
///
/// The withdrawable amount is vesting amount minus the amount already withdrawn.
/// Implementation copied from mars-protocol mars-vesting contract:
/// https://github.com/mars-protocol/v1-core/tree/master/contracts/mars-vesting
///
/// Note that is does not take withdrawn rewards into account,
/// so returned amount can be greater than the current user balance.
///
/// ## Params
/// * **allocated_amount** is an object of type [`Uint128`]. Total allocated amount to be vested.
///
/// * **withdrawn_amount** is an object of type [`Uint128`]. Already withdrawn amount (see `withdraw`).
/// Note that it does not count reward withdraws that skip vesting.
///
/// * **vest_schedule** is an object of type [`&Schedule`]. Vesting schedule.
///
/// * **current_time** is an object of type [`u64`]. Current UNIX time in seconds.
pub fn compute_withdrawable_amount(
    allocated_amount: Uint128,
    withdrawn_amount: Uint128,
    schedule: &Schedule,
    current_time: u64,
) -> StdResult<Uint128> {
    // Before the end of cliff period, no token will be vested/unlocked
    let vested_amount = if current_time < schedule.start_time + schedule.cliff {
        Uint128::zero()
        // After the end of cliff, tokens vest/unlock linearly between start time and end time
    } else if current_time < schedule.start_time + schedule.duration {
        allocated_amount.multiply_ratio(current_time - schedule.start_time, schedule.duration)
        // After end time, all tokens are fully vested/unlocked
    } else {
        allocated_amount
    };

    vested_amount
        .checked_sub(withdrawn_amount)
        .map_err(|overflow_err| overflow_err.into())
}
