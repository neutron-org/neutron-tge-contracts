use crate::enumerable::query_all_address_map;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, from_binary, to_binary, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Timestamp, Uint128, WasmMsg,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::{BalanceResponse, Cw20Contract, Cw20ExecuteMsg, Cw20QueryMsg};
use cw_utils::{Expiration, Scheduled};
use semver::Version;
use sha2::Digest;
use std::convert::TryInto;

use crate::error::ContractError;
use crate::helpers::CosmosSignature;
use crate::migrations::v0_12_1;
use crate::msg::{
    AccountMapResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, IsClaimedResponse,
    IsPausedResponse, MerkleRootResponse, MigrateMsg, QueryMsg, SignatureInfo,
    TotalClaimedResponse,
};
use crate::state::{
    Config, ACCOUNT_MAP, AMOUNT, AMOUNT_CLAIMED, CLAIM, CONFIG, HRP, MERKLE_ROOT, PAUSED,
    STAGE_EXPIRATION, START,
};
use credits::msg::ExecuteMsg::AddVesting;

// Version info, for migration info
const CONTRACT_NAME: &str = "crates.io:cw20-merkle-airdrop";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Vesting duration is 90 days
pub const VESTING_DURATION_SECONDS: u64 = 60 // seconds in minute
    * 60 // minutes in hour
    * 24 // hours in day
    * 90; // days
pub const NEUTRON_DENOM: &str = "untrn";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    PAUSED.save(deps.storage, &false)?;

    let credits_address = match msg.credits_address {
        Some(addr) => Some(deps.api.addr_validate(&addr)?),
        None => None,
    };
    let reserve_address = match msg.reserve_address {
        Some(addr) => Some(deps.api.addr_validate(&addr)?),
        None => None,
    };

    CONFIG.save(
        deps.storage,
        &Config {
            owner: info.sender,
            credits_address,
            reserve_address,
        },
    )?;

    // check merkle root length
    let mut root_buf: [u8; 32] = [0; 32];
    hex::decode_to_slice(&msg.merkle_root, &mut root_buf)?;

    MERKLE_ROOT.save(deps.storage, &msg.merkle_root)?;

    // save expiration
    STAGE_EXPIRATION.save(deps.storage, &msg.expiration)?;

    // save start
    START.save(deps.storage, &msg.start)?;

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
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig {
            new_owner,
            new_credits_address,
            new_reserve_address,
        } => execute_update_config(
            deps,
            env,
            info,
            new_owner,
            new_credits_address,
            new_reserve_address,
        ),
        ExecuteMsg::Claim {
            amount,
            proof,
            sig_info,
        } => execute_claim(deps, env, info, amount, proof, sig_info),
        ExecuteMsg::WithdrawAll {} => execute_withdraw_all(deps, env, info),
        ExecuteMsg::Pause {} => execute_pause(deps, env, info),
        ExecuteMsg::Resume { new_expiration } => execute_resume(deps, env, info, new_expiration),
    }
}

pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_owner: Option<String>,
    credits_address: Option<String>,
    reserve_address: Option<String>,
) -> Result<Response, ContractError> {
    // authorize owner
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    let owner = new_owner.map_or(Ok(cfg.owner), |addr| deps.api.addr_validate(&addr))?;
    let credits_address = match credits_address {
        Some(addr) => Some(deps.api.addr_validate(&addr)?),
        None => cfg.credits_address,
    };
    let reserve_address = match reserve_address {
        Some(addr) => Some(deps.api.addr_validate(&addr)?),
        None => cfg.reserve_address,
    };

    CONFIG.save(
        deps.storage,
        &Config {
            owner,
            credits_address,
            reserve_address,
        },
    )?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

pub fn execute_claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    proof: Vec<String>,
    sig_info: Option<SignatureInfo>,
) -> Result<Response, ContractError> {
    // airdrop begun
    let start = START.load(deps.storage)?;
    if !Scheduled::AtTime(start).is_triggered(&env.block) {
        return Err(ContractError::NotBegun { start });
    }
    // not expired
    let expiration = STAGE_EXPIRATION.load(deps.storage)?;
    if Expiration::AtTime(expiration).is_expired(&env.block) {
        return Err(ContractError::Expired { expiration });
    }

    let is_paused = PAUSED.load(deps.storage)?;
    if is_paused {
        return Err(ContractError::Paused {});
    }

    // if present verify signature and extract external address or use info.sender as proof
    // if signature is not present in the message, verification will fail since info.sender is not present in the merkle root
    let proof_addr = match sig_info {
        None => info.sender.to_string(),
        Some(sig) => {
            // verify signature
            let cosmos_signature: CosmosSignature = from_binary(&sig.signature)?;
            cosmos_signature.verify(deps.as_ref(), &sig.claim_msg)?;
            // get airdrop bech32 prefix and derive proof address from public key
            let hrp = HRP.load(deps.storage)?;
            let proof_addr = cosmos_signature.derive_addr_from_pubkey(hrp.as_str())?;

            if sig.extract_addr()? != info.sender {
                return Err(ContractError::VerificationFailed {});
            }

            // Save external address index
            ACCOUNT_MAP.save(deps.storage, proof_addr.clone(), &info.sender.to_string())?;

            proof_addr
        }
    };

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

    let credits_address = match config.credits_address {
        Some(addr) => addr,
        None => return Err(ContractError::CreditsAddress {}),
    };

    // Update claim index
    CLAIM.save(deps.storage, proof_addr, &true)?;

    // Update total claimed to reflect
    let mut claimed_amount = AMOUNT_CLAIMED.load(deps.storage)?;
    claimed_amount += amount;
    AMOUNT_CLAIMED.save(deps.storage, &claimed_amount)?;

    // we stop all airdrops at the date of expiration, and we use the very same date
    // as a start timestamp for vesting.
    let vesting_start_time = expiration;

    let transfer_message = Cw20Contract(credits_address.clone())
        .call(Cw20ExecuteMsg::Transfer {
            recipient: info.sender.to_string(),
            amount,
        })
        .map_err(ContractError::Std)?;
    let vesting_message = WasmMsg::Execute {
        contract_addr: credits_address.to_string(),
        msg: to_binary(&AddVesting {
            address: info.sender.to_string(),
            amount,
            start_time: vesting_start_time.seconds(),
            duration: VESTING_DURATION_SECONDS,
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

pub fn execute_withdraw_all(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // authorize owner
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    if !PAUSED.load(deps.storage)?
        && env.block.time
            <= STAGE_EXPIRATION
                .load(deps.storage)?
                .plus_seconds(VESTING_DURATION_SECONDS)
    {
        return Err(ContractError::Std(StdError::generic_err(
            "withdraw_all only works 3 months after the end of the event",
        )));
    }

    let reserve_address = match cfg.reserve_address {
        Some(addr) => addr,
        None => return Err(ContractError::ReserveAddress {}),
    };

    let credits_address = match cfg.credits_address {
        Some(addr) => addr,
        None => return Err(ContractError::CreditsAddress {}),
    };

    // Get the current total balance for the contract and burn it all.
    // By burning, we exchange them for NTRN tokens
    let amount_to_withdraw = deps
        .querier
        .query_wasm_smart::<BalanceResponse>(
            credits_address.to_string(),
            &Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?
        .balance;

    // Generate burn submessage and return a response
    let burn_message = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: credits_address.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Burn {
            amount: amount_to_withdraw,
        })?,
        funds: vec![],
    });
    let send_message = CosmosMsg::Bank(BankMsg::Send {
        to_address: reserve_address.to_string(),
        amount: vec![coin(amount_to_withdraw.u128(), NEUTRON_DENOM)],
    });
    let res = Response::new()
        .add_messages([burn_message, send_message])
        .add_attributes(vec![
            attr("action", "withdraw_all"),
            attr("address", info.sender),
            attr("amount", amount_to_withdraw),
            attr("recipient", reserve_address),
        ]);
    Ok(res)
}

pub fn execute_pause(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    // authorize owner
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    let start = START.load(deps.storage)?;
    if !Scheduled::AtTime(start).is_triggered(&env.block) {
        return Err(ContractError::NotBegun { start });
    }

    let expiration = STAGE_EXPIRATION.load(deps.storage)?;
    if Expiration::AtTime(expiration).is_expired(&env.block) {
        return Err(ContractError::Expired { expiration });
    }

    PAUSED.save(deps.storage, &true)?;
    Ok(Response::new().add_attributes(vec![attr("action", "pause"), attr("paused", "true")]))
}

pub fn execute_resume(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    new_expiration: Option<Timestamp>,
) -> Result<Response, ContractError> {
    // authorize owner
    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    let start = START.load(deps.storage)?;
    if !Scheduled::AtTime(start).is_triggered(&env.block) {
        return Err(ContractError::NotBegun { start });
    }

    let expiration = STAGE_EXPIRATION.load(deps.storage)?;
    if Expiration::AtTime(expiration).is_expired(&env.block) {
        return Err(ContractError::Expired { expiration });
    }

    let is_paused = PAUSED.load(deps.storage)?;
    if !is_paused {
        return Err(ContractError::NotPaused {});
    }

    if let Some(new_expiration) = new_expiration {
        if Expiration::AtTime(new_expiration).is_expired(&env.block) {
            return Err(ContractError::Expired { expiration });
        }
        STAGE_EXPIRATION.save(deps.storage, &new_expiration)?;
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
        credits_address: cfg.credits_address.map(|addr| addr.to_string()),
        reserve_address: cfg.reserve_address.map(|addr| addr.to_string()),
    })
}

pub fn query_merkle_root(deps: Deps) -> StdResult<MerkleRootResponse> {
    let merkle_root = MERKLE_ROOT.load(deps.storage)?;
    let expiration = STAGE_EXPIRATION.load(deps.storage)?;
    let start = START.load(deps.storage)?;
    let total_amount = AMOUNT.load(deps.storage)?;

    let resp = MerkleRootResponse {
        merkle_root,
        expiration,
        start,
        total_amount,
    };

    Ok(resp)
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
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_info = get_contract_version(deps.storage)?;
    if contract_info.contract != CONTRACT_NAME {
        return Err(ContractError::CannotMigrate {
            previous_contract: contract_info.contract,
        });
    }
    let contract_version: Version = contract_info.version.parse()?;
    let current_version: Version = CONTRACT_VERSION.parse()?;
    if contract_version < current_version {
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        v0_12_1::set_initial_pause_status(deps)?;
        Ok(Response::default())
    } else {
        Err(ContractError::CannotMigrate {
            previous_contract: contract_info.version,
        })
    }
}
