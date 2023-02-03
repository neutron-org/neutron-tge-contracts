use crate::{
    error::ContractError,
    msg::{Config, EventConfig, ExecuteMsg, InfoResponse, InstantiateMsg, MigrateMsg, QueryMsg},
    state::{CONFIG, DEPOSITS, TOTAL_DEPOSIT},
};
use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, BankMsg, Binary, Coin, Decimal, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128,
};
use cw2::set_contract_version;
use cw_asset::{Asset, AssetInfo};

pub const DEFAULT_SLOT_DURATION: u64 = 60 * 60;

const CONTRACT_NAME: &str = concat!("crates.io:neutron-contracts__", env!("CARGO_PKG_NAME"));
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let cfg = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        reserve: deps.api.addr_validate(&msg.reserve)?,
        token: deps.api.addr_validate(&msg.token)?,
        event_config: None,
        base_denom: msg.base_denom,
        slot_duration: msg.slot_duration.unwrap_or(DEFAULT_SLOT_DURATION),
    };
    TOTAL_DEPOSIT.save(deps.storage, &Uint128::zero())?;
    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::new())
}

#[entry_point]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

// QUERIES

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => query_config(deps),
        QueryMsg::Info { address } => query_info(deps, env, address),
    }
}

fn query_balance(deps: &Deps, address: &Addr, denom: String) -> StdResult<Uint128> {
    let balance = deps.querier.query_balance(address, &denom)?;
    Ok(balance.amount)
}

fn query_config(deps: Deps) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    to_binary(&config)
}

fn query_info(deps: Deps, env: Env, address: String) -> StdResult<Binary> {
    let time = env.block.time.seconds();
    let address = deps.api.addr_validate(&address)?;
    let config = CONFIG.load(deps.storage)?;
    let event_config = config
        .event_config
        .ok_or_else(|| StdError::generic_err("Event config is empty"))?;
    let info = DEPOSITS.load(deps.storage, &address).unwrap_or_default();

    let withdrawable_amount = if time > event_config.stage2_begin && !info.amount.is_zero() {
        if info.withdrew_stage2 || time >= event_config.stage2_end {
            Uint128::zero()
        } else {
            let current_slot = (event_config.stage2_end - time) / config.slot_duration;
            let total_slots =
                (event_config.stage2_end - event_config.stage2_begin) / config.slot_duration;

            let withdrawable_portion =
                Decimal::from_ratio(current_slot + 1u64, total_slots).min(Decimal::one());

            info.amount * withdrawable_portion
        }
    } else {
        info.amount
    };

    let total_deposit = TOTAL_DEPOSIT.load(deps.storage)?;
    let tokens_to_claim = if !total_deposit.is_zero() {
        event_config
            .amount
            .multiply_ratio(info.amount, total_deposit)
    } else {
        Uint128::zero()
    };

    let clamable =
        time >= event_config.stage2_end && !tokens_to_claim.is_zero() && !info.tokens_claimed;

    to_binary(&InfoResponse {
        deposit: info.amount,
        total_deposit,
        withdrawable_amount,
        tokens_to_claim,
        clamable,
    })
}

// EXECUTES

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    deps.api
        .debug(format!("WASMDEBUG: execute: received msg: {:?}", msg).as_str());

    match msg {
        ExecuteMsg::WithdrawReserve {} => execute_withdraw_reserve(deps, env, info),
        ExecuteMsg::Deposit {} => execute_deposit(deps, env, info),
        ExecuteMsg::Withdraw { amount } => execute_withdraw(deps, env, info, amount),
        ExecuteMsg::WithdrawTokens {} => execute_withdraw_tokens(deps, env, info),
        ExecuteMsg::SetupEvent { config } => execute_setup_event(deps, env, info, config),
    }
}

fn execute_withdraw_reserve(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let event_cfg = config
        .event_config
        .ok_or(ContractError::EmptyEventConfig {})?;

    if info.sender.as_str() != config.owner.as_str() {
        return Err(ContractError::Unauthorized {});
    }

    if env.block.time.seconds() < event_cfg.stage2_end {
        return Err(ContractError::InvalidReserveWithdraw {
            text: "cannot withdraw funds yet".to_string(),
        });
    }

    let balance = query_balance(
        &deps.as_ref(),
        &env.contract.address,
        config.base_denom.clone(),
    )?;

    let msg = BankMsg::Send {
        to_address: config.reserve.to_string(),
        amount: vec![Coin {
            denom: config.base_denom,
            amount: balance,
        }],
    };
    let res = Response::new().add_message(msg);
    Ok(res.add_attributes(vec![
        attr("action", "withdraw_reserve"),
        attr("amount", balance.to_string()),
    ]))
}

fn execute_deposit(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let event_config = config.event_config.unwrap();

    if env.block.time.seconds() < event_config.stage1_begin {
        return Err(ContractError::DepositError {
            text: format!(
                "deposit period is not start yet, now: {}, start: {}",
                env.block.time.seconds(),
                event_config.stage1_begin
            ),
        });
    }

    if env.block.time.seconds() >= event_config.stage2_begin {
        return Err(ContractError::DepositError {
            text: "deposit period is finished".to_string(),
        });
    }

    if info.funds.len() != 1 {
        return Err(ContractError::DepositError {
            text: format!("deposit must be sent as one sum of {}", config.base_denom),
        });
    }

    let coin = &info.funds[0];
    if coin.denom != config.base_denom || coin.amount == Uint128::zero() {
        return Err(ContractError::DepositError {
            text: format!("deposit must be positive amount of {}", config.base_denom),
        });
    }

    DEPOSITS.update(deps.storage, &info.sender, |current| -> StdResult<_> {
        let mut deposit = current.unwrap_or_default();
        deposit.amount += coin.amount;

        Ok(deposit)
    })?;

    TOTAL_DEPOSIT.update(deps.storage, |amount| -> StdResult<_> {
        Ok(amount + coin.amount)
    })?;

    Ok(Response::new()
        .add_attribute("action", "deposit")
        .add_attribute("amount", coin.amount.to_string()))
}

fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let event_config = config.event_config.unwrap();
    let current_time = env.block.time.seconds();

    if current_time >= event_config.stage2_end {
        return Err(ContractError::WithdrawError {
            text: "withdraw period is over".to_string(),
        });
    }
    let mut deposit_info = DEPOSITS
        .load(deps.storage, &info.sender)
        .unwrap_or_default();

    if deposit_info.amount == Uint128::zero() {
        return Err(ContractError::WithdrawError {
            text: "nothing to withdraw".to_string(),
        });
    }

    let withdrawable_amount = if current_time > event_config.stage2_begin {
        if deposit_info.withdrew_stage2 {
            return Err(ContractError::WithdrawError {
                text: "a withdraw was already executed on phase 2".to_string(),
            });
        }

        let current_slot = (event_config.stage2_end - current_time) / config.slot_duration;
        let total_slots =
            (event_config.stage2_end - event_config.stage2_begin) / config.slot_duration;
        let withdrawable_portion =
            Decimal::from_ratio(current_slot + 1u64, total_slots).min(Decimal::one());

        deposit_info.withdrew_stage2 = true;
        deposit_info.amount * withdrawable_portion
    } else {
        deposit_info.amount
    };

    let withdraw_amount = match amount {
        None => withdrawable_amount,
        Some(requested_amount) => {
            if requested_amount > withdrawable_amount {
                return Err(ContractError::WithdrawError {
                    text: format!(
                        "can not withdraw more than current withdrawable amount ({})",
                        withdrawable_amount
                    ),
                });
            }
            if requested_amount == Uint128::zero() {
                return Err(ContractError::WithdrawError {
                    text: "withdraw amount must be bigger than 0".to_string(),
                });
            }

            requested_amount
        }
    };

    deposit_info.amount -= withdraw_amount;

    DEPOSITS.save(deps.storage, &info.sender, &deposit_info)?;

    TOTAL_DEPOSIT.update(deps.storage, |curr| -> StdResult<Uint128> {
        Ok(curr - withdraw_amount)
    })?;

    let msg = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: vec![Coin {
            denom: config.base_denom,
            amount: withdraw_amount,
        }],
    };

    let res = Response::new().add_message(msg);

    Ok(res.add_attributes(vec![
        attr("action", "withdraw"),
        attr("amount", withdraw_amount.to_string()),
    ]))
}

pub fn execute_withdraw_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let event_config = config.event_config.unwrap();

    if env.block.time.seconds() < event_config.stage2_end {
        return Err(ContractError::WithdrawTokensError {
            text: "cannot withdraw tokens yet".to_string(),
        });
    }

    let mut deposit_info = DEPOSITS.load(deps.storage, &info.sender).map_err(|_| {
        ContractError::WithdrawTokensError {
            text: "deposit information not found".to_string(),
        }
    })?;
    if deposit_info.tokens_claimed {
        return Err(ContractError::WithdrawTokensError {
            text: "tokens were already claimed".to_string(),
        });
    }

    let deposit_total = TOTAL_DEPOSIT.load(deps.storage)?;
    let amount = event_config
        .amount
        .multiply_ratio(deposit_info.amount, deposit_total);
    if amount == Uint128::zero() {
        return Err(ContractError::WithdrawTokensError {
            text: "no tokens available for withdraw".to_string(),
        });
    }

    deposit_info.tokens_claimed = true;

    DEPOSITS.save(deps.storage, &info.sender, &deposit_info)?;
    let to_send = Asset {
        info: AssetInfo::Cw20(config.token),
        amount,
    };
    Ok(Response::new()
        .add_message(to_send.transfer_msg(&info.sender)?)
        .add_attributes(vec![
            attr("action", "withdraw_tokens"),
            attr("withdraw_amount", amount.to_string()),
        ]))
}

pub fn execute_setup_event(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    event_config: EventConfig,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender.as_str() != config.owner.as_str() {
        return Err(ContractError::Unauthorized {});
    }

    if config.event_config.is_some() {
        return Err(ContractError::DuplicatePostInit {});
    }

    if env.block.time.seconds() > event_config.stage1_begin {
        return Err(ContractError::InvalidEventConfig {
            text: format!(
                "stage1_begin must be in the future, but is {}",
                env.block.time.seconds()
            ),
        });
    }
    if event_config.stage1_begin > event_config.stage2_begin {
        return Err(ContractError::InvalidEventConfig {
            text: format!(
                "stage2_begin must be after stage1_begin, but stage1_begin is {} and stage2_begin is {}", 
                event_config.stage1_begin,
                event_config.stage2_begin
            ),
        });
    }
    if event_config.stage2_begin > event_config.stage2_end {
        return Err(ContractError::InvalidEventConfig {
            text: format!(
                "stage2_end must be after stage2_begin, but stage2_begin is {} and stage2_end is {}",
                event_config.stage2_begin, event_config.stage2_end
            ),
        });
    }
    if (event_config.stage2_end - event_config.stage2_begin) < config.slot_duration {
        return Err(ContractError::InvalidEventConfig {
            text: format!(
                "stage2_end must be at least {} seconds after stage2_begin, but stage2_begin is {} and stage2_end is {}",
               config.slot_duration, event_config.stage2_begin, event_config.stage2_end
            ),
        });
    }

    config.event_config = Some(event_config.clone());

    CONFIG.save(deps.storage, &config)?;

    let to_send = Asset {
        info: AssetInfo::Cw20(config.token),
        amount: event_config.amount,
    };

    Ok(Response::new().add_message(to_send.transfer_from_msg(info.sender, env.contract.address)?))
}
