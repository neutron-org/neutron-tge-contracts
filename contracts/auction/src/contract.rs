#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Addr, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};

use astroport_periphery::airdrop::ExecuteMsg::EnableClaims as AirdropEnableClaims;
use astroport_periphery::auction::{
    CallbackMsg, Config, ExecuteMsg, InstantiateMsg, MigrateMsg, PoolInfo, QueryMsg, State,
    UpdateConfigMsg, UserInfo, UserInfoResponse,
};
use astroport_periphery::helpers::{build_approve_cntrn_msg, cntrn_get_balance};
use astroport_periphery::lockdrop::ExecuteMsg::EnableClaims as LockdropEnableClaims;

use crate::state::{CONFIG, STATE, USERS};
use astroport::asset::{addr_validate_to_lower, Asset, AssetInfo, PairInfo};
use astroport::generator::{
    ExecuteMsg as GenExecuteMsg, PendingTokenResponse, QueryMsg as GenQueryMsg, RewardInfoResponse,
};
use astroport::pair::QueryMsg as AstroportPairQueryMsg;
use astroport::querier::query_token_balance;
use cw2::set_contract_version;
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "auction";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in the [`Instantiateamount: amount_opposite
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
            .map(|v| addr_validate_to_lower(deps.api, &v))
            .transpose()?
            .unwrap_or(info.sender),
        airdrop_contract_address: addr_validate_to_lower(deps.api, &msg.airdrop_contract_address)?,
        lockdrop_contract_address: addr_validate_to_lower(
            deps.api,
            &msg.lockdrop_contract_address,
        )?,
        pool_info: None,
        generator_contract: None,
        lp_tokens_vesting_duration: msg.lp_tokens_vesting_duration,
        init_timestamp: msg.init_timestamp,
        deposit_window: msg.deposit_window,
        withdrawal_window: msg.withdrawal_window,
        cntrn_token_address: addr_validate_to_lower(deps.api, &msg.cntrn_token_contract)?,
        native_denom: msg.native_denom,
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
/// * **ExecuteMsg::Receive(msg)** Parse incoming messages from the cNTRN token.
///
/// * **ExecuteMsg::UpdateConfig { new_config }** Admin function to update configuration parameters.
///
/// * **ExecuteMsg::DepositUst {}** Facilitates NATIVE deposits by users.
///
/// * **ExecuteMsg::WithdrawUst { amount }** Facilitates NATIVE withdrawals by users.
///
/// * **ExecuteMsg::InitPool { slippage }** Admin function which facilitates Liquidity addtion to the Astroport cNTRN-NATIVE Pool.
///
/// * **ExecuteMsg::StakeLpTokens {}** Admin function to stake cNTRN-NATIVE LP tokens with the generator contract.
///
/// * **ExecuteMsg::ClaimRewards { withdraw_lp_shares }** Facilitates cNTRN rewards claim.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, StdError> {
    match msg {
        ExecuteMsg::UpdateConfig { new_config } => execute_update_config(deps, info, new_config),
        ExecuteMsg::Deposit { cntrn_amount } => execute_deposit(deps, env, info, cntrn_amount),
        ExecuteMsg::Withdraw {
            amount_opposite,
            amount_cntrn,
        } => execute_withdraw(deps, env, info, amount_opposite, amount_cntrn),
        ExecuteMsg::InitPool { slippage } => execute_init_pool(deps, env, info, slippage),
        ExecuteMsg::StakeLpTokens {} => execute_stake_lp_tokens(deps, env, info),
        ExecuteMsg::ClaimRewards { withdraw_lp_shares } => {
            execute_claim_rewards_and_withdraw_lp_shares(deps, env, info, withdraw_lp_shares)
        }
        ExecuteMsg::Callback(msg) => execute_callback(deps, env, info, msg),
    }
}

pub fn execute_deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cntrn_amount: Uint128,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;

    // CHECK :: Auction deposit window open
    if !is_deposit_open(env.block.time.seconds(), &config) {
        return Err(StdError::generic_err("Deposit window closed"));
    }

    // CHECK ::: Amount needs to be valid
    if cntrn_amount.is_zero() {
        return Err(StdError::generic_err(format!(
            "{} amount must be greater than 0",
            config.cntrn_token_address
        )));
    }

    let mut state = STATE.load(deps.storage)?;
    let mut user_info = USERS
        .may_load(deps.storage, &info.sender)?
        .unwrap_or_default();

    // Retrieve native sent by the user
    if info.funds.len() != 1 || info.funds[0].denom != config.native_denom {
        return Err(StdError::generic_err(format!(
            "You may delegate {} native coin only",
            config.native_denom
        )));
    }

    let fund = &info.funds[0];

    // CHECK ::: Amount needs to be valid
    if fund.amount.is_zero() {
        return Err(StdError::generic_err("Amount must be greater than 0"));
    }

    // UPDATE STATE
    state.total_opposite_deposited += fund.amount;
    user_info.opposite_delegated += fund.amount;

    // SEND cNTRN TOKENS TO THE CONTRACT
    // A user must give the contract permission to transfer the tokens before it
    let cntrn_msg = Asset {
        info: AssetInfo::Token {
            contract_addr: config.cntrn_token_address,
        },
        amount: cntrn_amount,
    }
    .into_msg(&deps.querier, &env.contract.address)?;

    state.total_cntrn_deposited += cntrn_amount;
    user_info.cntrn_delegated += cntrn_amount;

    // SAVE UPDATED STATE
    STATE.save(deps.storage, &state)?;
    USERS.save(deps.storage, &info.sender, &user_info)?;

    Ok(Response::new().add_message(cntrn_msg).add_attributes(vec![
        attr("action", "Auction::ExecuteMsg::Deposit"),
        attr("user", info.sender.to_string()),
        attr("native_delegated", fund.amount),
        attr("cntrn_delegated", cntrn_amount),
    ]))
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
fn execute_callback(
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
        CallbackMsg::UpdateStateOnRewardClaim { prev_cntrn_balance } => {
            update_state_on_reward_claim(deps, env, prev_cntrn_balance)
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
pub fn execute_update_config(
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
        config.owner = addr_validate_to_lower(deps.api, &owner)?;
        attributes.push(attr("owner", config.owner.to_string()));
    }

    if let Some(cntrn_native_pair_address) = new_config.cntrn_native_pair_address {
        if state.lp_shares_minted.is_some() {
            return Err(StdError::generic_err(
                "Assets had already been provided to previous pool!",
            ));
        }
        let cntrn_native_pair_addr = addr_validate_to_lower(deps.api, &cntrn_native_pair_address)?;

        let pair_info: PairInfo = deps
            .querier
            .query_wasm_smart(cntrn_native_pair_address, &AstroportPairQueryMsg::Pair {})?;

        config.pool_info = Some(PoolInfo {
            cntrn_native_pool_address: cntrn_native_pair_addr,
            cntrn_native_lp_token_address: pair_info.liquidity_token,
        })
    }

    if let Some(generator_contract) = new_config.generator_contract {
        // check if the LP tokens are already staked or not
        if state.is_lp_staked {
            return Err(StdError::generic_err(
                "cNTRN-NATIVE LP tokens already staked",
            ));
        }

        let generator_addr = addr_validate_to_lower(deps.api, &generator_contract)?;
        config.generator_contract = Some(generator_addr.clone());
        attributes.push(attr("generator", generator_addr.to_string()));
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
    amount_native: Uint128,
    amount_cntrn: Uint128,
) -> Result<Response, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;
    let user_address = info.sender;
    let mut user_info = USERS.load(deps.storage, &user_address)?;

    // CHECK :: Has the user already withdrawn during the current window
    if user_info.opposite_withdrawn {
        return Err(StdError::generic_err("Max 1 withdrawal allowed"));
    }

    // Check :: Amount should be within the allowed withdrawal limit bounds
    let max_withdrawal_percent = allowed_withdrawal_percent(env.block.time.seconds(), &config);
    let max_allowed_native = user_info.opposite_delegated * max_withdrawal_percent;
    let max_allowed_cntrn = user_info.cntrn_delegated * max_withdrawal_percent;

    if amount_native > max_allowed_native || amount_cntrn > max_allowed_cntrn {
        return Err(StdError::generic_err(format!(
            "Amount exceeds maximum allowed withdrawal limit of {}",
            max_withdrawal_percent
        )));
    }
    if amount_native.gt(&Uint128::zero()) && amount_native.gt(&Uint128::zero()) {
        return Err(StdError::generic_err(
            "At least one token must be withdrawn",
        ));
    }

    // After deposit window is closed, we allow to withdraw only once
    if env.block.time.seconds() >= config.init_timestamp + config.deposit_window {
        user_info.opposite_withdrawn = true;
    }

    let mut res = Response::new();

    if amount_native.gt(&Uint128::zero()) {
        // Transfer Native tokens to the user
        let transfer_msg = CosmosMsg::Bank(BankMsg::Send {
            to_address: user_address.to_string(),
            amount: vec![Coin {
                denom: config.native_denom,
                amount: amount_native,
            }],
        });
        res = res.add_message(transfer_msg);
    }

    if amount_native.gt(&Uint128::zero()) {
        // Transfer cNTRN tokens to the user
        let cntrn_msg = Asset {
            info: AssetInfo::Token {
                contract_addr: config.cntrn_token_address,
            },
            amount: amount_cntrn,
        }
        .into_msg(&deps.querier, user_address.clone())?;
        res = res.add_message(cntrn_msg);
    }

    // UPDATE STATE
    state.total_opposite_deposited -= amount_native;
    user_info.opposite_delegated -= amount_native;

    // SAVE UPDATED STATE
    STATE.save(deps.storage, &state)?;
    USERS.save(deps.storage, &user_address, &user_info)?;

    Ok(res.add_attributes(vec![
        attr("action", "Auction::ExecuteMsg::Withdraw"),
        attr("user", user_address.to_string()),
        attr("opposite_withdrawn", amount_native),
        attr("cntrn_withdrawn", amount_cntrn),
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

/// Facilitates Liquidity addtion to the Astroport cNTRN-NATIVE Pool. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **slippage** is an optional object of type [`Decimal`].
pub fn execute_init_pool(
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
        cntrn_native_pool_address,
        cntrn_native_lp_token_address,
    }) = config.pool_info
    {
        let native_coin = deps
            .querier
            .query_balance(&env.contract.address, config.native_denom)?;

        // QUERY CURRENT LP TOKEN BALANCE (FOR SAFETY - IN ANY CASE)
        let cur_lp_balance = query_token_balance(
            &deps.querier,
            cntrn_native_lp_token_address,
            env.contract.address.clone(),
        )?;

        // COSMOS MSGS
        // :: 1.  APPROVE cNTRN WITH LP POOL ADDRESS AS BENEFICIARY
        // :: 2.  ADD LIQUIDITY
        // :: 3. CallbackMsg :: Update state on liquidity addition to LP Pool
        msgs.push(build_approve_cntrn_msg(
            config.cntrn_token_address.to_string(),
            cntrn_native_pool_address.to_string(),
            state.total_cntrn_deposited,
            env.block.height + 1u64,
        )?);

        msgs.push(build_provide_liquidity_to_lp_pool_msg(
            deps.as_ref(),
            config.cntrn_token_address,
            cntrn_native_pool_address,
            native_coin.amount,
            state.total_cntrn_deposited,
            slippage,
        )?);

        msgs.push(
            CallbackMsg::UpdateStateOnLiquidityAdditionToPool {
                prev_lp_balance: cur_lp_balance,
            }
            .to_cosmos_msg(&env)?,
        );
        Ok(Response::new().add_messages(msgs).add_attributes(vec![
            attr("action", "Auction::ExecuteMsg::InitPool"),
            attr("cntrn_provided", state.total_cntrn_deposited),
            attr("native_provided", native_coin.amount),
        ]))
    } else {
        Err(StdError::generic_err("Pool info isn't set yet!"))
    }
}

/// Builds provide liquidity to pool message.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **cntrn_token_address** is an object of type [`Addr`].
///
/// * **cntrn_native_pool_address** is an object of type [`Addr`].
///
/// * **native_amount** is an object of type [`Uint128`].
///
/// * **cntrn_amount** is an object of type [`Uint128`].
///
/// * **slippage_tolerance** is an optional object of type [`Decimal`].
fn build_provide_liquidity_to_lp_pool_msg(
    deps: Deps,
    cntrn_token_address: Addr,
    cntrn_native_pool_address: Addr,
    native_amount: Uint128,
    cntrn_amount: Uint128,
    slippage_tolerance: Option<Decimal>,
) -> StdResult<CosmosMsg> {
    let config = CONFIG.load(deps.storage)?;
    let cntrn = Asset {
        amount: cntrn_amount,
        info: AssetInfo::Token {
            contract_addr: cntrn_token_address,
        },
    };
    let native_denom = config.native_denom;

    let native = Asset {
        amount: native_amount,
        info: AssetInfo::NativeToken {
            denom: native_denom.clone(),
        },
    };

    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cntrn_native_pool_address.to_string(),
        funds: vec![Coin {
            denom: native_denom,
            amount: native.amount,
        }],
        msg: to_binary(&astroport::pair::ExecuteMsg::ProvideLiquidity {
            assets: [native, cntrn],
            slippage_tolerance,
            auto_stake: None,
            receiver: None,
        })?,
    }))
}

/// Stakes CW20-NATIVE LP tokens with the generator contract.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
pub fn execute_stake_lp_tokens(
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
        .ok_or_else(|| StdError::generic_err("Should be provided to the cNTRN/NATIVE pool!"))?;

    if let Some(PoolInfo {
        cntrn_native_lp_token_address,
        cntrn_native_pool_address: _,
    }) = config.pool_info
    {
        // QUERY CURRENT LP TOKEN BALANCE (FOR SAFETY - IN ANY CASE)
        let cur_lp_balance = query_token_balance(
            &deps.querier,
            cntrn_native_lp_token_address.clone(),
            env.contract.address.clone(),
        )?;

        // Init response
        let mut response = Response::new()
            .add_attribute("action", "Auction::ExecuteMsg::StakeLPTokens")
            .add_attribute("staked_amount", lp_shares_minted);

        // COSMOS MSGs
        // :: Add increase allowance msg so generator contract can transfer tokens to itself
        // :: To stake LP Tokens to the Neutron generator contract
        response.messages.push(SubMsg::new(build_approve_cntrn_msg(
            cntrn_native_lp_token_address.to_string(),
            generator.to_string(),
            cur_lp_balance,
            env.block.height + 1u64,
        )?));
        response.messages.push(SubMsg::new(WasmMsg::Execute {
            contract_addr: cntrn_native_lp_token_address.to_string(),
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

/// Facilitates CW20 Reward claim for users.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **withdraw_lp_shares** is an optional object of type [`Uint128`].
pub fn execute_claim_rewards_and_withdraw_lp_shares(
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
    if user_info.cntrn_delegated.is_zero() && user_info.opposite_delegated.is_zero() {
        return Err(StdError::generic_err("No delegated assets"));
    }

    let mut cosmos_msgs = vec![];

    if let Some(lp_balance) = state.lp_shares_minted {
        // Calculate user's LP shares & cNTRN incentives (if possible)
        if user_info.lp_shares.is_none() {
            update_user_lp_shares(&state, lp_balance, &mut user_info)?;

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
                cntrn_native_pool_address: _,
                cntrn_native_lp_token_address,
            }) = config.pool_info
            {
                // QUERY :: Check if there are any pending staking rewards
                let pending_rewards: PendingTokenResponse = deps.querier.query_wasm_smart(
                    &generator,
                    &GenQueryMsg::PendingToken {
                        lp_token: cntrn_native_lp_token_address.to_string(),
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
                            lp_token: cntrn_native_lp_token_address.to_string(),
                        },
                    )?;

                    let cntrn_balance = {
                        let res: BalanceResponse = deps.querier.query_wasm_smart(
                            rwi.base_reward_token,
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
                            lp_token: cntrn_native_lp_token_address.to_string(),
                            amount: Uint128::zero(),
                        })?,
                    }));

                    cosmos_msgs.push(
                        CallbackMsg::UpdateStateOnRewardClaim {
                            prev_cntrn_balance: cntrn_balance,
                        }
                        .to_cosmos_msg(&env)?,
                    );
                }
            } else {
                return Err(StdError::generic_err("Pool info isn't set yet!"));
            }
        }
        // If no rewards to claim and no LP tokens to be withdrawn.
        else if user_info.cntrn_incentive_transferred && withdraw_lp_shares.is_none() {
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

/// Calculates user's cNTRN - NATIVE LP shares based on amount delegated.
/// User LP shares (cNTRN delegation share) = (1/2) *  (cNTRN delegated / total cNTRN delegated)
/// User LP shares (NATIVE deposit share) = (1/2) *  (NATIVE deposited / total NATIVE deposited)
/// User's total LP shares  = User's cNTRN delegation LP share + User's NATIVE deposit LP share
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
        user_info.cntrn_delegated,
        state.total_cntrn_deposited * Uint128::new(2),
    ) + Decimal::from_ratio(
        user_info.opposite_delegated,
        state.total_opposite_deposited * Uint128::new(2),
    )) * lp_balance;
    user_info.lp_shares = Some(user_lp_share);

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
        cntrn_native_pool_address: _,
        cntrn_native_lp_token_address,
    }) = config.pool_info
    {
        let user_lp_shares = user_info
            .lp_shares
            .ok_or_else(|| StdError::generic_err("Lp share should be calculated"))?;

        let neutron_lp_amount = user_lp_shares - user_info.claimed_lp_shares;

        if state.is_lp_staked {
            let generator = config
                .generator_contract
                .ok_or_else(|| StdError::generic_err("Generator should be set!"))?;

            let rwi: RewardInfoResponse = deps.querier.query_wasm_smart(
                &generator,
                &GenQueryMsg::RewardInfo {
                    lp_token: cntrn_native_lp_token_address.to_string(),
                },
            )?;

            // Calculate cNTRN staking reward receivable by the user
            let pending_cntrn_rewards = (state.generator_cntrn_per_share * neutron_lp_amount)
                - (user_info.user_gen_cntrn_per_share * neutron_lp_amount);
            user_info.user_gen_cntrn_per_share = state.generator_cntrn_per_share;
            user_info.generator_cntrn_debt += pending_cntrn_rewards;

            // If no rewards / LP tokens to be claimed
            if pending_cntrn_rewards == Uint128::zero()
                && user_info.cntrn_incentive_transferred
                && withdraw_lp_shares.is_none()
            {
                return Err(StdError::generic_err("Nothing to claim!"));
            }

            // COSMOS MSG ::: CLAIM Pending Generator cNTRN Rewards
            cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: rwi.base_reward_token.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: user_address.to_string(),
                    amount: pending_cntrn_rewards,
                })?,
            }));
            attributes.push(attr("generator_cntrn_reward", pending_cntrn_rewards));

            //  COSMOS MSG :: If LP Tokens are staked, we unstake the amount which needs to be returned to the user
            if let Some(withdrawn_lp_shares) = withdraw_lp_shares {
                cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: generator.to_string(),
                    funds: vec![],
                    msg: to_binary(&GenExecuteMsg::Withdraw {
                        lp_token: cntrn_native_lp_token_address.to_string(),
                        amount: withdrawn_lp_shares,
                    })?,
                }));
            }
        }

        // Transfer cNTRN incentives when they have been calculated
        if user_info.auction_incentive_amount.is_some() && !user_info.cntrn_incentive_transferred {
            cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: config.cntrn_token_address.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: user_address.to_string(),
                    amount: user_info.auction_incentive_amount.unwrap(),
                })?,
            }));
            user_info.cntrn_incentive_transferred = true;
            attributes.push(attr(
                "auction_cntrn_reward",
                user_info.auction_incentive_amount.unwrap(),
            ));
        }

        if let Some(withdrawn_lp_shares) = withdraw_lp_shares {
            cosmos_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: cntrn_native_lp_token_address.to_string(),
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

/// Updates state after liquidity is added to the cNTRN-NATIVE Pool
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
        cntrn_native_pool_address: _,
        cntrn_native_lp_token_address,
    }) = config.pool_info
    {
        // QUERY CURRENT LP TOKEN BALANCE :: NEWLY MINTED LP TOKENS
        let cur_lp_balance = cntrn_get_balance(
            &deps.querier,
            cntrn_native_lp_token_address,
            env.contract.address,
        )?;
        // STATE :: UPDATE --> SAVE
        state.lp_shares_minted = Some(cur_lp_balance - prev_lp_balance);
        state.pool_init_timestamp = env.block.time.seconds();
        STATE.save(deps.storage, &state)?;

        // Activate lockdrop and airdrop claims
        let cosmos_msgs = vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: config.lockdrop_contract_address.to_string(),
                msg: to_binary(&LockdropEnableClaims {})?,
                funds: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: config.airdrop_contract_address.to_string(),
                msg: to_binary(&AirdropEnableClaims {})?,
                funds: vec![],
            }),
        ];

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

/// Updates state after cNTRN rewards are claimed from the neutron generator
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **prev_cntrn_balance** is an object of type [`Uint128`]. Number of cNTRN tokens available with the contract before the claim
pub fn update_state_on_reward_claim(
    deps: DepsMut,
    env: Env,
    prev_cntrn_balance: Uint128,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    let generator = config
        .generator_contract
        .ok_or_else(|| StdError::generic_err("Generator should be set!"))?;

    if let Some(PoolInfo {
        cntrn_native_pool_address: _,
        cntrn_native_lp_token_address,
    }) = config.pool_info
    {
        let rwi: RewardInfoResponse = deps.querier.query_wasm_smart(
            &generator,
            &GenQueryMsg::RewardInfo {
                lp_token: cntrn_native_lp_token_address.to_string(),
            },
        )?;

        let lp_balance: Uint128 = deps.querier.query_wasm_smart(
            &generator,
            &GenQueryMsg::Deposit {
                lp_token: cntrn_native_lp_token_address.to_string(),
                user: env.contract.address.to_string(),
            },
        )?;

        let base_reward_received;
        state.generator_cntrn_per_share += {
            let res: BalanceResponse = deps.querier.query_wasm_smart(
                rwi.base_reward_token,
                &Cw20QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                },
            )?;
            base_reward_received = res.balance - prev_cntrn_balance;
            Decimal::from_ratio(base_reward_received, lp_balance)
        };

        // SAVE UPDATED STATE OF THE POOL
        STATE.save(deps.storage, &state)?;

        Ok(Response::new()
            .add_attribute("cntrn_reward_received", base_reward_received)
            .add_attribute(
                "generator_cntrn_per_share",
                state.generator_cntrn_per_share.to_string(),
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
    let user_address = addr_validate_to_lower(deps.api, &user_address)?;
    let mut user_info = USERS
        .may_load(deps.storage, &user_address)?
        .unwrap_or_default();

    // User Info Response
    let mut user_info_response = UserInfoResponse {
        cntrn_delegated: user_info.cntrn_delegated,
        native_delegated: user_info.opposite_delegated,
        native_withdrawn: user_info.opposite_withdrawn,
        lp_shares: user_info.lp_shares,
        claimed_lp_shares: user_info.claimed_lp_shares,
        withdrawable_lp_shares: None,
        generator_cntrn_debt: user_info.generator_cntrn_debt,
        claimable_generator_cntrn: Uint128::zero(),
        user_gen_cntrn_per_share: user_info.user_gen_cntrn_per_share,
    };

    // If cNTRN - NATIVE Pool info is present
    if let Some(PoolInfo {
        cntrn_native_pool_address: _,
        cntrn_native_lp_token_address,
    }) = &config.pool_info
    {
        // If cNTRN - NATIVE LP Tokens have been minted
        if let Some(lp_balance) = state.lp_shares_minted {
            // Calculate user's LP shares & cNTRN incentives (if possible)
            if user_info.lp_shares.is_none() {
                update_user_lp_shares(&state, lp_balance, &mut user_info)?;
                user_info_response.lp_shares = user_info.lp_shares;
            }
            let neutron_lp_amount = user_info.lp_shares.unwrap() - user_info.claimed_lp_shares;
            // If LP tokens are staked and user has a > 0 LP share balance, we calculate user's claimable cNTRN staking rewards
            if state.is_lp_staked && !neutron_lp_amount.is_zero() {
                let generator = config
                    .generator_contract
                    .clone()
                    .ok_or_else(|| StdError::generic_err("Generator should be set!"))?;
                // Auction contract's staked LP balance
                let lp_balance: Uint128 = deps.querier.query_wasm_smart(
                    &generator,
                    &GenQueryMsg::Deposit {
                        lp_token: cntrn_native_lp_token_address.to_string(),
                        user: env.contract.address.to_string(),
                    },
                )?;

                // QUERY :: Check if there are any pending staking rewards
                let pending_rewards: PendingTokenResponse = deps.querier.query_wasm_smart(
                    &generator,
                    &GenQueryMsg::PendingToken {
                        lp_token: cntrn_native_lp_token_address.to_string(),
                        user: env.contract.address.to_string(),
                    },
                )?;

                state.generator_cntrn_per_share +=
                    Decimal::from_ratio(pending_rewards.pending, lp_balance);

                // Calculated claimable cNTRN staking rewards
                user_info_response.claimable_generator_cntrn = (state.generator_cntrn_per_share
                    * neutron_lp_amount)
                    - (user_info.user_gen_cntrn_per_share * neutron_lp_amount);
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

    Ok(user_info_response)
}
