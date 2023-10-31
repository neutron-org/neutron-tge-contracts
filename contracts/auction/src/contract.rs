use astroport::asset::{Asset, AssetInfo, MINIMUM_LIQUIDITY_AMOUNT};
use astroport::U256;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Order, Response, StdError, StdResult, Uint128, WasmMsg,
};
use std::str::FromStr;

use astroport::vesting::{
    ExecuteMsg as VestingExecuteMsg, VestingAccount, VestingSchedule, VestingSchedulePoint,
};
use astroport_periphery::auction::{
    CallbackMsg, Config, ExecuteMsg, InstantiateMsg, MigrateMsg, PoolBalance, PoolInfo, QueryMsg,
    State, UpdateConfigMsg, UserInfoResponse, UserLpInfo,
};
use astroport_periphery::lockdrop::{
    Cw20HookMsg as LockDropCw20HookMsg, ExecuteMsg as LockDropExecuteMsg,
    PoolType as LockDropPoolType,
};

use crate::state::{get_users_store, CONFIG, STATE};
use astroport::querier::query_token_balance;
use astroport_periphery::pricefeed::{PriceFeedRate, QueryMsg as PriceFeedQueryMsg};
use cw2::set_contract_version;
use cw20::Cw20ExecuteMsg;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "auction";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const UNTRN_DENOM: &str = "untrn";

/// ## Description
/// Creates a new contract with the specified parameters
/// Returns the [`Response`] with the specified attributes if the operation was successful, or a [`StdError`] if
/// the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    // CHECK :: init_timestamp needs to be valid
    if env.block.time.seconds() > msg.init_timestamp {
        return Err(StdError::generic_err(format!(
            "Invalid init_timestamp. Current timestamp : {}",
            env.block.time.seconds()
        )));
    }

    let lockdrop_contract_address =
        if let Some(lockdrop_contract_address) = msg.lockdrop_contract_address {
            Some(deps.api.addr_validate(&lockdrop_contract_address)?)
        } else {
            None
        };

    let config = Config {
        owner: msg
            .owner
            .map(|v| deps.api.addr_validate(&v))
            .transpose()?
            .unwrap_or(info.sender),
        token_info_manager: deps.api.addr_validate(&msg.token_info_manager)?,
        lockdrop_contract_address,
        price_feed_contract: deps.api.addr_validate(msg.price_feed_contract.as_str())?,
        reserve_contract_address: deps.api.addr_validate(&msg.reserve_contract_address)?,
        vesting_usdc_contract_address: deps
            .api
            .addr_validate(&msg.vesting_usdc_contract_address)?,
        vesting_atom_contract_address: deps
            .api
            .addr_validate(&msg.vesting_atom_contract_address)?,
        pool_info: None,
        lp_tokens_lock_window: msg.lp_tokens_lock_window,
        init_timestamp: msg.init_timestamp,
        deposit_window: msg.deposit_window,
        withdrawal_window: msg.withdrawal_window,
        usdc_denom: None,
        atom_denom: None,
        ntrn_denom: UNTRN_DENOM.to_string(),
        max_exchange_rate_age: msg.max_exchange_rate_age,
        min_ntrn_amount: msg.min_ntrn_amount,
        vesting_migration_pack_size: msg.vesting_migration_pack_size,
        vesting_lp_duration: msg.vesting_lp_duration,
    };

    CONFIG.save(deps.storage, &config)?;
    STATE.save(deps.storage, &State::default())?;

    Ok(Response::default())
}

/// ## Description
/// Exposes all the execute functions available in the contract.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, StdError> {
    match msg {
        ExecuteMsg::UpdateConfig { new_config } => execute_update_config(deps, info, new_config),
        ExecuteMsg::SetTokenInfo {
            usdc_denom,
            atom_denom,
            pool_info,
        } => execute_set_token_info(deps, info, usdc_denom, atom_denom, pool_info),
        ExecuteMsg::Deposit {} => execute_deposit(deps, env, info),
        ExecuteMsg::Withdraw {
            amount_usdc,
            amount_atom,
        } => execute_withdraw(deps, env, info, amount_usdc, amount_atom),
        ExecuteMsg::SetPoolSize {} => execute_set_pool_size(deps, env, info),
        ExecuteMsg::InitPool {} => execute_init_pool(deps, env, info),
        ExecuteMsg::LockLp {
            asset,
            amount,
            duration,
        } => execute_lock_lp_tokens(deps, env, info, asset, amount, duration),
        ExecuteMsg::WithdrawLp {
            asset,
            amount,
            duration,
        } => execute_withdraw_lp_tokens(deps, env, info, asset, amount, duration),
        ExecuteMsg::MigrateToVesting {} => execute_migrate_to_vesting(deps, env, info),
        ExecuteMsg::Callback(msg) => execute_callback(deps, env, info, msg),
    }
}

fn execute_callback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> Result<Response, StdError> {
    match msg {
        CallbackMsg::FinalizePoolInitialization { prev_lp_balance } => {
            execute_finalize_init_pool(deps, env, info, prev_lp_balance)
        }
    }
}

pub fn execute_deposit(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let (usdc_denom, atom_denom) = get_denoms(&config)?;
    let users_store = get_users_store();

    // CHECK :: Auction deposit window open
    if !is_deposit_open(env.block.time.seconds(), &config) {
        return Err(StdError::generic_err("Deposit window closed"));
    }

    let mut usdc_amount = Uint128::zero();
    let mut atom_amount = Uint128::zero();

    for fund in info.funds.iter() {
        if fund.denom == usdc_denom {
            usdc_amount = fund.amount;
        } else if fund.denom == atom_denom {
            atom_amount = fund.amount;
        } else {
            return Err(StdError::generic_err(format!(
                "Invalid denom. Expected {} or {}",
                usdc_denom, atom_denom
            )));
        }
    }
    if usdc_amount.is_zero() && atom_amount.is_zero() {
        return Err(StdError::generic_err(format!(
            "You must send at least one of {} or {}",
            usdc_denom, atom_denom
        )));
    }

    let mut state = STATE.load(deps.storage)?;
    let mut user_info = users_store
        .may_load(deps.storage, &info.sender)?
        .unwrap_or_default();

    // UPDATE STATE
    state.total_usdc_deposited += usdc_amount;
    state.total_atom_deposited += atom_amount;
    user_info.usdc_deposited += usdc_amount;
    user_info.atom_deposited += atom_amount;

    // SAVE UPDATED STATE
    STATE.save(deps.storage, &state)?;
    users_store.save(deps.storage, &info.sender, &user_info)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "Auction::ExecuteMsg::Deposit"),
        attr("user", info.sender.to_string()),
        attr("usdc_deposited", usdc_amount),
        attr("atom_deposited", atom_amount),
    ]))
}

/// Exposes all the queries available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::State {} => to_binary(&STATE.load(deps.storage)?),
        QueryMsg::UserInfo { address } => to_binary(&query_user_info(deps, _env, address)?),
    }
}

/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

/// Admin function to update any of the configuration parameters. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **new_config** is an object of type [`UpdateConfigMsg`].
pub fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_config: UpdateConfigMsg,
) -> StdResult<Response> {
    let mut config = CONFIG.load(deps.storage)?;
    let mut attributes = vec![attr("action", "update_config")];

    // CHECK :: ONLY OWNER CAN CALL THIS FUNCTION
    if info.sender != config.owner {
        return Err(StdError::generic_err("Only owner can update configuration"));
    }

    // UPDATE :: ADDRESSES IF PROVIDED
    if let Some(owner) = new_config.owner {
        config.owner = deps.api.addr_validate(&owner)?;
        attributes.push(attr("owner", config.owner.to_string()));
    }
    if let Some(lockdrop_contract_address) = new_config.lockdrop_contract_address {
        config.lockdrop_contract_address =
            Some(deps.api.addr_validate(&lockdrop_contract_address)?);
        attributes.push(attr("lockdrop_contract_address", lockdrop_contract_address));
    }
    if let Some(price_feed_contract) = new_config.price_feed_contract {
        config.price_feed_contract = deps.api.addr_validate(&price_feed_contract)?;
        attributes.push(attr(
            "price_feed_contract",
            config.price_feed_contract.to_string(),
        ));
    }
    if let Some(vesting_migration_pack_size) = new_config.vesting_migration_pack_size {
        config.vesting_migration_pack_size = vesting_migration_pack_size;
        attributes.push(attr(
            "vesting_migration_pack_size",
            config.vesting_migration_pack_size.to_string(),
        ));
    }
    if let Some(pool_info) = new_config.pool_info {
        deps.api
            .addr_validate(pool_info.ntrn_usdc_pool_address.as_str())?;
        deps.api
            .addr_validate(pool_info.ntrn_atom_pool_address.as_str())?;
        deps.api
            .addr_validate(pool_info.ntrn_usdc_lp_token_address.as_str())?;
        deps.api
            .addr_validate(pool_info.ntrn_atom_lp_token_address.as_str())?;
        config.pool_info = Some(pool_info);
        attributes.push(attr("pool_info", format!("{:?}", config.pool_info)));
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attributes(attributes))
}

/// Returns a boolean value indicating if the deposit is open.
/// ## Params
/// * **current_timestamp** is an object of type [`u64`].
///
/// * **config** is an object of type [`Config`].
fn is_deposit_open(current_timestamp: u64, config: &Config) -> bool {
    current_timestamp >= config.init_timestamp
        && current_timestamp < config.init_timestamp + config.deposit_window
}

pub fn execute_set_token_info(
    deps: DepsMut,
    info: MessageInfo,
    usdc_denom: Option<String>,
    atom_denom: Option<String>,
    pool_info: Option<PoolInfo>,
) -> StdResult<Response> {
    let mut config = CONFIG.load(deps.storage)?;
    let mut attributes = vec![attr("action", "set_denoms")];

    if info.sender != config.token_info_manager && info.sender != config.owner {
        return Err(StdError::generic_err(
            "Only owner and denom_manager can update denoms",
        ));
    }
    if let Some(usdc_denom) = usdc_denom {
        config.usdc_denom = Some(usdc_denom.clone());
        attributes.push(attr("new_usdc_denom", usdc_denom));
    }
    if let Some(atom_denom) = atom_denom {
        config.atom_denom = Some(atom_denom.clone());
        attributes.push(attr("new_atom_denom", atom_denom));
    }
    if let Some(pool_info) = pool_info {
        deps.api.addr_validate(&pool_info.ntrn_usdc_pool_address)?;
        deps.api.addr_validate(&pool_info.ntrn_atom_pool_address)?;
        deps.api
            .addr_validate(&pool_info.ntrn_usdc_lp_token_address)?;
        deps.api
            .addr_validate(&pool_info.ntrn_atom_lp_token_address)?;
        config.pool_info = Some(pool_info);
        attributes.push(attr("pool_info", format!("{:?}", config.pool_info)));
    }
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attributes(attributes))
}

/// Facilitates Opposite (native) token withdrawals by users. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **amount** is an object of type [`Uint128`].
pub fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount_usdc: Uint128,
    amount_atom: Uint128,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let (usdc_denom, atom_denom) = get_denoms(&config)?;
    let mut state = STATE.load(deps.storage)?;
    let user_address = info.sender;
    let users_store = get_users_store();
    let mut user_info = users_store.load(deps.storage, &user_address)?;

    // CHECK :: Has the user already withdrawn during the current window
    if user_info.withdrawn {
        return Err(StdError::generic_err("Max 1 withdrawal allowed"));
    }

    // Check :: Amount should be within the allowed withdrawal limit bounds
    let max_withdrawal_percent = allowed_withdrawal_percent(env.block.time.seconds(), &config);
    let max_allowed_usdc = user_info.usdc_deposited * max_withdrawal_percent;
    let max_allowed_atom = user_info.atom_deposited * max_withdrawal_percent;

    if amount_usdc > max_allowed_usdc || amount_atom > max_allowed_atom {
        return Err(StdError::generic_err(format!(
            "Amount exceeds maximum allowed withdrawal limit of {}",
            max_withdrawal_percent
        )));
    }

    if amount_usdc.is_zero() && amount_atom.is_zero() {
        return Err(StdError::generic_err(
            "At least one token must be withdrawn",
        ));
    }

    // After deposit window is closed, we allow to withdraw only once
    if env.block.time.seconds() >= config.init_timestamp + config.deposit_window {
        user_info.withdrawn = true;
    }

    let mut res = Response::new();

    if amount_usdc.gt(&Uint128::zero()) {
        // Transfer Native tokens to the user
        let transfer_msg = CosmosMsg::Bank(BankMsg::Send {
            to_address: user_address.to_string(),
            amount: vec![Coin {
                denom: usdc_denom,
                amount: amount_usdc,
            }],
        });
        res = res.add_message(transfer_msg);
    }

    if amount_atom.gt(&Uint128::zero()) {
        // Transfer Native tokens to the user
        let transfer_msg = CosmosMsg::Bank(BankMsg::Send {
            to_address: user_address.to_string(),
            amount: vec![Coin {
                denom: atom_denom,
                amount: amount_atom,
            }],
        });
        res = res.add_message(transfer_msg);
    }

    // UPDATE STATE
    state.total_usdc_deposited -= amount_usdc;
    state.total_atom_deposited -= amount_atom;

    user_info.usdc_deposited -= amount_usdc;
    user_info.atom_deposited -= amount_atom;

    // SAVE UPDATED STATE
    STATE.save(deps.storage, &state)?;
    users_store.save(deps.storage, &user_address, &user_info)?;

    Ok(res.add_attributes(vec![
        attr("action", "Auction::ExecuteMsg::Withdraw"),
        attr("user", user_address.to_string()),
        attr("usdc_withdrawn", amount_usdc),
        attr("atom_withdrawn", amount_atom),
    ]))
}

/// Allow withdrawal percent. Returns a default object of type [`Response`].
/// ## Params
/// * **current_timestamp** is an object of type [`u64`].
///
/// * **config** is an object of type [`Config`].
fn allowed_withdrawal_percent(current_timestamp: u64, config: &Config) -> Decimal {
    let withdrawal_cutoff_init_point = config.init_timestamp + config.deposit_window;

    // Deposit window :: 100% withdrawals allowed
    if current_timestamp < withdrawal_cutoff_init_point {
        return Decimal::from_ratio(100u32, 100u32);
    }

    let withdrawal_cutoff_second_point =
        withdrawal_cutoff_init_point + (config.withdrawal_window / 2u64);
    // Deposit window closed, 1st half of withdrawal window :: 50% withdrawals allowed
    if current_timestamp <= withdrawal_cutoff_second_point {
        return Decimal::from_ratio(50u32, 100u32);
    }

    // max withdrawal allowed decreasing linearly from 50% to 0% vs time elapsed
    let withdrawal_cutoff_final = withdrawal_cutoff_init_point + config.withdrawal_window;
    //  Deposit window closed, 2nd half of withdrawal window :: max withdrawal allowed decreases linearly from 50% to 0% vs time elapsed
    if current_timestamp < withdrawal_cutoff_final {
        let time_left = withdrawal_cutoff_final - current_timestamp;
        Decimal::from_ratio(
            50u64 * time_left,
            100u64 * (withdrawal_cutoff_final - withdrawal_cutoff_second_point),
        )
    } else {
        // Withdrawals not allowed
        Decimal::from_ratio(0u32, 100u32)
    }
}

pub fn get_lp_size(token1: Uint128, token2: Uint128) -> StdResult<Uint128> {
    Uint128::new(
        (U256::from(token1.u128()) * U256::from(token2.u128()))
            .integer_sqrt()
            .as_u128(),
    )
    .checked_sub(MINIMUM_LIQUIDITY_AMOUNT)
    .map_err(|_| StdError::generic_err("LP size is too big"))
}

pub fn get_lp_balances(
    deps: Deps,
    owner_address: &Addr,
    ntrn_usdc_lp_token_address: &String,
    ntrn_atom_lp_token_address: &String,
) -> Result<(Uint128, Uint128), StdError> {
    let usdc_lp_amount =
        query_token_balance(&deps.querier, ntrn_usdc_lp_token_address, owner_address)?;
    let atom_lp_amount =
        query_token_balance(&deps.querier, ntrn_atom_lp_token_address, owner_address)?;
    Ok((usdc_lp_amount, atom_lp_amount))
}

pub fn execute_set_pool_size(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let mut state: State = STATE.load(deps.storage)?;
    // CHECK :: Can be executed once
    if state.lp_usdc_shares_minted.is_some() || state.lp_atom_shares_minted.is_some() {
        return Err(StdError::generic_err("Liquidity already added"));
    }

    // CHECK :: Deposit / withdrawal windows need to be over
    if !are_windows_closed(env.block.time.seconds(), &config) {
        return Err(StdError::generic_err(
            "Deposit/withdrawal windows are still open",
        ));
    }

    if !state.usdc_ntrn_size.is_zero() || !state.atom_ntrn_size.is_zero() {
        return Err(StdError::generic_err("Pool size has already been set"));
    }

    let ntrn_amount = deps
        .querier
        .query_balance(&env.contract.address, config.ntrn_denom)?
        .amount;
    let usdc_amount = state.total_usdc_deposited;
    let atom_amount = state.total_atom_deposited;

    let exchange_data: Vec<PriceFeedRate> = deps
        .querier
        .query_wasm_smart(config.price_feed_contract, &PriceFeedQueryMsg::GetRate {})?;

    if exchange_data.len() != 2 {
        return Err(StdError::generic_err("Invalid price feed data"));
    }

    if exchange_data[0].resolve_time.u64() < env.block.time.seconds() - config.max_exchange_rate_age
    {
        return Err(StdError::generic_err("Price feed data is too old"));
    }

    if ntrn_amount < config.min_ntrn_amount {
        return Err(StdError::generic_err(format!(
            "Not enough NTRN in the contract. Min NTRN amount: {}",
            config.min_ntrn_amount
        )));
    }

    let usdc_to_atom_rate = Decimal::from_ratio(exchange_data[1].rate, exchange_data[0].rate);
    let atom_in_usdc = Decimal::from_str(&atom_amount.to_string())? / usdc_to_atom_rate;
    let all_in_usdc = Uint128::checked_add(usdc_amount, atom_in_usdc.to_uint_floor())?;
    let div_ratio = Decimal::from_ratio(usdc_amount, all_in_usdc);
    let usdc_ntrn_size = ntrn_amount * div_ratio;
    let atom_ntrn_size = ntrn_amount - usdc_ntrn_size;
    let atom_lp_size = get_lp_size(atom_ntrn_size, atom_amount)?;
    let usdc_lp_size = get_lp_size(usdc_ntrn_size, usdc_amount)?;

    // UPDATE STATE
    state.usdc_ntrn_size = usdc_ntrn_size;
    state.atom_ntrn_size = atom_ntrn_size;
    state.atom_lp_size = atom_lp_size;
    state.usdc_lp_size = usdc_lp_size;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "Auction::ExecuteMsg::SetPoolSize"),
        attr("div_ratio", div_ratio.to_string()),
        attr("atom", exchange_data[0].rate),
        attr("usdc", exchange_data[1].rate),
        attr("all_in_usdc", all_in_usdc),
        attr("usdc_to_atom_rate", usdc_to_atom_rate.to_string()),
        attr("usdc_ntrn_size", usdc_ntrn_size),
        attr("atom_ntrn_size", atom_ntrn_size),
        attr("usdc_lp_size", usdc_lp_size),
        attr("atom_lp_size", atom_lp_size),
    ]))
}

/// Facilitates Liquidity addtion to the Astroport NTRN-NATIVE Pool. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
pub fn execute_init_pool(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let (usdc_denom, atom_denom) = get_denoms(&config)?;
    let state = STATE.load(deps.storage)?;

    // CHECK :: Can be executed once
    if state.lp_usdc_shares_minted.is_some() || state.lp_atom_shares_minted.is_some() {
        return Err(StdError::generic_err("Liquidity already added"));
    }

    if state.usdc_lp_size.is_zero() && state.atom_lp_size.is_zero() {
        return Err(StdError::generic_err("Pool size has not been set"));
    }

    if !is_lock_window_closed(env.block.time.seconds(), &config) {
        return Err(StdError::generic_err("Lock window is still open!"));
    }

    let mut msgs = vec![];
    if let Some(PoolInfo {
        ntrn_usdc_pool_address,
        ntrn_atom_pool_address,
        ntrn_usdc_lp_token_address,
        ntrn_atom_lp_token_address,
    }) = config.pool_info
    {
        // QUERY CURRENT LP TOKEN BALANCE (FOR SAFETY - IN ANY CASE)
        let (cur_usdc_lp_balance, cur_atom_lp_balance) = get_lp_balances(
            deps.as_ref(),
            &env.contract.address,
            &ntrn_usdc_lp_token_address,
            &ntrn_atom_lp_token_address,
        )?;

        msgs.push(build_provide_liquidity_to_lp_pool_msg(
            ntrn_usdc_pool_address,
            state.usdc_ntrn_size,
            config.ntrn_denom.clone(),
            state.total_usdc_deposited,
            usdc_denom,
        )?);
        msgs.push(build_provide_liquidity_to_lp_pool_msg(
            ntrn_atom_pool_address,
            state.atom_ntrn_size,
            config.ntrn_denom,
            state.total_atom_deposited,
            atom_denom,
        )?);
        msgs.push(
            CallbackMsg::FinalizePoolInitialization {
                prev_lp_balance: PoolBalance {
                    atom: cur_atom_lp_balance,
                    usdc: cur_usdc_lp_balance,
                },
            }
            .to_cosmos_msg(&env)?,
        );

        Ok(Response::new()
            .add_messages(msgs)
            .add_attributes(vec![attr("action", "Auction::ExecuteMsg::InitPool")]))
    } else {
        Err(StdError::generic_err("Pool info isn't set yet!"))
    }
}

pub fn execute_finalize_init_pool(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    prev_lp_balance: PoolBalance,
) -> Result<Response, StdError> {
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("Unauthorized"));
    }

    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;
    let lockdrop_address = config.lockdrop_contract_address.ok_or_else(|| {
        StdError::generic_err("Lockdrop address is not set yet. Please set it first.")
    })?;
    if let Some(PoolInfo {
        ntrn_usdc_pool_address: _,
        ntrn_atom_pool_address: _,
        ntrn_usdc_lp_token_address,
        ntrn_atom_lp_token_address,
    }) = config.pool_info
    {
        let (cur_usdc_lp_balance, cur_atom_lp_balance) = get_lp_balances(
            deps.as_ref(),
            &env.contract.address,
            &ntrn_usdc_lp_token_address,
            &ntrn_atom_lp_token_address,
        )?;

        // send 50% of lp tokens to the reserve
        let usdc_lp_to_reserve =
            (cur_usdc_lp_balance - prev_lp_balance.usdc) / Uint128::from(2u128);
        let atom_lp_to_reserve =
            (cur_atom_lp_balance - prev_lp_balance.atom) / Uint128::from(2u128);

        state.lp_atom_shares_minted = Some(cur_atom_lp_balance - prev_lp_balance.atom);
        state.lp_usdc_shares_minted = Some(cur_usdc_lp_balance - prev_lp_balance.usdc);
        state.pool_init_timestamp = env.block.time.seconds();
        STATE.save(deps.storage, &state)?;

        let mut msgs = vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ntrn_usdc_lp_token_address.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: config.reserve_contract_address.to_string(),
                    amount: usdc_lp_to_reserve,
                })?,
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ntrn_atom_lp_token_address.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: config.reserve_contract_address.to_string(),
                    amount: atom_lp_to_reserve,
                })?,
            }),
        ];

        // Send locked tokens to the lockdrop contract
        if !state.atom_lp_locked.is_zero() {
            msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ntrn_atom_lp_token_address,
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: lockdrop_address.to_string(),
                    amount: state.atom_lp_locked,
                    msg: to_binary(&LockDropCw20HookMsg::InitializePool {
                        pool_type: LockDropPoolType::ATOM,
                        incentives_share: state.atom_ntrn_size,
                    })?,
                })?,
            }))
        }
        if !state.usdc_lp_locked.is_zero() {
            msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: ntrn_usdc_lp_token_address,
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: lockdrop_address.to_string(),
                    amount: state.usdc_lp_locked,
                    msg: to_binary(&LockDropCw20HookMsg::InitializePool {
                        pool_type: LockDropPoolType::USDC,
                        incentives_share: state.usdc_ntrn_size,
                    })?,
                })?,
            }))
        }

        Ok(Response::new().add_messages(msgs).add_attributes(vec![
            attr("action", "Auction::ExecuteMsg::FinalizePoolInitialization"),
            attr("usdc_lp_to_reserve", usdc_lp_to_reserve),
            attr("atom_lp_to_reserve", atom_lp_to_reserve),
        ]))
    } else {
        Err(StdError::generic_err("Pool info isn't set yet!"))
    }
}

fn execute_migrate_to_vesting(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;
    let users_store = get_users_store();
    let users = users_store
        .idx
        .vested
        .prefix(0u8)
        .range(deps.storage, None, None, Order::Ascending)
        .take(config.vesting_migration_pack_size.into())
        .collect::<StdResult<Vec<_>>>()?;
    let pool_info = config
        .pool_info
        .ok_or_else(|| StdError::generic_err("Pool info isn't set yet. Please set it first."))?;

    if state.pool_init_timestamp == 0 {
        return Err(StdError::generic_err("Pool isn't initialized yet!"));
    }
    if users.is_empty() {
        return Err(StdError::generic_err("No users to migrate!"));
    }
    let mut atom_users: Vec<_> = vec![];
    let mut usdc_users: Vec<_> = vec![];
    let mut atom_lp_amount = Uint128::zero();
    let mut usdc_lp_amount = Uint128::zero();

    for (user_addr, mut user) in users {
        let user_lp_balance = get_user_lp_info(
            user.usdc_deposited,
            user.atom_deposited,
            state.total_usdc_deposited,
            state.total_atom_deposited,
            state.usdc_lp_size,
            state.atom_lp_size,
        );

        let vest_atom_lp_amount = user_lp_balance.atom_lp_amount - user.atom_lp_locked;
        let vest_usdc_lp_amount = user_lp_balance.usdc_lp_amount - user.usdc_lp_locked;

        if !vest_atom_lp_amount.is_zero() {
            atom_users.push(VestingAccount {
                address: user_addr.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: env.block.time.seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: env.block.time.seconds() + config.vesting_lp_duration,
                        amount: vest_atom_lp_amount,
                    }),
                }],
            });
            atom_lp_amount += vest_atom_lp_amount;
        }
        if !vest_usdc_lp_amount.is_zero() {
            usdc_users.push(VestingAccount {
                address: user_addr.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: env.block.time.seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: env.block.time.seconds() + config.vesting_lp_duration,
                        amount: vest_usdc_lp_amount,
                    }),
                }],
            });
            usdc_lp_amount += vest_usdc_lp_amount;
        }
        user.is_vested = true;
        users_store.save(deps.storage, &user_addr, &user)?;
    }

    let mut msgs = vec![];
    if !atom_lp_amount.is_zero() {
        msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pool_info.ntrn_atom_lp_token_address,
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: config.vesting_atom_contract_address.to_string(),
                amount: atom_lp_amount,
                msg: to_binary(&VestingExecuteMsg::RegisterVestingAccounts {
                    vesting_accounts: atom_users,
                })?,
            })?,
        }));
    }
    if !usdc_lp_amount.is_zero() {
        msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pool_info.ntrn_usdc_lp_token_address,
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: config.vesting_usdc_contract_address.to_string(),
                amount: usdc_lp_amount,
                msg: to_binary(&VestingExecuteMsg::RegisterVestingAccounts {
                    vesting_accounts: usdc_users,
                })?,
            })?,
        }));
    }

    Ok(Response::new().add_messages(msgs))
}

/// Builds provide liquidity to pool message.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **pool_address** is an object of type [`Addr`].
///
/// * **base_amount** is an object of type [`Uint128`].
///
/// * **base_denom** is an object of type [`String`].
///
/// * **other_amount** is an object of type [`Uint128`].
///
/// * **other_denom** is an object of type [`String`].

fn build_provide_liquidity_to_lp_pool_msg(
    pool_address: String,
    base_amount: Uint128,
    base_denom: String,
    other_amount: Uint128,
    other_denom: String,
) -> StdResult<CosmosMsg> {
    let base = Asset {
        amount: base_amount,
        info: AssetInfo::NativeToken {
            denom: base_denom.clone(),
        },
    };
    let other = Asset {
        amount: other_amount,
        info: AssetInfo::NativeToken {
            denom: other_denom.clone(),
        },
    };
    let mut funds = vec![
        Coin {
            denom: base_denom,
            amount: base_amount,
        },
        Coin {
            denom: other_denom,
            amount: other_amount,
        },
    ];
    funds.sort_by(|a, b| a.denom.cmp(&b.denom));
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pool_address,
        funds,
        msg: to_binary(&astroport::pair::ExecuteMsg::ProvideLiquidity {
            assets: vec![base, other],
            slippage_tolerance: None,
            auto_stake: None,
            receiver: None,
        })?,
    }))
}

pub fn get_user_lp_info(
    user_usdc_deposited: Uint128,
    user_atom_deposited: Uint128,
    total_usdc_deposited: Uint128,
    total_atom_deposited: Uint128,
    total_usdc_lp_tokens: Uint128,
    total_atom_lp_tokens: Uint128,
) -> UserLpInfo {
    let atom_lp_amount = if total_atom_deposited.is_zero() {
        Uint128::zero()
    } else {
        Decimal::from_ratio(user_atom_deposited, total_atom_deposited) * total_atom_lp_tokens
    };
    let usdc_lp_amount = if total_usdc_deposited.is_zero() {
        Uint128::zero()
    } else {
        Decimal::from_ratio(user_usdc_deposited, total_usdc_deposited) * total_usdc_lp_tokens
    };

    UserLpInfo {
        atom_lp_amount: atom_lp_amount / Uint128::from(2_u128),
        usdc_lp_amount: usdc_lp_amount / Uint128::from(2_u128),
    }
}

/// Lock LP tokens with the LockDrop contract.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
pub fn execute_lock_lp_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset: LockDropPoolType,
    amount: Uint128,
    duration: u64,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;
    let users_store = get_users_store();
    let mut user_info = users_store.load(deps.storage, &info.sender)?;

    if state.atom_ntrn_size.is_zero() || state.usdc_ntrn_size.is_zero() {
        return Err(StdError::generic_err("Pool size isn't set yet!"));
    }

    if is_lock_window_closed(env.block.time.seconds(), &config) {
        return Err(StdError::generic_err("Lock window is closed!"));
    }

    let lockdrop_address = config.lockdrop_contract_address.ok_or_else(|| {
        StdError::generic_err("Lockdrop address is not set yet. Please set it first.")
    })?;

    let user_lp_info = get_user_lp_info(
        user_info.usdc_deposited,
        user_info.atom_deposited,
        state.total_usdc_deposited,
        state.total_atom_deposited,
        state.usdc_lp_size,
        state.atom_lp_size,
    );

    match asset {
        LockDropPoolType::USDC => {
            if user_info.usdc_deposited.is_zero() {
                return Err(StdError::generic_err("No USDC deposited!"));
            }
            if amount > user_lp_info.usdc_lp_amount - user_info.usdc_lp_locked {
                return Err(StdError::generic_err("Not enough USDC LP!"));
            }
            user_info.usdc_lp_locked = user_info.usdc_lp_locked.checked_add(amount)?;
            state.usdc_lp_locked = state.usdc_lp_locked.checked_add(amount)?;
        }
        LockDropPoolType::ATOM => {
            if user_info.atom_deposited.is_zero() {
                return Err(StdError::generic_err("No ATOM deposited!"));
            }
            if amount > user_lp_info.atom_lp_amount - user_info.atom_lp_locked {
                return Err(StdError::generic_err("Not enough ATOM LP!"));
            }
            user_info.atom_lp_locked = user_info.atom_lp_locked.checked_add(amount)?;
            state.atom_lp_locked = state.atom_lp_locked.checked_add(amount)?;
        }
    }

    let msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: lockdrop_address.to_string(),
        funds: vec![],
        msg: to_binary(&LockDropExecuteMsg::IncreaseLockupFor {
            user_address: info.sender.to_string(),
            pool_type: asset,
            amount,
            duration,
        })?,
    });

    users_store.save(deps.storage, &info.sender, &user_info)?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_message(msg).add_attributes(vec![
        attr("action", "lock_lp_tokens"),
        attr("asset", asset),
        attr("amount", amount),
        attr("duration", duration.to_string()),
    ]))
}

/// Lock LP tokens with the LockDrop contract back to auction contract
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
pub fn execute_withdraw_lp_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    asset: LockDropPoolType,
    amount: Uint128,
    duration: u64,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;
    let users_store = get_users_store();
    let mut user_info = users_store.load(deps.storage, &info.sender)?;

    if state.atom_ntrn_size.is_zero() || state.usdc_ntrn_size.is_zero() {
        return Err(StdError::generic_err("Pool size isn't set yet!"));
    }

    if is_lock_window_closed(env.block.time.seconds(), &config) {
        return Err(StdError::generic_err("Lock window is closed!"));
    }

    let lockdrop_address = config.lockdrop_contract_address.ok_or_else(|| {
        StdError::generic_err("Lockdrop address is not set yet. Please set it first.")
    })?;

    match asset {
        LockDropPoolType::USDC => {
            user_info.usdc_lp_locked = user_info.usdc_lp_locked.checked_sub(amount)?;
            state.usdc_lp_locked = state.usdc_lp_locked.checked_sub(amount)?;
        }
        LockDropPoolType::ATOM => {
            user_info.atom_lp_locked = user_info.atom_lp_locked.checked_sub(amount)?;
            state.atom_lp_locked = state.atom_lp_locked.checked_sub(amount)?;
        }
    }

    let msg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: lockdrop_address.to_string(),
        funds: vec![],
        msg: to_binary(&LockDropExecuteMsg::WithdrawFromLockup {
            user_address: info.sender.to_string(),
            pool_type: asset,
            amount,
            duration,
        })?,
    });

    users_store.save(deps.storage, &info.sender, &user_info)?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_message(msg).add_attributes(vec![
        attr("action", "withdraw_lp_tokens"),
        attr("asset", asset),
        attr("amount", amount),
        attr("duration", duration.to_string()),
    ]))
}

/// Returns true if the deposit & withdrawal windows are closed, else returns false
/// ## Params
/// * **current_timestamp** is an object of type [`u64`].
///
/// * **config** is an object of type [`Config`].
fn are_windows_closed(current_timestamp: u64, config: &Config) -> bool {
    let window_end = config.init_timestamp + config.deposit_window + config.withdrawal_window;
    current_timestamp >= window_end
}

fn is_lock_window_closed(current_timestamp: u64, config: &Config) -> bool {
    let lock_window_end = config.init_timestamp
        + config.deposit_window
        + config.withdrawal_window
        + config.lp_tokens_lock_window;
    current_timestamp >= lock_window_end
}

/// Returns User's Info
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **user_info** is an object of type [`UserInfo`].
fn query_user_info(deps: Deps, _env: Env, user_address: String) -> StdResult<UserInfoResponse> {
    let state = STATE.load(deps.storage)?;
    let users_store = get_users_store();
    let user_address = deps.api.addr_validate(&user_address)?;
    let user_info = users_store
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    let user_lp_info = get_user_lp_info(
        user_info.usdc_deposited,
        user_info.atom_deposited,
        state.total_usdc_deposited,
        state.total_atom_deposited,
        state.usdc_lp_size,
        state.atom_lp_size,
    );

    // User Info Response
    Ok(UserInfoResponse {
        usdc_deposited: user_info.usdc_deposited,
        atom_deposited: user_info.atom_deposited,
        withdrawn: user_info.withdrawn,
        usdc_lp_amount: user_lp_info.usdc_lp_amount,
        atom_lp_amount: user_lp_info.atom_lp_amount,
        atom_lp_locked: user_info.atom_lp_locked,
        usdc_lp_locked: user_info.usdc_lp_locked,
    })
}

fn get_denoms(config: &astroport_periphery::auction::Config) -> StdResult<(String, String)> {
    let usdc_denom = config
        .usdc_denom
        .as_ref()
        .ok_or_else(|| StdError::generic_err("USDC Denom is not set yet. Please set it first."))?;
    let atom_denom = config
        .atom_denom
        .as_ref()
        .ok_or_else(|| StdError::generic_err("ATOM Denom is not set yet. Please set it first."))?;

    Ok((usdc_denom.to_string(), atom_denom.to_string()))
}
