use crate::enumerable::query_all_address_map;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, to_binary, Attribute, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};
use neutron_sdk::bindings::msg::{IbcFee, NeutronMsg};
use neutron_sdk::sudo::msg::RequestPacketTimeoutHeight;
use sha2::Digest;
use std::convert::TryInto;

use crate::error::ContractError;
use crate::msg::{
    AccountMapResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, IsClaimedResponse,
    IsPausedResponse, MerkleRootResponse, MigrateMsg, QueryMsg, TotalClaimedResponse,
};
use crate::state::{
    Config, ACCOUNT_MAP, AIRDROP_START, AMOUNT, AMOUNT_CLAIMED, CLAIM, CONFIG, HRP, MERKLE_ROOT,
    PAUSED, VESTING_DURATION, VESTING_START,
};
use credits::msg::ExecuteMsg::AddVesting;

// Version info, for migration info
const CONTRACT_NAME: &str = "crates.io:cw20-merkle-airdrop";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NEUTRON_DENOM: &str = "untrn";
const DEFAULT_TRANSFER_CHANNEL: &str = "channel-1";
const DEFAULT_IBC_FEE_DENOM: &str = "untrn";
const IBC_TRANSFER_PORT: &str = "transfer";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<NeutronMsg>, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    PAUSED.save(deps.storage, &false)?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: info.sender,
            credits_address: deps.api.addr_validate(&msg.credits_address)?,
            cosmos_hub_treasury: msg.cosmos_hub_treasury,
            transfer_channel: msg
                .transfer_channel
                .unwrap_or(DEFAULT_TRANSFER_CHANNEL.to_string()),
        },
    )?;

    // check merkle root length
    let mut root_buf: [u8; 32] = [0; 32];
    hex::decode_to_slice(&msg.merkle_root, &mut root_buf)?;

    MERKLE_ROOT.save(deps.storage, &msg.merkle_root)?;

    if msg.vesting_start < msg.airdrop_start {
        return Err(ContractError::VestingBeforeAirdrop {
            airdrop_start: msg.airdrop_start,
            vesting_start: msg.vesting_start,
        });
    }

    AIRDROP_START.save(deps.storage, &msg.airdrop_start)?;
    VESTING_START.save(deps.storage, &msg.vesting_start)?;
    VESTING_DURATION.save(deps.storage, &msg.vesting_duration_seconds)?;

    // save hrp
    if let Some(hrp) = msg.hrp {
        HRP.save(deps.storage, &hrp)?;
    }

    // save total airdropped amount
    let amount = msg.total_amount.unwrap_or_else(Uint128::zero);
    AMOUNT.save(deps.storage, &amount)?;
    AMOUNT_CLAIMED.save(deps.storage, &Uint128::zero())?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "instantiate"),
        attr("merkle_root", msg.merkle_root),
        attr("total_amount", amount),
    ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<NeutronMsg>, ContractError> {
    match msg {
        ExecuteMsg::Claim { amount, proof } => execute_claim(deps, env, info, amount, proof),
        ExecuteMsg::WithdrawAll {
            recv_fee,
            ack_fee,
            timeout_fee,
            timeout_revision,
            timeout_height,
            denom,
        } => execute_withdraw_all(
            deps,
            env,
            info,
            recv_fee,
            ack_fee,
            timeout_fee,
            timeout_revision,
            timeout_height,
            denom,
        ),
        ExecuteMsg::Pause {} => execute_pause(deps, env, info),
        ExecuteMsg::Resume {} => execute_resume(deps, env, info),
        ExecuteMsg::UpdateConfig {
            new_transfer_channel,
        } => execute_update_config(deps, env, info, new_transfer_channel),
    }
}

pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_transfer_channel: Option<String>,
) -> Result<Response<NeutronMsg>, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let mut attrs: Vec<Attribute> = vec![attr("action", "update_config")];
    if let Some(channel) = new_transfer_channel {
        config.transfer_channel = channel.clone();
        attrs.push(attr("new_transfer_channel", channel))
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::new().add_attributes(attrs))
}

pub fn execute_claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    proof: Vec<String>,
) -> Result<Response<NeutronMsg>, ContractError> {
    // airdrop begun
    let start = AIRDROP_START.load(deps.storage)?;
    if env.block.time.seconds() < start {
        return Err(ContractError::NotBegun { start });
    }
    // not expired
    let vesting_start = VESTING_START.load(deps.storage)?;
    let vesting_duration = VESTING_DURATION.load(deps.storage)?;
    let expiration = vesting_start + vesting_duration;
    if env.block.time.seconds() > expiration {
        return Err(ContractError::Expired { expiration });
    }

    let is_paused = PAUSED.load(deps.storage)?;
    if is_paused {
        return Err(ContractError::Paused {});
    }

    // if present verify signature and extract external address or use info.sender as proof
    // if signature is not present in the message, verification will fail since info.sender is not present in the merkle root
    let proof_addr = info.sender.to_string();

    // verify not claimed
    let claimed = CLAIM.may_load(deps.storage, proof_addr.clone())?;
    if claimed.is_some() {
        return Err(ContractError::Claimed {});
    }

    // verify merkle root
    let config = CONFIG.load(deps.storage)?;
    let merkle_root = MERKLE_ROOT.load(deps.storage)?;

    let user_input = format!("{}{}", proof_addr, amount);
    let hash = sha2::Sha256::digest(user_input.as_bytes())
        .as_slice()
        .try_into()
        .map_err(|_| ContractError::WrongLength {})?;

    let hash = proof.into_iter().try_fold(hash, |hash, p| {
        let mut proof_buf = [0; 32];
        hex::decode_to_slice(p, &mut proof_buf)?;
        let mut hashes = [hash, proof_buf];
        hashes.sort_unstable();
        sha2::Sha256::digest(&hashes.concat())
            .as_slice()
            .try_into()
            .map_err(|_| ContractError::WrongLength {})
    })?;

    let mut root_buf: [u8; 32] = [0; 32];
    hex::decode_to_slice(merkle_root, &mut root_buf)?;
    if root_buf != hash {
        return Err(ContractError::VerificationFailed {});
    }

    // Update claim index
    CLAIM.save(deps.storage, proof_addr, &true)?;

    // Update total claimed to reflect
    let mut claimed_amount = AMOUNT_CLAIMED.load(deps.storage)?;
    claimed_amount += amount;
    AMOUNT_CLAIMED.save(deps.storage, &claimed_amount)?;

    let transfer_message = WasmMsg::Execute {
        contract_addr: config.credits_address.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.to_string(),
            amount,
        })?,
        funds: vec![],
    };

    let vesting_message = WasmMsg::Execute {
        contract_addr: config.credits_address.to_string(),
        msg: to_binary(&AddVesting {
            address: info.sender.to_string(),
            amount,
            start_time: vesting_start,
            duration: vesting_duration,
        })?,
        funds: vec![],
    };
    let res = Response::new()
        .add_message(transfer_message)
        .add_message(vesting_message)
        .add_attributes(vec![
            attr("action", "claim"),
            attr("address", info.sender.to_string()),
            attr("amount", amount),
        ]);
    Ok(res)
}

fn get_fee_item(denom: String, amount: u128) -> Vec<Coin> {
    if amount == 0 {
        vec![]
    } else {
        vec![coin(amount, denom)]
    }
}

#[allow(clippy::too_many_arguments)]
pub fn execute_withdraw_all(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recv_fee: Uint128,
    ack_fee: Uint128,
    timeout_fee: Uint128,
    timeout_revision: u64,
    timeout_height: u64,
    denom: Option<String>,
) -> Result<Response<NeutronMsg>, ContractError> {
    let vesting_start = VESTING_START.load(deps.storage)?;
    let vesting_duration = VESTING_DURATION.load(deps.storage)?;
    let expiration = vesting_start + vesting_duration;
    deps.api.debug(&format!(
        "now: {} then {}",
        env.block.time.seconds(),
        expiration
    ));
    if env.block.time.seconds() <= expiration {
        return Err(ContractError::WithdrawAllUnavailable {
            available_at: expiration,
        });
    }

    let is_paused = PAUSED.load(deps.storage)?;
    if is_paused {
        return Err(ContractError::Paused {});
    }

    // Get the current total balance for the contract and burn it all.
    // By burning, we exchange them for NTRN tokens
    let cfg = CONFIG.load(deps.storage)?;
    let cntrn_amount = deps
        .querier
        .query_wasm_smart::<BalanceResponse>(
            cfg.credits_address.to_string(),
            &Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?
        .balance;
    let mut msgs: Vec<CosmosMsg<NeutronMsg>> = vec![];
    let amount_to_withdraw = if !cntrn_amount.is_zero() {
        // Generate burn submessage and return a response
        let burn_message = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cfg.credits_address.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Burn {
                amount: cntrn_amount,
            })?,
            funds: vec![],
        });
        msgs.push(burn_message);
        cntrn_amount
    } else {
        // previous try of execute_withdraw_all failed on ibc stage
        // we already have burned our cNRTN and got NTRN on contract account.
        deps.querier
            .query_balance(env.contract.address.clone(), NEUTRON_DENOM)?
            .amount
    };

    let fee_denom = denom.unwrap_or(DEFAULT_IBC_FEE_DENOM.to_string());
    let required_fees = recv_fee + ack_fee + timeout_fee;
    if let Some(funded_fee) = info.funds.iter().find(|c| c.denom == fee_denom) {
        if funded_fee.amount < required_fees {
            return Err(ContractError::Underfunded {
                got_fees: funded_fee.amount.u128(),
                required_fees: required_fees.u128(),
            });
        }
    }
    let fee = IbcFee {
        recv_fee: get_fee_item(fee_denom.clone(), recv_fee.u128()),
        ack_fee: get_fee_item(fee_denom.clone(), ack_fee.u128()),
        timeout_fee: get_fee_item(fee_denom, timeout_fee.u128()),
    };
    let send_message = CosmosMsg::Custom(NeutronMsg::IbcTransfer {
        source_port: IBC_TRANSFER_PORT.to_string(),
        source_channel: cfg.transfer_channel,
        sender: env.contract.address.to_string(),
        receiver: cfg.cosmos_hub_treasury.clone(),
        token: coin(amount_to_withdraw.u128(), NEUTRON_DENOM),
        timeout_height: RequestPacketTimeoutHeight {
            revision_number: Some(timeout_revision),
            revision_height: Some(timeout_height),
        },
        timeout_timestamp: 0,
        fee,
        memo: "".to_string(),
    });
    msgs.push(send_message);
    let res = Response::new().add_messages(msgs).add_attributes(vec![
        attr("action", "withdraw_all"),
        attr("address", info.sender),
        attr("amount", cntrn_amount),
        attr("recipient", cfg.cosmos_hub_treasury),
    ]);
    Ok(res)
}

pub fn execute_pause(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response<NeutronMsg>, ContractError> {
    // authorize owner
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    let start = AIRDROP_START.load(deps.storage)?;
    if env.block.time.seconds() < start {
        return Err(ContractError::NotBegun { start });
    }

    let vesting_start = VESTING_START.load(deps.storage)?;
    let vesting_duration = VESTING_DURATION.load(deps.storage)?;
    let expiration = vesting_start + vesting_duration;
    if env.block.time.seconds() > expiration {
        return Err(ContractError::Expired { expiration });
    }

    PAUSED.save(deps.storage, &true)?;
    Ok(Response::new().add_attributes(vec![attr("action", "pause"), attr("paused", "true")]))
}

pub fn execute_resume(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response<NeutronMsg>, ContractError> {
    // authorize owner
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    let start = AIRDROP_START.load(deps.storage)?;
    if env.block.time.seconds() < start {
        return Err(ContractError::NotBegun { start });
    }

    let vesting_start = VESTING_START.load(deps.storage)?;
    let vesting_duration = VESTING_DURATION.load(deps.storage)?;
    let expiration = vesting_start + vesting_duration;
    if env.block.time.seconds() > expiration {
        return Err(ContractError::Expired { expiration });
    }

    let is_paused = PAUSED.load(deps.storage)?;
    if !is_paused {
        return Err(ContractError::NotPaused {});
    }

    PAUSED.save(deps.storage, &false)?;
    Ok(Response::new().add_attributes(vec![attr("action", "resume"), attr("paused", "false")]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::MerkleRoot {} => to_binary(&query_merkle_root(deps)?),
        QueryMsg::IsClaimed { address } => to_binary(&query_is_claimed(deps, address)?),
        QueryMsg::IsPaused {} => to_binary(&query_is_paused(deps)?),
        QueryMsg::TotalClaimed {} => to_binary(&query_total_claimed(deps)?),
        QueryMsg::AccountMap { external_address } => {
            to_binary(&query_address_map(deps, external_address)?)
        }
        QueryMsg::AllAccountMaps { start_after, limit } => {
            to_binary(&query_all_address_map(deps, start_after, limit)?)
        }
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: cfg.owner.to_string(),
        credits_address: cfg.credits_address.to_string(),
        reserve_address: cfg.cosmos_hub_treasury,
    })
}

pub fn query_merkle_root(deps: Deps) -> StdResult<MerkleRootResponse> {
    let merkle_root = MERKLE_ROOT.load(deps.storage)?;
    let airdrop_start = AIRDROP_START.load(deps.storage)?;
    let vesting_start = VESTING_START.load(deps.storage)?;
    let vesting_duration_seconds = VESTING_DURATION.load(deps.storage)?;
    let total_amount = AMOUNT.load(deps.storage)?;

    Ok(MerkleRootResponse {
        merkle_root,
        airdrop_start,
        vesting_start,
        vesting_duration_seconds,
        total_amount,
    })
}

pub fn query_is_claimed(deps: Deps, address: String) -> StdResult<IsClaimedResponse> {
    let is_claimed = CLAIM.may_load(deps.storage, address)?.unwrap_or(false);
    let resp = IsClaimedResponse { is_claimed };

    Ok(resp)
}

pub fn query_is_paused(deps: Deps) -> StdResult<IsPausedResponse> {
    let is_paused = PAUSED.may_load(deps.storage)?.unwrap_or(false);
    let resp = IsPausedResponse { is_paused };

    Ok(resp)
}

pub fn query_total_claimed(deps: Deps) -> StdResult<TotalClaimedResponse> {
    let total_claimed = AMOUNT_CLAIMED.load(deps.storage)?;
    let resp = TotalClaimedResponse { total_claimed };

    Ok(resp)
}

pub fn query_address_map(deps: Deps, external_address: String) -> StdResult<AccountMapResponse> {
    let host_address = ACCOUNT_MAP.load(deps.storage, external_address.clone())?;
    let resp = AccountMapResponse {
        host_address,
        external_address,
    };

    Ok(resp)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
