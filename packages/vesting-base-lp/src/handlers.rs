use std::borrow::Borrow;
use std::ops::Mul;
use crate::error::ContractError;
use crate::ext_historical::{handle_execute_historical_msg, handle_query_historical_msg};
use crate::ext_managed::{handle_execute_managed_msg, handle_query_managed_msg};
use crate::ext_with_managers::{handle_execute_with_managers_msg, handle_query_managers_msg};
use crate::msg::{CallbackMsg, Cw20HookMsg, ExecuteMsg, MigrateMsg, QueryMsg};
use crate::state::{read_vesting_infos, vesting_info, vesting_state, XYK_TO_CL_MIGRATION_CONFIG, XykToClMigrationConfig};
use crate::state::{CONFIG, OWNERSHIP_PROPOSAL, VESTING_MANAGERS};
use crate::types::{
    Config, OrderBy, VestingAccount, VestingAccountResponse, VestingAccountsResponse, VestingInfo,
    VestingSchedule, VestingState,
};
use astroport::asset::{addr_opt_validate, token_asset_info, AssetInfo, AssetInfoExt, PairInfo, native_asset};
use astroport::pair::{
    Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg, QueryMsg as PairQueryMsg,
};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use cosmwasm_std::{attr, from_binary, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Storage, SubMsg, Uint128, Decimal, CosmosMsg, WasmMsg, Coin};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg};
use cw_utils::must_pay;
use astroport::cosmwasm_ext::IntegerToDecimal;
use astroport::generator::RewardInfoResponse;

/// Exposes execute functions available in the contract.
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Claim { recipient, amount } => claim(deps, env, info, recipient, amount),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::RegisterVestingAccounts { vesting_accounts } => {
            let config = CONFIG.load(deps.storage)?;
            let vesting_token = get_vesting_token(&config)?;

            match &vesting_token {
                AssetInfo::NativeToken { denom }
                if is_sender_whitelisted(deps.storage, &config, &info.sender) =>
                    {
                        let amount = must_pay(&info, denom)?;
                        register_vesting_accounts(deps, vesting_accounts, amount, env.block.height)
                    }
                _ => Err(ContractError::Unauthorized {}),
            }
        }
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config: Config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                &OWNERSHIP_PROPOSAL,
            )
                .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, &OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, &OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
                .map_err(Into::into)
        }
        ExecuteMsg::SetVestingToken { vesting_token } => {
            set_vesting_token(deps, env, info, vesting_token)
        }
        ExecuteMsg::ManagedExtension { msg } => handle_execute_managed_msg(deps, env, info, msg),
        ExecuteMsg::WithManagersExtension { msg } => {
            handle_execute_with_managers_msg(deps, env, info, msg)
        }
        ExecuteMsg::HistoricalExtension { msg } => {
            handle_execute_historical_msg(deps, env, info, msg)
        }
        ExecuteMsg::MigrateLiquidity {} => execute_migrate_liquidity(deps, env, None, None, None)
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** CW20 message to process.
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let vesting_token = get_vesting_token(&config)?;

    // Permission check
    if !is_sender_whitelisted(
        deps.storage,
        &config,
        &deps.api.addr_validate(&cw20_msg.sender)?,
    ) || token_asset_info(info.sender) != vesting_token
    {
        return Err(ContractError::Unauthorized {});
    }

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::RegisterVestingAccounts { vesting_accounts } => {
            register_vesting_accounts(deps, vesting_accounts, cw20_msg.amount, env.block.height)
        }
    }
}

/// Create new vesting schedules.
///
/// * **vesting_accounts** list of accounts and associated vesting schedules to create.
///
/// * **cw20_amount** sets the amount that confirms the total amount of all accounts to register.
fn register_vesting_accounts(
    deps: DepsMut,
    vesting_accounts: Vec<VestingAccount>,
    amount: Uint128,
    height: u64,
) -> Result<Response, ContractError> {
    let response = Response::new();
    let config = CONFIG.load(deps.storage)?;
    let mut to_deposit = Uint128::zero();

    for mut vesting_account in vesting_accounts {
        let mut released_amount = Uint128::zero();
        let account_address = deps.api.addr_validate(&vesting_account.address)?;

        assert_vesting_schedules(&account_address, &vesting_account.schedules)?;

        for sch in &vesting_account.schedules {
            let amount = if let Some(end_point) = &sch.end_point {
                end_point.amount
            } else {
                sch.start_point.amount
            };
            to_deposit = to_deposit.checked_add(amount)?;
        }

        let vesting_info = vesting_info(config.extensions.historical);
        if let Some(mut old_info) = vesting_info.may_load(deps.storage, account_address.clone())? {
            released_amount = old_info.released_amount;
            vesting_account.schedules.append(&mut old_info.schedules);
        }

        vesting_info.save(
            deps.storage,
            account_address,
            &VestingInfo {
                schedules: vesting_account.schedules,
                released_amount,
            },
            height,
        )?;
    }

    if to_deposit != amount {
        return Err(ContractError::VestingScheduleAmountError {});
    }

    vesting_state(config.extensions.historical).update::<_, ContractError>(
        deps.storage,
        height,
        |s| {
            let mut state = s.unwrap_or_default();
            state.total_granted = state.total_granted.checked_add(to_deposit)?;
            Ok(state)
        },
    )?;

    Ok(response.add_attributes({
        vec![
            attr("action", "register_vesting_accounts"),
            attr("deposited", to_deposit),
        ]
    }))
}

/// Claims vested tokens and transfers them to the vesting recipient.
///
/// * **recipient** vesting recipient for which to claim tokens.
///
/// * **amount** amount of vested tokens to claim.
fn claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: Option<String>,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let vesting_token = get_vesting_token(&config)?;
    let vesting_info = vesting_info(config.extensions.historical);
    let mut sender_vesting_info = vesting_info.load(deps.storage, info.sender.clone())?;

    let available_amount =
        compute_available_amount(env.block.time.seconds(), &sender_vesting_info)?;

    let claim_amount = if let Some(a) = amount {
        if a > available_amount {
            return Err(ContractError::AmountIsNotAvailable {});
        };
        a
    } else {
        available_amount
    };

    let mut response = Response::new();

    if !claim_amount.is_zero() {
        let transfer_msg = vesting_token.with_balance(claim_amount).into_msg(
            &deps.querier,
            recipient.unwrap_or_else(|| info.sender.to_string()),
        )?;
        response = response.add_submessage(SubMsg::new(transfer_msg));

        sender_vesting_info.released_amount = sender_vesting_info
            .released_amount
            .checked_add(claim_amount)?;
        vesting_info.save(
            deps.storage,
            info.sender.clone(),
            &sender_vesting_info,
            env.block.height,
        )?;
        vesting_state(config.extensions.historical).update::<_, ContractError>(
            deps.storage,
            env.block.height,
            |s| {
                let mut state = s.ok_or(ContractError::AmountIsNotAvailable {})?;
                state.total_released = state.total_released.checked_add(claim_amount)?;
                Ok(state)
            },
        )?;
    };

    Ok(response.add_attributes(vec![
        attr("action", "claim"),
        attr("address", &info.sender),
        attr("available_amount", available_amount),
        attr("claimed_amount", claim_amount),
    ]))
}

pub(crate) fn set_vesting_token(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token: AssetInfo,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner && info.sender != config.token_info_manager {
        return Err(ContractError::Unauthorized {});
    }
    token.check(deps.api)?;
    config.vesting_token = Some(token);

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new())
}

pub(crate) fn get_vesting_token(config: &Config) -> Result<AssetInfo, ContractError> {
    config
        .vesting_token
        .clone()
        .ok_or(ContractError::VestingTokenIsNotSet {})
}

fn execute_migrate_liquidity(
    deps: DepsMut,
    env: Env,
    slippage_tolerance: Option<Decimal>,
    ntrn_atom_amount: Option<Uint128>,
    ntrn_usdc_amount: Option<Uint128>,

) -> Result<Response, ContractError> {
    // todo batching
    let vesting_infos = read_vesting_infos(deps.as_ref(), start_after, limit, order_by)?;

    let vesting_accounts: Vec<_> = vesting_infos
        .into_iter()
        .map(|(address, info)| VestingAccountResponse { address, info })
        .collect();

    for user in vesting_accounts {
        let migration_config: XykToClMigrationConfig = XYK_TO_CL_MIGRATION_CONFIG.load(deps.storage)?;

        // get pairs LP token addresses
        let ntrn_atom_pair_info: PairInfo = deps.querier.query_wasm_smart(
            migration_config.ntrn_atom_xyk_pair.clone(),
            &PairQueryMsg::Pair {},
        )?;
        let ntrn_usdc_pair_info: PairInfo = deps.querier.query_wasm_smart(
            migration_config.ntrn_usdc_xyk_pair.clone(),
            &PairQueryMsg::Pair {},
        )?;

        // query max available amounts to be withdrawn from both pairs
        let max_available_ntrn_atom_amount = {
            let resp: BalanceResponse = deps.querier.query_wasm_smart(
                ntrn_atom_pair_info.liquidity_token.clone(),
                &Cw20QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                },
            )?;
            resp.balance
        };
        let max_available_ntrn_usdc_amount = {
            let resp: BalanceResponse = deps.querier.query_wasm_smart(
                ntrn_usdc_pair_info.liquidity_token.clone(),
                &Cw20QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                },
            )?;
            resp.balance
        };
        if max_available_ntrn_atom_amount.is_zero() && max_available_ntrn_usdc_amount.is_zero() {
            return Err(ContractError::MigrationComplete {});
        }

        // validate parameters to the max available values
        if let Some(ntrn_atom_amount) = ntrn_atom_amount {
            if ntrn_atom_amount.gt(&max_available_ntrn_atom_amount) {
                return Err(ContractError::MigrationAmountUnavailable {
                    amount: ntrn_atom_amount,
                    max_amount: max_available_ntrn_atom_amount,
                });
            }
        }
        if let Some(ntrn_usdc_amount) = ntrn_usdc_amount {
            if ntrn_usdc_amount.gt(&max_available_ntrn_usdc_amount) {
                return Err(ContractError::MigrationAmountUnavailable {
                    amount: ntrn_usdc_amount,
                    max_amount: max_available_ntrn_usdc_amount,
                });
            }
        }
        if let Some(slippage_tolerance) = slippage_tolerance {
            if slippage_tolerance.gt(&migration_config.max_slippage) {
                return Err(ContractError::MigrationSlippageToBig {
                    slippage_tolerance,
                    max_slippage_tolerance: migration_config.max_slippage,
                });
            }
        }

        let ntrn_atom_amount = ntrn_atom_amount.unwrap_or(max_available_ntrn_atom_amount);
        let ntrn_usdc_amount = ntrn_usdc_amount.unwrap_or(max_available_ntrn_usdc_amount);
        let slippage_tolerance = slippage_tolerance.unwrap_or(migration_config.max_slippage);

        let mut resp = Response::default();
        if !ntrn_atom_amount.is_zero() {
            resp = resp.add_message(
                CallbackMsg::MigrateLiquidityToClPair {
                    ntrn_denom: migration_config.ntrn_denom.clone(),
                    amount: ntrn_atom_amount,
                    slippage_tolerance,
                    xyk_pair: migration_config.ntrn_atom_xyk_pair,
                    xyk_lp_token: ntrn_atom_pair_info.liquidity_token,
                    cl_pair: migration_config.ntrn_atom_cl_pair,
                    paired_asset_denom: migration_config.atom_denom,
                    user
                }
                    .to_cosmos_msg(&env)?,
            );
        }
        if !ntrn_usdc_amount.is_zero() {
            resp = resp.add_message(
                CallbackMsg::MigrateLiquidityToClPair {
                    ntrn_denom: migration_config.ntrn_denom,
                    amount: ntrn_usdc_amount,
                    slippage_tolerance,
                    xyk_pair: migration_config.ntrn_usdc_xyk_pair,
                    xyk_lp_token: ntrn_usdc_pair_info.liquidity_token,
                    cl_pair: migration_config.ntrn_usdc_cl_pair,
                    paired_asset_denom: migration_config.usdc_denom,
                    user: user.clone().address,
                }
                    .to_cosmos_msg(&env)?,
            );
        }

        Ok(resp)
    }

    Ok(Default::default())
}


fn _handle_callback(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> Result<Response, ContractError> {
    // Only the contract itself can call callbacks
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    match msg {
        CallbackMsg::MigrateLiquidityToClPair {
            xyk_pair,
            xyk_lp_token,
            amount,
            slippage_tolerance,
            cl_pair,
            ntrn_denom,
            paired_asset_denom,
            user
        } => migrate_liquidity_to_cl_pair_callback(
            deps,
            env,
            xyk_pair,
            xyk_lp_token,
            amount,
            slippage_tolerance,
            cl_pair,
            ntrn_denom,
            paired_asset_denom,
            user
        ),
        CallbackMsg::ProvideLiquidityToClPairAfterWithdrawal {
            ntrn_denom,
            ntrn_init_balance,
            paired_asset_denom,
            paired_asset_init_balance,
            cl_pair: cl_pair_address,
            slippage_tolerance,
            user
        } => provide_liquidity_to_cl_pair_after_withdrawal_callback(
            deps,
            env,
            ntrn_denom,
            ntrn_init_balance,
            paired_asset_denom,
            paired_asset_init_balance,
            cl_pair_address,
            slippage_tolerance,
            user
        ),
        CallbackMsg::PostMigrationBalancesCheck {
            ntrn_denom,
            ntrn_init_balance,
            paired_asset_denom,
            paired_asset_init_balance,
            user
        } => post_migration_balances_check_callback(
            deps,
            env,
            ntrn_denom,
            ntrn_init_balance,
            paired_asset_denom,
            paired_asset_init_balance,
            user
        ),
    }
}


#[allow(clippy::too_many_arguments)]
fn migrate_liquidity_to_cl_pair_callback(
    deps: DepsMut,
    env: Env,
    xyk_pair: Addr,
    xyk_lp_token: Addr,
    amount: Uint128,
    slippage_tolerance: Decimal,
    cl_pair: Addr,
    ntrn_denom: String,
    paired_asset_denom: String,
    user: VestingAccountResponse
) -> Result<Response, ContractError> {
    let ntrn_init_balance = deps
        .querier
        .query_balance(env.contract.address.to_string(), ntrn_denom.clone())?
        .amount;
    let paired_asset_init_balance = deps
        .querier
        .query_balance(env.contract.address.to_string(), paired_asset_denom.clone())?
        .amount;

    let reward_amount = get_reward_amount(
        deps.as_ref(),
        &env,
        &pool.lp_token.to_string(),
        &generator.to_string(),
    )?;

    let user_share = compute_share(&user.info)?;
    let user_rewards_share = reward_amount.to_decimal().checked_mul(user_share)?;

    //unstake from generator
    attrs.push(attr(
        "unstake_from_generator",
        user_rewards_share,
    ));
    msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: generator.to_string(),
        funds: vec![],
        msg: to_binary(&astroport::generator::ExecuteMsg::Withdraw {
            lp_token: pool.lp_token.to_string(),
            amount: Uint128::from(user_rewards_share),
        })?,
    }));


    // TODO send shared generator ins to user

    let msgs: Vec<CosmosMsg> = vec![
        // push message to withdraw liquidity from the xyk pair
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: xyk_lp_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: xyk_pair.to_string(),
                amount,
                msg: to_binary(&PairCw20HookMsg::WithdrawLiquidity { assets: vec![] })?,
            })?,
            funds: vec![],
        }),
        // push the next migration step as a callback message
        CallbackMsg::ProvideLiquidityToClPairAfterWithdrawal {
            ntrn_denom,
            ntrn_init_balance,
            paired_asset_denom,
            paired_asset_init_balance,
            cl_pair,
            slippage_tolerance,
            user
        }
            .to_cosmos_msg(&env)?,
    ];

    Ok(Response::default().add_messages(msgs))
}

#[allow(clippy::too_many_arguments)]
fn provide_liquidity_to_cl_pair_after_withdrawal_callback(
    deps: DepsMut,
    env: Env,
    ntrn_denom: String,
    ntrn_init_balance: Uint128,
    paired_asset_denom: String,
    paired_asset_init_balance: Uint128,
    cl_pair_address: Addr,
    slippage_tolerance: Decimal,
    user: VestingAccountResponse
) -> Result<Response, ContractError> {
    let ntrn_balance_after_withdrawal = deps
        .querier
        .query_balance(user.to_string(), ntrn_denom.clone())?
        .amount;
    let paired_asset_balance_after_withdrawal = deps
        .querier
        .query_balance(user.to_string(), paired_asset_denom.clone())?
        .amount;

    // calc amount of assets that's been withdrawn
    let withdrawn_ntrn_amount = ntrn_balance_after_withdrawal.checked_sub(ntrn_init_balance)?;
    let withdrawn_paired_asset_amount =
        paired_asset_balance_after_withdrawal.checked_sub(paired_asset_init_balance)?;

    let msgs: Vec<CosmosMsg> = vec![
        // push message to provide liquidity to the CL pair
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cl_pair_address.to_string(),
            msg: to_binary(&PairExecuteMsg::ProvideLiquidity {
                assets: vec![
                    native_asset(ntrn_denom.clone(), withdrawn_ntrn_amount),
                    native_asset(paired_asset_denom.clone(), withdrawn_paired_asset_amount),
                ],
                slippage_tolerance: Some(slippage_tolerance),
                auto_stake: None,
                receiver: None,
            })?,
            funds: vec![
                Coin::new(withdrawn_ntrn_amount.into(), ntrn_denom.clone()),
                Coin::new(
                    withdrawn_paired_asset_amount.into(),
                    paired_asset_denom.clone(),
                ),
            ],
        }),
        // push the next migration step as a callback message
        CallbackMsg::PostMigrationBalancesCheck {
            ntrn_denom,
            ntrn_init_balance,
            paired_asset_denom,
            paired_asset_init_balance,
            user
        }
            .to_cosmos_msg(&env)?,
    ];

    Ok(Response::default().add_messages(msgs))
}

fn get_reward_amount(
    deps: Deps,
    env: &Env,
    astroport_lp_token: &String,
    generator: &String,
) -> StdResult<Uint128> {
    let rwi: RewardInfoResponse = deps.querier.query_wasm_smart(
        generator,
        &GenQueryMsg::RewardInfo {
            lp_token: astroport_lp_token.to_string(),
        },
    )?;

    let reward_token_balance = deps
        .querier
        .query_balance(
            env.contract.address.clone(),
            rwi.base_reward_token.to_string(),
        )?
        .amount;

    Ok(reward_token_balance)
}

fn post_migration_balances_check_callback(
    deps: DepsMut,
    env: Env,
    ntrn_denom: String,
    ntrn_init_balance: Uint128,
    paired_asset_denom: String,
    paired_asset_init_balance: Uint128,
    user: Addr
) -> Result<Response, ContractError> {
    // let ntrn_balance = deps
    //     .querier
    //     .query_balance(user.to_string(), ntrn_denom.clone())?
    //     .amount;
    // let paired_asset_balance = deps
    //     .querier
    //     .query_balance(user.to_string(), paired_asset_denom.clone())?
    //     .amount;
    //
    // if !ntrn_balance.eq(&ntrn_init_balance) {
    //     return Err(ContractError::MigrationBalancesMismatch {
    //         denom: ntrn_denom,
    //         initial_balance: ntrn_init_balance,
    //         final_balance: ntrn_balance,
    //     });
    // }
    // if !paired_asset_balance.eq(&paired_asset_init_balance) {
    //     return Err(ContractError::MigrationBalancesMismatch {
    //         denom: paired_asset_denom,
    //         initial_balance: paired_asset_init_balance,
    //         final_balance: paired_asset_balance,
    //     });
    // }
    //
    Ok(Response::default())
}




/// Exposes all the queries available in the contract.
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => Ok(to_binary(&query_config(deps)?)?),
        QueryMsg::VestingAccount { address } => {
            Ok(to_binary(&query_vesting_account(deps, address)?)?)
        }
        QueryMsg::VestingAccounts {
            start_after,
            limit,
            order_by,
        } => Ok(to_binary(&query_vesting_accounts(
            deps,
            start_after,
            limit,
            order_by,
        )?)?),
        QueryMsg::AvailableAmount { address } => Ok(to_binary(&query_vesting_available_amount(
            deps, env, address,
        )?)?),
        QueryMsg::VestingState {} => Ok(to_binary(&query_vesting_state(deps)?)?),
        QueryMsg::Timestamp {} => Ok(to_binary(&query_timestamp(env)?)?),
        QueryMsg::ManagedExtension { msg } => handle_query_managed_msg(deps, env, msg),
        QueryMsg::WithManagersExtension { msg } => handle_query_managers_msg(deps, env, msg),
        QueryMsg::HistoricalExtension { msg } => handle_query_historical_msg(deps, env, msg),
    }
}

/// Returns the vesting contract configuration using a [`Config`] object.
fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}

/// Returns the accumulated vesting information for all addresses using a [`VestingState`] object.
fn query_vesting_state(deps: Deps) -> StdResult<VestingState> {
    let config = CONFIG.load(deps.storage)?;
    let state = vesting_state(config.extensions.historical).load(deps.storage)?;

    Ok(state)
}

/// Return the current block timestamp (in seconds)
/// * **env** is an object of type [`Env`].
fn query_timestamp(env: Env) -> StdResult<u64> {
    Ok(env.block.time.seconds())
}

/// Returns the vesting data for a specific vesting recipient using a [`VestingAccountResponse`] object.
///
/// * **address** vesting recipient for which to return vesting data.
fn query_vesting_account(deps: Deps, address: String) -> StdResult<VestingAccountResponse> {
    let address = deps.api.addr_validate(&address)?;
    let config = CONFIG.load(deps.storage)?;
    let info = vesting_info(config.extensions.historical).load(deps.storage, address.clone())?;

    Ok(VestingAccountResponse { address, info })
}

/// Returns a list of vesting schedules using a [`VestingAccountsResponse`] object.
///
/// * **start_after** index from which to start reading vesting schedules.
///
/// * **limit** amount of vesting schedules to return.
///
/// * **order_by** whether results should be returned in an ascending or descending order.
fn query_vesting_accounts(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<VestingAccountsResponse> {
    let start_after = addr_opt_validate(deps.api, &start_after)?;

    let vesting_infos = read_vesting_infos(deps, start_after, limit, order_by)?;

    let vesting_accounts: Vec<_> = vesting_infos
        .into_iter()
        .map(|(address, info)| VestingAccountResponse { address, info })
        .collect();

    Ok(VestingAccountsResponse { vesting_accounts })
}

/// Returns the available amount of vested and yet to be claimed tokens for a specific vesting recipient.
///
/// * **address** vesting recipient for which to return the available amount of tokens to claim.
fn query_vesting_available_amount(deps: Deps, env: Env, address: String) -> StdResult<Uint128> {
    let address = deps.api.addr_validate(&address)?;

    let config = CONFIG.load(deps.storage)?;
    let info = vesting_info(config.extensions.historical).load(deps.storage, address)?;
    let available_amount = compute_available_amount(env.block.time.seconds(), &info)?;
    Ok(available_amount)
}

/// Manages contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    XYK_TO_CL_MIGRATION_CONFIG.save(
        deps.storage,
        &XykToClMigrationConfig {
            max_slippage: msg.max_slippage,
            ntrn_denom: msg.ntrn_denom,
            atom_denom: msg.atom_denom,
            ntrn_atom_xyk_pair: deps.api.addr_validate(msg.ntrn_atom_xyk_pair.as_str())?,
            ntrn_atom_cl_pair: deps.api.addr_validate(msg.ntrn_atom_cl_pair.as_str())?,
            usdc_denom: msg.usdc_denom,
            ntrn_usdc_xyk_pair: deps.api.addr_validate(msg.ntrn_usdc_xyk_pair.as_str())?,
            ntrn_usdc_cl_pair: deps.api.addr_validate(msg.ntrn_usdc_cl_pair.as_str())?,
        },
    )?;
    Ok(Response::default())
}


fn is_sender_whitelisted(store: &mut dyn Storage, config: &Config, sender: &Addr) -> bool {
    if *sender == config.owner {
        return true;
    }
    if VESTING_MANAGERS.has(store, sender.clone()) {
        return true;
    }
    false
}

/// Asserts the validity of a list of vesting schedules.
///
/// * **addr** receiver of the vested tokens.
///
/// * **vesting_schedules** vesting schedules to validate.
fn assert_vesting_schedules(
    addr: &Addr,
    vesting_schedules: &[VestingSchedule],
) -> Result<(), ContractError> {
    for sch in vesting_schedules {
        if let Some(end_point) = &sch.end_point {
            if !(sch.start_point.time < end_point.time && sch.start_point.amount < end_point.amount)
            {
                return Err(ContractError::VestingScheduleError(addr.to_string()));
            }
        }
    }

    Ok(())
}

/// Computes the amount of vested and yet unclaimed tokens for a specific vesting recipient.
/// Returns the computed amount if the operation is successful.
///
/// * **current_time** timestamp from which to start querying for vesting schedules.
/// Schedules that started later than current_time will be omitted.
///
/// * **vesting_info** vesting schedules for which to compute the amount of tokens
/// that are vested and can be claimed by the recipient.
fn compute_available_amount(current_time: u64, vesting_info: &VestingInfo) -> StdResult<Uint128> {
    let mut available_amount: Uint128 = Uint128::zero();
    for sch in &vesting_info.schedules {
        if sch.start_point.time > current_time {
            continue;
        }

        available_amount = available_amount.checked_add(sch.start_point.amount)?;

        if let Some(end_point) = &sch.end_point {
            let passed_time = current_time.min(end_point.time) - sch.start_point.time;
            let time_period = end_point.time - sch.start_point.time;
            if passed_time != 0 && time_period != 0 {
                let release_amount = Uint128::from(passed_time).multiply_ratio(
                    end_point.amount.checked_sub(sch.start_point.amount)?,
                    time_period,
                );
                available_amount = available_amount.checked_add(release_amount)?;
            }
        }
    }

    available_amount
        .checked_sub(vesting_info.released_amount)
        .map_err(StdError::from)
}

fn compute_share(vesting_info: &VestingInfo) -> StdResult<Decimal> {
    let config = CONFIG.load(deps.storage)?;
    let state = vesting_state(config.extensions.historical).load(deps.storage)?;
    let mut available_amount: Uint128 = Uint128::zero();
    for sch in &vesting_info.schedules {

        if let Some(end_point) = &sch.end_point {
            available_amount = available_amount.checked_add(end_point.amount)?
        }
    }

    available_amount
        .checked_div(state.total_granted)?
        .map_err(StdError::from)
}
