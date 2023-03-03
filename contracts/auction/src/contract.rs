#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, from_binary, to_binary, Addr, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};

use astroport_periphery::airdrop::ExecuteMsg::EnableClaims as AirdropEnableClaims;
use astroport_periphery::auction::{
    CallbackMsg, Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, PoolInfo, QueryMsg,
    State, UpdateConfigMsg, UserInfo, UserInfoResponse,
};
use astroport_periphery::helpers::{build_approve_cw20_msg, cw20_get_balance};

use crate::state::{CONFIG, STATE, USERS};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::generator::{
    ExecuteMsg as GenExecuteMsg, PendingTokenResponse, QueryMsg as GenQueryMsg, RewardInfoResponse,
};
use astroport::pair::QueryMsg as AstroportPairQueryMsg;
use astroport::querier::query_token_balance;
use cw2::set_contract_version;
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg};

/// TerraUSD denom.
const UUSD_DENOM: &str = "uusd";

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport_auction";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
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

    let config = Config {
        owner: msg
            .owner
            .map(|v| deps.api.addr_validate(&v))
            .transpose()?
            .unwrap_or(info.sender),
        astro_token_address: deps.api.addr_validate(&msg.astro_token_address)?,
        airdrop_contract_address: deps.api.addr_validate(&msg.airdrop_contract_address)?,
        lockdrop_contract_address: deps.api.addr_validate(&msg.lockdrop_contract_address)?,
        pool_info: None,
        generator_contract: None,
        astro_incentive_amount: None,
        lp_tokens_vesting_duration: msg.lp_tokens_vesting_duration,
        init_timestamp: msg.init_timestamp,
        deposit_window: msg.deposit_window,
        withdrawal_window: msg.withdrawal_window,
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
///
/// ## Execute messages
///
/// * **ExecuteMsg::Receive(msg)** Parse incoming messages from the ASTRO token.
///
/// * **ExecuteMsg::UpdateConfig { new_config }** Admin function to update configuration parameters.
///
/// * **ExecuteMsg::DepositUst {}** Facilitates UST deposits by users.
///
/// * **ExecuteMsg::WithdrawUst { amount }** Facilitates UST withdrawals by users.
///
/// * **ExecuteMsg::InitPool { slippage }** Admin function which facilitates Liquidity addtion to the Astroport ASTRO-UST Pool.
///
/// * **ExecuteMsg::StakeLpTokens {}** Admin function to stake ASTRO-UST LP tokens with the generator contract.
///
/// * **ExecuteMsg::ClaimRewards { withdraw_lp_shares }** Facilitates ASTRO rewards claim.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, StdError> {
    match msg {
        ExecuteMsg::UpdateConfig { new_config } => handle_update_config(deps, info, new_config),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::DepositUst {} => handle_deposit_ust(deps, env, info),
        ExecuteMsg::WithdrawUst { amount } => handle_withdraw_ust(deps, env, info, amount),
        ExecuteMsg::InitPool { slippage } => handle_init_pool(deps, env, info, slippage),
        ExecuteMsg::StakeLpTokens {} => handle_stake_lp_tokens(deps, env, info),
        ExecuteMsg::ClaimRewards { withdraw_lp_shares } => {
            handle_claim_rewards_and_withdraw_lp_shares(deps, env, info, withdraw_lp_shares)
        }
        ExecuteMsg::Callback(msg) => handle_callback(deps, env, info, msg),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then an [`StdError`] is returned,
/// otherwise it returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`]. This is the CW20 message that has to be processed.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.astro_token_address {
        return Err(StdError::generic_err("Only astro tokens are received!"));
    }

    // CHECK ::: Amount needs to be valid
    if cw20_msg.amount.is_zero() {
        return Err(StdError::generic_err("Amount must be greater than 0"));
    }

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::DelegateAstroTokens { user_address } => {
            // CHECK :: Delegation can happen only via airdrop / lockdrop contracts
            if cw20_msg.sender == config.airdrop_contract_address
                || cw20_msg.sender == config.lockdrop_contract_address
            {
                handle_delegate_astro_tokens(deps, env, user_address, cw20_msg.amount)
            } else {
                Err(StdError::generic_err("Unauthorized"))
            }
        }
        Cw20HookMsg::IncreaseNTRNIncentives {} => {
            handle_increasing_astro_incentives(deps, cw20_msg.amount)
        }
    }
}

/// ## Description
/// Handles callback. Returns a [`ContractError`] on failure.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`CallbackMsg`].
fn handle_callback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> StdResult<Response> {
    // Callback functions can only be called this contract itself
    if info.sender != env.contract.address {
        return Err(StdError::generic_err(
            "callbacks cannot be invoked externally",
        ));
    }
    match msg {
        CallbackMsg::UpdateStateOnLiquidityAdditionToPool { prev_lp_balance } => {
            update_state_on_liquidity_addition_to_pool(deps, env, prev_lp_balance)
        }
        CallbackMsg::UpdateStateOnRewardClaim { prev_astro_balance } => {
            update_state_on_reward_claim(deps, env, prev_astro_balance)
        }
        CallbackMsg::WithdrawUserRewardsCallback {
            user_address,
            withdraw_lp_shares,
        } => {
            callback_withdraw_user_rewards_and_optionally_lp(deps, user_address, withdraw_lp_shares)
        }
    }
}

/// Exposes all the queries available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns the config info.
///
/// * **QueryMsg::State {}** Returns state of the contract.
///
/// * **QueryMsg::UserInfo { address }** Returns user position details.
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
pub fn handle_update_config(
    deps: DepsMut,
    info: MessageInfo,
    new_config: UpdateConfigMsg,
) -> StdResult<Response> {
    let mut config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;
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

    if let Some(astro_ust_pair_address) = new_config.astro_ust_pair_address {
        if state.lp_shares_minted.is_some() {
            return Err(StdError::generic_err(
                "Assets had already been provided to previous pool!",
            ));
        }
        let astro_ust_pair_addr = deps.api.addr_validate(&astro_ust_pair_address)?;

        let pair_info: PairInfo = deps
            .querier
            .query_wasm_smart(astro_ust_pair_address, &AstroportPairQueryMsg::Pair {})?;

        config.pool_info = Some(PoolInfo {
            astro_ust_pool_address: astro_ust_pair_addr,
            astro_ust_lp_token_address: pair_info.liquidity_token,
        })
    }

    if let Some(generator_contract) = new_config.generator_contract {
        // check if the LP tokens are already staked or not
        if state.is_lp_staked {
            return Err(StdError::generic_err("ASTRO-UST LP tokens already staked"));
        }

        let generator_addr = deps.api.addr_validate(&generator_contract)?;
        config.generator_contract = Some(generator_addr.clone());
        attributes.push(attr("generator", generator_addr.to_string()));
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attributes(attributes))
}

/// Increases ASTRO incentives. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **amount** is an object of type [`Uint128`].
pub fn handle_increasing_astro_incentives(
    deps: DepsMut,
    amount: Uint128,
) -> Result<Response, StdError> {
    let state = STATE.load(deps.storage)?;
    let mut config = CONFIG.load(deps.storage)?;

    if state.lp_shares_minted.is_some() {
        return Err(StdError::generic_err("ASTRO is already being distributed"));
    };

    // Anyone can increase astro incentives

    config.astro_incentive_amount = config
        .astro_incentive_amount
        .map_or(Some(amount), |v| Some(v + amount));

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new()
        .add_attribute("action", "astro_incentives_increased")
        .add_attribute("amount", amount))
}

/// Delegates ASTRO tokens. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **user_address** is an object of type [`String`].
///
/// * **amount** is an object of type [`Uint128`].
pub fn handle_delegate_astro_tokens(
    deps: DepsMut,
    env: Env,
    user_address: String,
    amount: Uint128,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;

    let user_address = deps.api.addr_validate(&user_address)?;

    // CHECK :: Auction deposit window open
    if !is_deposit_open(env.block.time.seconds(), &config) {
        return Err(StdError::generic_err("Deposit window closed"));
    }

    let mut state = STATE.load(deps.storage)?;
    let mut user_info = USERS
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    // UPDATE STATE
    state.total_astro_delegated += amount;
    user_info.astro_delegated += amount;

    // SAVE UPDATED STATE
    STATE.save(deps.storage, &state)?;
    USERS.save(deps.storage, &user_address, &user_info)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "Auction::ExecuteMsg::DelegateAstroTokens"),
        attr("user", user_address.to_string()),
        attr("astro_delegated", amount),
    ]))
}

/// Facilitates UST deposits by users. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
pub fn handle_deposit_ust(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;

    // CHECK :: Auction deposit window open
    if !is_deposit_open(env.block.time.seconds(), &config) {
        return Err(StdError::generic_err("Deposit window closed"));
    }

    let mut state = STATE.load(deps.storage)?;
    let mut user_info = USERS
        .may_load(deps.storage, &info.sender)?
        .unwrap_or_default();

    // Retrieve UST sent by the user
    if info.funds.len() != 1 || info.funds[0].denom != UUSD_DENOM {
        return Err(StdError::generic_err(
            "You may delegate UST native coin only",
        ));
    }

    let fund = &info.funds[0];

    // CHECK ::: Amount needs to be valid
    if fund.amount.is_zero() {
        return Err(StdError::generic_err("Amount must be greater than 0"));
    }

    // UPDATE STATE
    state.total_ust_delegated += fund.amount;
    user_info.ust_delegated += fund.amount;

    // SAVE UPDATED STATE
    STATE.save(deps.storage, &state)?;
    USERS.save(deps.storage, &info.sender, &user_info)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "Auction::ExecuteMsg::DelegateUst"),
        attr("user", info.sender.to_string()),
        attr("ust_delegated", fund.amount),
    ]))
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

/// Facilitates UST withdrawals by users. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **amount** is an object of type [`Uint128`].
pub fn handle_withdraw_ust(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    let user_address = info.sender;

    let mut user_info = USERS.load(deps.storage, &user_address)?;

    // CHECK :: Has the user already withdrawn during the current window
    if user_info.ust_withdrawn {
        return Err(StdError::generic_err("Max 1 withdrawal allowed"));
    }

    // Check :: Amount should be within the allowed withdrawal limit bounds
    let max_withdrawal_percent = allowed_withdrawal_percent(env.block.time.seconds(), &config);
    let max_withdrawal_allowed = user_info.ust_delegated * max_withdrawal_percent;

    if amount > max_withdrawal_allowed {
        return Err(StdError::generic_err(format!(
            "Amount exceeds maximum allowed withdrawal limit of {}",
            max_withdrawal_percent
        )));
    }

    // After deposit window is closed, we allow to withdraw only once
    if env.block.time.seconds() >= config.init_timestamp + config.deposit_window {
        user_info.ust_withdrawn = true;
    }

    // UPDATE STATE
    state.total_ust_delegated -= amount;
    user_info.ust_delegated -= amount;

    // SAVE UPDATED STATE
    STATE.save(deps.storage, &state)?;
    USERS.save(deps.storage, &user_address, &user_info)?;

    // Transfer UST to the user
    let transfer_ust = Asset {
        amount,
        info: AssetInfo::NativeToken {
            denom: String::from(UUSD_DENOM),
        },
    };

    Ok(Response::new()
        .add_attributes(vec![
            attr("action", "Auction::ExecuteMsg::WithdrawUst"),
            attr("user", user_address.to_string()),
            attr("ust_withdrawn", amount),
            attr("ust_commission", transfer_ust.compute_tax(&deps.querier)?),
        ])
        .add_message(transfer_ust.into_msg(&deps.querier, user_address)?))
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

/// Facilitates Liquidity addtion to the Astroport ASTRO-UST Pool. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **slippage** is an optional object of type [`Decimal`].
pub fn handle_init_pool(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    slippage: Option<Decimal>,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;

    // CHECK :: Only admin can call this function
    if info.sender != config.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    // CHECK :: Can be executed once
    if state.lp_shares_minted.is_some() {
        return Err(StdError::generic_err("Liquidity already added"));
    }

    // CHECK :: Deposit / withdrawal windows need to be over
    if !are_windows_closed(env.block.time.seconds(), &config) {
        return Err(StdError::generic_err(
            "Deposit/withdrawal windows are still open",
        ));
    }

    let mut msgs = vec![];

    if let Some(PoolInfo {
        astro_ust_pool_address,
        astro_ust_lp_token_address,
    }) = config.pool_info
    {
        let ust_coin = deps
            .querier
            .query_balance(&env.contract.address, UUSD_DENOM)?;

        // QUERY CURRENT LP TOKEN BALANCE (FOR SAFETY - IN ANY CASE)
        let cur_lp_balance = query_token_balance(
            &deps.querier,
            astro_ust_lp_token_address,
            env.contract.address.clone(),
        )?;

        // COSMOS MSGS
        // :: 1.  APPROVE ASTRO WITH LP POOL ADDRESS AS BENEFICIARY
        // :: 2.  ADD LIQUIDITY
        // :: 3. CallbackMsg :: Update state on liquidity addition to LP Pool
        msgs.push(build_approve_cw20_msg(
            config.astro_token_address.to_string(),
            astro_ust_pool_address.to_string(),
            state.total_astro_delegated,
            env.block.height + 1u64,
        )?);

        msgs.push(build_provide_liquidity_to_lp_pool_msg(
            deps.as_ref(),
            config.astro_token_address,
            astro_ust_pool_address,
            ust_coin.amount,
            state.total_astro_delegated,
            slippage,
        )?);

        msgs.push(
            CallbackMsg::UpdateStateOnLiquidityAdditionToPool {
                prev_lp_balance: cur_lp_balance,
            }
            .to_cosmos_msg(&env)?,
        );
        Ok(Response::new().add_messages(msgs).add_attributes(vec![
            attr("action", "Auction::ExecuteMsg::AddLiquidityToAstroportPool"),
            attr("astro_provided", state.total_astro_delegated),
            attr("ust_provided", ust_coin.amount),
        ]))
    } else {
        Err(StdError::generic_err("Pool info isn't set yet!"))
    }
}

/// Builds provide liquidity to pool message.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **astro_token_address** is an object of type [`Addr`].
///
/// * **astro_ust_pool_address** is an object of type [`Addr`].
///
/// * **ust_amount** is an object of type [`Uint128`].
///
/// * **astro_amount** is an object of type [`Uint128`].
///
/// * **slippage_tolerance** is an optional object of type [`Decimal`].
fn build_provide_liquidity_to_lp_pool_msg(
    deps: Deps,
    astro_token_address: Addr,
    astro_ust_pool_address: Addr,
    ust_amount: Uint128,
    astro_amount: Uint128,
    slippage_tolerance: Option<Decimal>,
) -> StdResult<CosmosMsg> {
    let astro = Asset {
        amount: astro_amount,
        info: AssetInfo::Token {
            contract_addr: astro_token_address,
        },
    };

    let mut ust = Asset {
        amount: ust_amount,
        info: AssetInfo::NativeToken {
            denom: String::from(UUSD_DENOM),
        },
    };

    // Deduct tax
    ust.amount = ust.amount.checked_sub(ust.compute_tax(&deps.querier)?)?;

    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: astro_ust_pool_address.to_string(),
        funds: vec![Coin {
            denom: String::from(UUSD_DENOM),
            amount: ust.amount,
        }],
        msg: to_binary(&astroport::pair::ExecuteMsg::ProvideLiquidity {
            assets: [ust, astro].to_vec(),
            slippage_tolerance,
            auto_stake: None,
            receiver: None,
        })?,
    }))
}

/// Stakes ASTRO-UST LP tokens with the generator contract.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
pub fn handle_stake_lp_tokens(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    // CHECK :: Only admin can call this function
    if info.sender != config.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    let generator = config
        .generator_contract
        .ok_or_else(|| StdError::generic_err("Generator should be set!"))?;

    // CHECK :: Can be staked only once
    if state.is_lp_staked {
        return Err(StdError::generic_err("Already staked"));
    }

    let lp_shares_minted = state
        .lp_shares_minted
        .ok_or_else(|| StdError::generic_err("Should be provided to the ASTRO/UST pool!"))?;

    if let Some(PoolInfo {
        astro_ust_lp_token_address,
        astro_ust_pool_address: _,
    }) = config.pool_info
    {
        // QUERY CURRENT LP TOKEN BALANCE (FOR SAFETY - IN ANY CASE)
        let cur_lp_balance = query_token_balance(
            &deps.querier,
            astro_ust_lp_token_address.clone(),
            env.contract.address.clone(),
        )?;

        // Init response
        let mut response = Response::new()
            .add_attribute("action", "Auction::ExecuteMsg::StakeLPTokens")
            .add_attribute("staked_amount", lp_shares_minted);

        // COSMOS MSGs
        // :: Add increase allowance msg so generator contract can transfer tokens to itself
        // :: To stake LP Tokens to the Astroport generator contract
        response.messages.push(SubMsg::new(build_approve_cw20_msg(
            astro_ust_lp_token_address.to_string(),
            generator.to_string(),
            cur_lp_balance,
            env.block.height + 1u64,
        )?));
        response.messages.push(SubMsg::new(WasmMsg::Execute {
            contract_addr: astro_ust_lp_token_address.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: generator.to_string(),
                msg: to_binary(&astroport::generator::Cw20HookMsg::Deposit {})?,
                amount: cur_lp_balance,
            })?,
        }));

        state.is_lp_staked = true;
        STATE.save(deps.storage, &state)?;

        Ok(response)
    } else {
        Err(StdError::generic_err("Pool info isn't set yet!"))
    }
}

/// Facilitates ASTRO Reward claim for users.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **withdraw_lp_shares** is an optional object of type [`Uint128`].
pub fn handle_claim_rewards_and_withdraw_lp_shares(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    withdraw_lp_shares: Option<Uint128>,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;
    let user_address = info.sender;
    let mut user_info = USERS.load(deps.storage, &user_address)?;

    // CHECK :: User has valid delegation / deposit balances
    if user_info.astro_delegated.is_zero() && user_info.ust_delegated.is_zero() {
        return Err(StdError::generic_err("No delegated assets"));
    }

    let mut cosmos_msgs = vec![];

    if let Some(lp_balance) = state.lp_shares_minted {
        // Calculate user's LP shares & ASTRO incentives (if possible)
        if user_info.lp_shares.is_none() {
            update_user_lp_shares(&state, lp_balance, &mut user_info)?;
            update_user_astro_incentives(
                config.astro_incentive_amount,
                user_info.lp_shares,
                lp_balance,
                &mut user_info,
            )?;
            USERS.save(deps.storage, &user_address, &user_info)?;
        }
        // If user's ASTRO incentives are not set, but the total ASTRO incentives have been set
        if config.astro_incentive_amount.is_some() && user_info.auction_incentive_amount.is_none() {
            update_user_astro_incentives(
                config.astro_incentive_amount,
                user_info.lp_shares,
                lp_balance,
                &mut user_info,
            )?;
            USERS.save(deps.storage, &user_address, &user_info)?;
        }

        // If user wants to withdraw LP tokens, then we calculate the max amount he can withdraw for check
        if let Some(withdraw_lp_shares) = withdraw_lp_shares {
            let max_withdrawable = calculate_withdrawable_lp_shares(
                env.block.time.seconds(),
                &config,
                &state,
                &user_info,
            )?;
            if max_withdrawable.is_none() || withdraw_lp_shares > max_withdrawable.unwrap() {
                return Err(StdError::generic_err(
                    "No available LP shares to withdraw / Invalid amount",
                ));
            }
        }

        if state.is_lp_staked {
            let generator = config
                .generator_contract
                .ok_or_else(|| StdError::generic_err("Generator should be set!"))?;

            if let Some(PoolInfo {
                astro_ust_pool_address: _,
                astro_ust_lp_token_address,
            }) = config.pool_info
            {
                // QUERY :: Check if there are any pending staking rewards
                let pending_rewards: PendingTokenResponse = deps.querier.query_wasm_smart(
                    &generator,
                    &GenQueryMsg::PendingToken {
                        lp_token: astro_ust_lp_token_address.to_string(),
                        user: env.contract.address.to_string(),
                    },
                )?;

                if !pending_rewards.pending.is_zero()
                    || (pending_rewards.pending_on_proxy.is_some()
                        && !pending_rewards.pending_on_proxy.unwrap().is_empty())
                {
                    let rwi: RewardInfoResponse = deps.querier.query_wasm_smart(
                        &generator,
                        &GenQueryMsg::RewardInfo {
                            lp_token: astro_ust_lp_token_address.to_string(),
                        },
                    )?;

                    let astro_balance = {
                        let res: BalanceResponse = deps.querier.query_wasm_smart(
                            rwi.base_reward_token.to_string(),
                            &Cw20QueryMsg::Balance {
                                address: env.contract.address.to_string(),
                            },
                        )?;
                        res.balance
                    };

                    cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: generator.to_string(),
                        funds: vec![],
                        msg: to_binary(&GenExecuteMsg::Withdraw {
                            lp_token: astro_ust_lp_token_address.to_string(),
                            amount: Uint128::zero(),
                        })?,
                    }));

                    cosmos_msgs.push(
                        CallbackMsg::UpdateStateOnRewardClaim {
                            prev_astro_balance: astro_balance,
                        }
                        .to_cosmos_msg(&env)?,
                    );
                }
            } else {
                return Err(StdError::generic_err("Pool info isn't set yet!"));
            }
        }
        // If no rewards to claim and no LP tokens to be withdrawn.
        else if user_info.astro_incentive_transferred && withdraw_lp_shares.is_none() {
            return Err(StdError::generic_err(
                "Rewards already claimed. Provide number of LP tokens to claim!",
            ));
        }
    } else {
        return Err(StdError::generic_err(
            "Astro/USD should be provided to the pool!",
        ));
    };

    cosmos_msgs.push(
        CallbackMsg::WithdrawUserRewardsCallback {
            user_address,
            withdraw_lp_shares,
        }
        .to_cosmos_msg(&env)?,
    );

    Ok(Response::new().add_messages(cosmos_msgs))
}

/// Calculates user's ASTRO - UST LP shares based on amount delegated.
/// User LP shares (ASTRO delegation share) = (1/2) *  (ASTRO delegated / total ASTRO delegated)
/// User LP shares (UST deposit share) = (1/2) *  (UST deposited / total UST deposited)
/// User's total LP shares  = User's ASTRO delegation LP share + User's UST deposit LP share
/// ## Params
/// * **state** is an object of type [`State`].
///
/// * **lp_balance** is an object of type [`Uint128`].
///
/// * **user_info** is an object of type [`UserInfo`].
fn update_user_lp_shares(
    state: &State,
    lp_balance: Uint128,
    mut user_info: &mut UserInfo,
) -> StdResult<()> {
    let user_lp_share = (Decimal::from_ratio(
        user_info.astro_delegated,
        state.total_astro_delegated * Uint128::new(2),
    ) + Decimal::from_ratio(
        user_info.ust_delegated,
        state.total_ust_delegated * Uint128::new(2),
    )) * lp_balance;
    user_info.lp_shares = Some(user_lp_share);

    Ok(())
}

/// Calculates user's ASTRO incentives for auction participation
/// Formula, == User's auction incentives (ASTRO) = (User's total LP shares / Total LP shares minted) * Total ASTRO auction incentives
/// ## Params
/// * **total_astro_incentives** is an optional object of type [`Uint128`].
///
/// * **user_lp_share** is an optional object of type [`Uint128`].
///
/// * **lp_balance** is an object of type [`Uint128`].
///
/// * **user_info** is an object of type [`UserInfo`].
fn update_user_astro_incentives(
    total_astro_incentives: Option<Uint128>,
    user_lp_share: Option<Uint128>,
    lp_balance: Uint128,
    mut user_info: &mut UserInfo,
) -> StdResult<()> {
    if let Some(total_astro_incentives) = total_astro_incentives {
        if let Some(user_lp_share) = user_lp_share {
            user_info.auction_incentive_amount =
                Some(Decimal::from_ratio(user_lp_share, lp_balance) * total_astro_incentives);
        }
    }
    Ok(())
}

/// Withdraws user rewards and LP Tokens if available
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **user_address** is an object of type [`Addr`].
///
/// * **withdraw_lp_shares** is an optional object of type [`Uint128`].
pub fn callback_withdraw_user_rewards_and_optionally_lp(
    deps: DepsMut,
    user_address: Addr,
    withdraw_lp_shares: Option<Uint128>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;
    let mut user_info = USERS.load(deps.storage, &user_address)?;

    let mut cosmos_msgs = vec![];
    let mut attributes = vec![
        attr("action", "Withdraw rewards and lp tokens"),
        attr("user_address", &user_address),
    ];

    if let Some(PoolInfo {
        astro_ust_pool_address: _,
        astro_ust_lp_token_address,
    }) = config.pool_info
    {
        let user_lp_shares = user_info
            .lp_shares
            .ok_or_else(|| StdError::generic_err("Lp share should be calculated"))?;

        let astroport_lp_amount = user_lp_shares - user_info.claimed_lp_shares;

        if state.is_lp_staked {
            let generator = config
                .generator_contract
                .ok_or_else(|| StdError::generic_err("Generator should be set!"))?;

            let rwi: RewardInfoResponse = deps.querier.query_wasm_smart(
                &generator,
                &GenQueryMsg::RewardInfo {
                    lp_token: astro_ust_lp_token_address.to_string(),
                },
            )?;

            // Calculate ASTRO staking reward receivable by the user
            let pending_astro_rewards = (state.generator_astro_per_share * astroport_lp_amount)
                - (user_info.user_gen_astro_per_share * astroport_lp_amount);
            user_info.user_gen_astro_per_share = state.generator_astro_per_share;
            user_info.generator_astro_debt += pending_astro_rewards;

            // If no rewards / LP tokens to be claimed
            if pending_astro_rewards == Uint128::zero()
                && user_info.astro_incentive_transferred
                && withdraw_lp_shares.is_none()
            {
                return Err(StdError::generic_err("Nothing to claim!"));
            }

            // COSMOS MSG ::: CLAIM Pending Generator ASTRO Rewards
            cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: rwi.base_reward_token.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: user_address.to_string(),
                    amount: pending_astro_rewards,
                })?,
            }));
            attributes.push(attr("generator_astro_reward", pending_astro_rewards));

            //  COSMOS MSG :: If LP Tokens are staked, we unstake the amount which needs to be returned to the user
            if let Some(withdrawn_lp_shares) = withdraw_lp_shares {
                cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: generator.to_string(),
                    funds: vec![],
                    msg: to_binary(&GenExecuteMsg::Withdraw {
                        lp_token: astro_ust_lp_token_address.to_string(),
                        amount: withdrawn_lp_shares,
                    })?,
                }));
            }
        }

        // Transfer ASTRO incentives when they have been calculated
        if user_info.auction_incentive_amount.is_some() && !user_info.astro_incentive_transferred {
            cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: config.astro_token_address.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: user_address.to_string(),
                    amount: user_info.auction_incentive_amount.unwrap(),
                })?,
            }));
            user_info.astro_incentive_transferred = true;
            attributes.push(attr(
                "auction_astro_reward",
                user_info.auction_incentive_amount.unwrap(),
            ));
        }

        if let Some(withdrawn_lp_shares) = withdraw_lp_shares {
            cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: astro_ust_lp_token_address.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: user_address.to_string(),
                    amount: withdrawn_lp_shares,
                })?,
                funds: vec![],
            }));
            attributes.push(attr("lp_withdrawn", withdrawn_lp_shares));
            user_info.claimed_lp_shares += withdrawn_lp_shares;
        }
        USERS.save(deps.storage, &user_address, &user_info)?;
    } else {
        return Err(StdError::generic_err("Pool info isn't set yet!"));
    }

    Ok(Response::new()
        .add_messages(cosmos_msgs)
        .add_attributes(attributes))
}

/// Updates state after liquidity is added to the ASTRO-UST Pool
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **prev_lp_balance** is an object of type [`Uint128`].
pub fn update_state_on_liquidity_addition_to_pool(
    deps: DepsMut,
    env: Env,
    prev_lp_balance: Uint128,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    if let Some(PoolInfo {
        astro_ust_pool_address: _,
        astro_ust_lp_token_address,
    }) = config.pool_info
    {
        // QUERY CURRENT LP TOKEN BALANCE :: NEWLY MINTED LP TOKENS
        let cur_lp_balance = cw20_get_balance(
            &deps.querier,
            astro_ust_lp_token_address,
            env.contract.address,
        )?;
        // STATE :: UPDATE --> SAVE
        state.lp_shares_minted = Some(cur_lp_balance - prev_lp_balance);
        state.pool_init_timestamp = env.block.time.seconds();
        STATE.save(deps.storage, &state)?;

        // Activate lockdrop and airdrop claims
        let cosmos_msgs = vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.airdrop_contract_address.to_string(),
            msg: to_binary(&AirdropEnableClaims {})?,
            funds: vec![],
        })];

        Ok(Response::new()
            .add_messages(cosmos_msgs)
            .add_attributes(vec![
                ("action", "update_state_on_liquidity_addition_to_pool"),
                ("lp_shares_minted", &cur_lp_balance.to_string()),
                (
                    "pool_init_timestamp",
                    &state.pool_init_timestamp.to_string(),
                ),
            ]))
    } else {
        Err(StdError::generic_err("Pool info isn't set yet!"))
    }
}

/// Updates state after ASTRO rewards are claimed from the astroport generator
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **prev_astro_balance** is an object of type [`Uint128`]. Number of ASTRO tokens available with the contract before the claim
pub fn update_state_on_reward_claim(
    deps: DepsMut,
    env: Env,
    prev_astro_balance: Uint128,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    let generator = config
        .generator_contract
        .ok_or_else(|| StdError::generic_err("Generator should be set!"))?;

    if let Some(PoolInfo {
        astro_ust_pool_address: _,
        astro_ust_lp_token_address,
    }) = config.pool_info
    {
        let rwi: RewardInfoResponse = deps.querier.query_wasm_smart(
            &generator,
            &GenQueryMsg::RewardInfo {
                lp_token: astro_ust_lp_token_address.to_string(),
            },
        )?;

        let lp_balance: Uint128 = deps.querier.query_wasm_smart(
            &generator,
            &GenQueryMsg::Deposit {
                lp_token: astro_ust_lp_token_address.to_string(),
                user: env.contract.address.to_string(),
            },
        )?;

        let base_reward_received;
        state.generator_astro_per_share += {
            let res: BalanceResponse = deps.querier.query_wasm_smart(
                rwi.base_reward_token.to_string(),
                &Cw20QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                },
            )?;
            base_reward_received = res.balance - prev_astro_balance;
            Decimal::from_ratio(base_reward_received, lp_balance)
        };

        // SAVE UPDATED STATE OF THE POOL
        STATE.save(deps.storage, &state)?;

        Ok(Response::new()
            .add_attribute("astro_reward_received", base_reward_received)
            .add_attribute(
                "generator_astro_per_share",
                state.generator_astro_per_share.to_string(),
            ))
    } else {
        Err(StdError::generic_err("Pool info isn't set yet!"))
    }
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

/// Returns LP Balance  that a user can withdraw based on a vesting schedule
/// ## Params
/// * **cur_timestamp** is an object of type [`u64`].
///
/// * **config** is an object of type [`Config`].
///
/// * **state** is an object of type [`State`].
///
/// * **user_info** is an object of type [`UserInfo`].
pub fn calculate_withdrawable_lp_shares(
    cur_timestamp: u64,
    config: &Config,
    state: &State,
    user_info: &UserInfo,
) -> StdResult<Option<Uint128>> {
    if let Some(user_lp_shares) = user_info.lp_shares {
        let time_elapsed = cur_timestamp - state.pool_init_timestamp;
        if time_elapsed >= config.lp_tokens_vesting_duration {
            return Ok(Some(user_lp_shares - user_info.claimed_lp_shares));
        }

        let withdrawable_lp_balance =
            user_lp_shares * Decimal::from_ratio(time_elapsed, config.lp_tokens_vesting_duration);
        Ok(Some(withdrawable_lp_balance - user_info.claimed_lp_shares))
    } else {
        Ok(None)
    }
}

/// Returns User's Info
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **user_info** is an object of type [`UserInfo`].
fn query_user_info(deps: Deps, env: Env, user_address: String) -> StdResult<UserInfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;
    let user_address = deps.api.addr_validate(&user_address)?;
    let mut user_info = USERS
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    // User Info Response
    let mut user_info_response = UserInfoResponse {
        astro_delegated: user_info.astro_delegated,
        ust_delegated: user_info.ust_delegated,
        ust_withdrawn: user_info.ust_withdrawn,
        lp_shares: user_info.lp_shares,
        claimed_lp_shares: user_info.claimed_lp_shares,
        withdrawable_lp_shares: None,
        auction_incentive_amount: user_info.auction_incentive_amount,
        astro_incentive_transferred: user_info.astro_incentive_transferred,
        generator_astro_debt: user_info.generator_astro_debt,
        claimable_generator_astro: Uint128::zero(),
        user_gen_astro_per_share: user_info.user_gen_astro_per_share,
    };

    // If ASTRO - UST Pool info is present
    if let Some(PoolInfo {
        astro_ust_pool_address: _,
        astro_ust_lp_token_address,
    }) = &config.pool_info
    {
        // If ASTRO - UST LP Tokens have been minted
        if let Some(lp_balance) = state.lp_shares_minted {
            // Calculate user's LP shares & ASTRO incentives (if possible)
            if user_info.lp_shares.is_none() {
                update_user_lp_shares(&state, lp_balance, &mut user_info)?;
                user_info_response.lp_shares = user_info.lp_shares;
            }
            // If user's ASTRO incentives are not set, but the total ASTRO incentives have been set
            if config.astro_incentive_amount.is_some()
                && user_info.auction_incentive_amount.is_none()
            {
                update_user_astro_incentives(
                    config.astro_incentive_amount,
                    user_info.lp_shares,
                    lp_balance,
                    &mut user_info,
                )?;
                user_info_response.auction_incentive_amount = user_info.auction_incentive_amount;
            }
            let astroport_lp_amount = user_info.lp_shares.unwrap() - user_info.claimed_lp_shares;
            // If LP tokens are staked and user has a > 0 LP share balance, we calculate user's claimable ASTRO staking rewards
            if state.is_lp_staked && !astroport_lp_amount.is_zero() {
                let generator = config
                    .generator_contract
                    .clone()
                    .ok_or_else(|| StdError::generic_err("Generator should be set!"))?;
                // Auction contract's staked LP balance
                let lp_balance: Uint128 = deps.querier.query_wasm_smart(
                    &generator,
                    &GenQueryMsg::Deposit {
                        lp_token: astro_ust_lp_token_address.to_string(),
                        user: env.contract.address.to_string(),
                    },
                )?;

                // QUERY :: Check if there are any pending staking rewards
                let pending_rewards: PendingTokenResponse = deps.querier.query_wasm_smart(
                    &generator,
                    &GenQueryMsg::PendingToken {
                        lp_token: astro_ust_lp_token_address.to_string(),
                        user: env.contract.address.to_string(),
                    },
                )?;

                state.generator_astro_per_share +=
                    Decimal::from_ratio(pending_rewards.pending, lp_balance);

                // Calculated claimable ASTRO staking rewards
                user_info_response.claimable_generator_astro = (state.generator_astro_per_share
                    * astroport_lp_amount)
                    - (user_info.user_gen_astro_per_share * astroport_lp_amount);
            }

            // Updated withdrawable LP shares balance
            user_info_response.withdrawable_lp_shares = calculate_withdrawable_lp_shares(
                env.block.time.seconds(),
                &config,
                &state,
                &user_info,
            )?;
        }
    }

    if user_info_response.auction_incentive_amount.is_none() {
        user_info_response.auction_incentive_amount =
            calculate_auction_reward_for_user(&state, &user_info, config.astro_incentive_amount);
    }

    Ok(user_info_response)
}
/// Calculates ASTRO tokens receivable by a user for participating (providing UST & ASTRO) in the bootstraping phase of the ASTRO-UST Pool
/// ## Params
/// * **state** is an object of type [`State`].
///
/// * **user_info** is an object of type [`UserInfo`].
///
/// * **total_astro_rewards** is an optional object of type [`Uint128`].
fn calculate_auction_reward_for_user(
    state: &State,
    user_info: &UserInfo,
    total_astro_rewards: Option<Uint128>,
) -> Option<Uint128> {
    if !user_info.astro_delegated.is_zero() || !user_info.ust_delegated.is_zero() {
        if let Some(total_astro_rewards) = total_astro_rewards {
            let mut user_astro_incentives = Uint128::zero();

            // ASTRO incentives from ASTRO delegated
            if state.total_astro_delegated > Uint128::zero() {
                let astro_incentives_from_astro = Decimal::from_ratio(
                    user_info.astro_delegated,
                    state.total_astro_delegated * Uint128::new(2),
                ) * total_astro_rewards;
                user_astro_incentives += astro_incentives_from_astro;
            }

            // ASTRO incentives from UST delegated
            if state.total_ust_delegated > Uint128::zero() {
                let astro_incentives_from_ust = Decimal::from_ratio(
                    user_info.ust_delegated,
                    state.total_ust_delegated * Uint128::new(2),
                ) * total_astro_rewards;
                user_astro_incentives += astro_incentives_from_ust;
            }
            return Some(user_astro_incentives);
        }
    }

    None
}
