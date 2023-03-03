use crate::enumerable::query_all_address_map;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, from_binary, to_binary, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
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
const VESTING_DURATION_SECONDS: u64 = 60 // seconds in minute
    * 60 // minutes in hour
    * 24 // hours in day
    * 90; // days

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let owner = msg
        .owner
        .map_or(Ok(info.sender), |o| deps.api.addr_validate(&o))?;

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
            owner: Some(owner),
            credits_address,
            reserve_address,
            neutron_denom: msg.neutron_denom,
        },
    )?;

    Ok(Response::default())
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
            new_neutron_denom,
        } => execute_update_config(
            deps,
            env,
            info,
            new_owner,
            new_credits_address,
            new_reserve_address,
            new_neutron_denom,
        ),
        ExecuteMsg::RegisterMerkleRoot {
            merkle_root,
            expiration,
            start,
            total_amount,
            hrp,
        } => execute_register_merkle_root(
            deps,
            env,
            info,
            merkle_root,
            expiration,
            start,
            total_amount,
            hrp,
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
    neutron_denom: Option<String>,
) -> Result<Response, ContractError> {
    // authorize owner
    let cfg = CONFIG.load(deps.storage)?;
    let owner = cfg.owner.ok_or(ContractError::Unauthorized {})?;
    if info.sender != owner {
        return Err(ContractError::Unauthorized {});
    }

    // if owner some validated to addr, otherwise set to none
    let mut tmp_owner = None;
    if let Some(addr) = new_owner {
        tmp_owner = Some(deps.api.addr_validate(&addr)?)
    }

    let credits_address = match credits_address {
        Some(addr) => Some(deps.api.addr_validate(&addr)?),
        None => cfg.credits_address,
    };
    let reserve_address = match reserve_address {
        Some(addr) => Some(deps.api.addr_validate(&addr)?),
        None => cfg.reserve_address,
    };
    let neutron_denom = match neutron_denom {
        Some(denom) => denom,
        None => cfg.neutron_denom,
    };

    CONFIG.save(
        deps.storage,
        &Config {
            owner: tmp_owner,
            credits_address,
            reserve_address,
            neutron_denom,
        },
    )?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

#[allow(clippy::too_many_arguments)]
pub fn execute_register_merkle_root(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    merkle_root: String,
    expiration: Option<Expiration>,
    start: Option<Scheduled>,
    total_amount: Option<Uint128>,
    hrp: Option<String>,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    // if owner set validate, otherwise unauthorized
    let owner = cfg.owner.ok_or(ContractError::Unauthorized {})?;
    if info.sender != owner {
        return Err(ContractError::Unauthorized {});
    }

    // check merkle root length
    let mut root_buf: [u8; 32] = [0; 32];
    hex::decode_to_slice(&merkle_root, &mut root_buf)?;

    MERKLE_ROOT.save(deps.storage, &merkle_root)?;

    // save expiration
    let exp = expiration.unwrap_or(Expiration::Never {});
    STAGE_EXPIRATION.save(deps.storage, &exp)?;

    // save start
    if let Some(start) = start {
        START.save(deps.storage, &start)?;
    }

    // save hrp
    if let Some(hrp) = hrp {
        HRP.save(deps.storage, &hrp)?;
    }

    // save total airdropped amount
    let amount = total_amount.unwrap_or_else(Uint128::zero);
    AMOUNT.save(deps.storage, &amount)?;
    AMOUNT_CLAIMED.save(deps.storage, &Uint128::zero())?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "register_merkle_root"),
        attr("merkle_root", merkle_root),
        attr("total_amount", amount),
    ]))
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
    let start = START.may_load(deps.storage)?;
    if let Some(start) = start {
        if !start.is_triggered(&env.block) {
            return Err(ContractError::NotBegun { start });
        }
    }
    // not expired
    let expiration = STAGE_EXPIRATION.load(deps.storage)?;
    if expiration.is_expired(&env.block) {
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
    // as a start timestamp for vesting. if expiration is not set, vesting will not work.
    let vesting_start_time =
        match expiration {
            Expiration::AtHeight(_) => {
                return Err(ContractError::Vesting {
                    description: "Vesting must be scheduled from timestamp, not block height"
                        .to_string(),
                })
            }
            Expiration::Never {} => return Err(ContractError::Vesting {
                description:
                    "Vesting must be scheduled from some timestamp, but there wasn't any provided"
                        .to_string(),
            }),
            Expiration::AtTime(timestamp) => timestamp,
        };

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
    let owner = cfg.owner.ok_or(ContractError::Unauthorized {})?;
    if info.sender != owner {
        return Err(ContractError::Unauthorized {});
    }

    if !PAUSED.load(deps.storage)?
        && env.block.time
            <= match STAGE_EXPIRATION.load(deps.storage)? {
                Expiration::AtTime(timestamp) => timestamp,
                _ => {
                    return Err(ContractError::Std(StdError::generic_err(
                        "withdraw_all only works if AtTime expiration is set",
                    )))
                }
            }
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
        amount: vec![coin(amount_to_withdraw.u128(), cfg.neutron_denom)],
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
    let owner = cfg.owner.ok_or(ContractError::Unauthorized {})?;
    if info.sender != owner {
        return Err(ContractError::Unauthorized {});
    }

    let start = START.may_load(deps.storage)?;
    if let Some(start) = start {
        if !start.is_triggered(&env.block) {
            return Err(ContractError::NotBegun { start });
        }
    }

    let expiration = STAGE_EXPIRATION.load(deps.storage)?;
    if expiration.is_expired(&env.block) {
        return Err(ContractError::Expired { expiration });
    }

    PAUSED.save(deps.storage, &true)?;
    Ok(Response::new().add_attributes(vec![attr("action", "pause"), attr("paused", "true")]))
}

pub fn execute_resume(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    new_expiration: Option<Expiration>,
) -> Result<Response, ContractError> {
    // authorize owner
    let cfg = CONFIG.load(deps.storage)?;
    let owner = cfg.owner.ok_or(ContractError::Unauthorized {})?;
    if info.sender != owner {
        return Err(ContractError::Unauthorized {});
    }

    let start = START.may_load(deps.storage)?;
    if let Some(start) = start {
        if !start.is_triggered(&env.block) {
            return Err(ContractError::NotBegun { start });
        }
    }

    let expiration = STAGE_EXPIRATION.load(deps.storage)?;
    if expiration.is_expired(&env.block) {
        return Err(ContractError::Expired { expiration });
    }

    let is_paused = PAUSED.load(deps.storage)?;
    if !is_paused {
        return Err(ContractError::NotPaused {});
    }

    if let Some(new_expiration) = new_expiration {
        if new_expiration.is_expired(&env.block) {
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
        owner: cfg.owner.map(|o| o.to_string()),
        credits_address: cfg.credits_address.map(|addr| addr.to_string()),
        reserve_address: cfg.reserve_address.map(|addr| addr.to_string()),
        neutron_denom: cfg.neutron_denom,
    })
}

pub fn query_merkle_root(deps: Deps) -> StdResult<MerkleRootResponse> {
    let merkle_root = MERKLE_ROOT.load(deps.storage)?;
    let expiration = STAGE_EXPIRATION.load(deps.storage)?;
    let start = START.may_load(deps.storage)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::SignatureInfo;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info,
    };
    use cosmwasm_std::{
        from_binary, from_slice, Addr, Attribute, BlockInfo, Coin, CosmosMsg, Empty, SubMsg,
        Timestamp, WasmMsg,
    };
    use cw20::MinterResponse;
    use cw_multi_test::{App, BankKeeper, Contract, ContractWrapper, Executor};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    use crate::contract::{execute, instantiate, query};
    use crate::msg::{ExecuteMsg, InstantiateMsg};

    fn mock_app() -> App {
        App::default()
    }

    pub fn contract_merkle_airdrop() -> Box<dyn Contract<Empty>> {
        Box::new(ContractWrapper::new(execute, instantiate, query))
    }

    pub fn contract_cw20() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            cw20_base::contract::execute,
            cw20_base::contract::instantiate,
            cw20_base::contract::query,
        );
        Box::new(contract)
    }

    #[test]
    fn proper_instantiation() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            owner: Some("owner0000".to_string()),
            credits_address: Some("credits0000".to_string()),
            reserve_address: Some("reserve0000".to_string()),
            neutron_denom: "untrn".to_string(),
        };

        let env = mock_env();
        let info = mock_info("addr0000", &[]);

        // we can just call .unwrap() to assert this was a success
        let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // it worked, let's query the state
        let res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_binary(&res).unwrap();
        assert_eq!("owner0000", config.owner.unwrap().as_str());
        assert_eq!("credits0000", config.credits_address.unwrap());
        assert_eq!("reserve0000", config.reserve_address.unwrap());
        assert_eq!("untrn", config.neutron_denom);
    }

    #[test]
    fn update_config() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            owner: None,
            credits_address: Some(String::from("credits0000")),
            reserve_address: Some(String::from("reserve0000")),
            neutron_denom: "untrn".to_string(),
        };

        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

        // update owner
        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let msg = ExecuteMsg::UpdateConfig {
            new_owner: Some("owner0001".to_string()),
            new_credits_address: None,
            new_reserve_address: None,
            new_neutron_denom: None,
        };

        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_binary(&res).unwrap();
        assert_eq!("owner0001", config.owner.unwrap().as_str());
        assert_eq!("credits0000", config.credits_address.unwrap());
        assert_eq!("untrn", config.neutron_denom);

        // Unauthorized err
        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let msg = ExecuteMsg::UpdateConfig {
            new_owner: None,
            new_credits_address: None,
            new_reserve_address: None,
            new_neutron_denom: None,
        };

        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(res, ContractError::Unauthorized {});

        // update credits and reserve addresses
        let env = mock_env();
        let info = mock_info("owner0001", &[]);
        let msg = ExecuteMsg::UpdateConfig {
            new_owner: Some("owner0001".to_string()),
            new_credits_address: Some("credits0001".to_string()),
            new_reserve_address: Some("reserve0001".to_string()),
            new_neutron_denom: None,
        };

        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_binary(&res).unwrap();
        assert_eq!("owner0001", config.owner.unwrap().as_str());
        assert_eq!("credits0001", config.credits_address.unwrap());
        assert_eq!("reserve0001", config.reserve_address.unwrap());
        assert_eq!("untrn", config.neutron_denom);

        // update neutron denom
        let env = mock_env();
        let info = mock_info("owner0001", &[]);
        let msg = ExecuteMsg::UpdateConfig {
            new_owner: Some("owner0001".to_string()),
            new_credits_address: None,
            new_reserve_address: None,
            new_neutron_denom: Some("ujunox".to_string()),
        };

        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_binary(&res).unwrap();
        assert_eq!("owner0001", config.owner.unwrap().as_str());
        assert_eq!("credits0001", config.credits_address.unwrap());
        assert_eq!("reserve0001", config.reserve_address.unwrap());
        assert_eq!("ujunox", config.neutron_denom);
    }

    #[test]
    fn register_merkle_root() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            owner: Some("owner0000".to_string()),
            credits_address: Some("credits0000".to_string()),
            reserve_address: Some("reserve0000".to_string()),
            neutron_denom: "untrn".to_string(),
        };

        let env = mock_env();
        let info = mock_info("addr0000", &[]);
        let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

        // register new merkle root
        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let msg = ExecuteMsg::RegisterMerkleRoot {
            merkle_root: "634de21cde1044f41d90373733b0f0fb1c1c71f9652b905cdf159e73c4cf0d37"
                .to_string(),
            expiration: None,
            start: None,
            total_amount: None,
            hrp: None,
        };

        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(
            res.attributes,
            vec![
                attr("action", "register_merkle_root"),
                attr(
                    "merkle_root",
                    "634de21cde1044f41d90373733b0f0fb1c1c71f9652b905cdf159e73c4cf0d37",
                ),
                attr("total_amount", "0"),
            ]
        );

        let res = query(deps.as_ref(), env, QueryMsg::MerkleRoot {}).unwrap();
        let merkle_root: MerkleRootResponse = from_binary(&res).unwrap();
        assert_eq!(
            "634de21cde1044f41d90373733b0f0fb1c1c71f9652b905cdf159e73c4cf0d37".to_string(),
            merkle_root.merkle_root
        );
    }

    const TEST_DATA_1: &[u8] = include_bytes!("../testdata/airdrop_test_data.json");

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    struct Encoded {
        account: String,
        amount: Uint128,
        root: String,
        proofs: Vec<String>,
        signed_msg: Option<SignatureInfo>,
        hrp: Option<String>,
    }

    #[test]
    fn cant_claim_without_credits_address() {
        let mut deps = mock_dependencies();
        let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();

        let msg = InstantiateMsg {
            owner: Some("owner0000".to_string()),
            credits_address: None,
            reserve_address: Some("reserve0000".to_string()),
            neutron_denom: "untrn".to_string(),
        };

        let env = mock_env();
        let info = mock_info("addr0000", &[]);
        let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let msg = ExecuteMsg::RegisterMerkleRoot {
            merkle_root: test_data.root,
            expiration: None,
            start: None,
            total_amount: None,
            hrp: None,
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        let msg = ExecuteMsg::Claim {
            amount: test_data.amount,
            proof: test_data.proofs,
            sig_info: None,
        };

        let env = mock_env();
        let info = mock_info(test_data.account.as_str(), &[]);
        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(res, ContractError::CreditsAddress {});
    }

    #[test]
    fn claim() {
        let mut deps = mock_dependencies();
        let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();

        let msg = InstantiateMsg {
            owner: Some("owner0000".to_string()),
            credits_address: Some("credits0000".to_string()),
            reserve_address: Some("reserve0000".to_string()),
            neutron_denom: "untrn".to_string(),
        };

        let env = mock_env();
        let info = mock_info("addr0000", &[]);
        let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let msg = ExecuteMsg::RegisterMerkleRoot {
            merkle_root: test_data.root,
            expiration: Some(Expiration::AtTime(env.block.time.plus_seconds(10_000))),
            start: None,
            total_amount: None,
            hrp: None,
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        let msg = ExecuteMsg::Claim {
            amount: test_data.amount,
            proof: test_data.proofs,
            sig_info: None,
        };

        let env = mock_env();
        let info = mock_info(test_data.account.as_str(), &[]);
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
        let expected = vec![
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "credits0000".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: test_data.account.clone(),
                    amount: test_data.amount,
                })
                .unwrap(),
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "credits0000".to_string(),
                msg: to_binary(&AddVesting {
                    address: test_data.account.clone(),
                    amount: test_data.amount,
                    start_time: env.block.time.plus_seconds(10_000).seconds(),
                    duration: VESTING_DURATION_SECONDS,
                })
                .unwrap(),
                funds: vec![],
            })),
        ];
        assert_eq!(res.messages, expected);

        assert_eq!(
            res.attributes,
            vec![
                attr("action", "claim"),
                attr("address", test_data.account.clone()),
                attr("amount", test_data.amount),
            ]
        );

        // Check total claimed
        assert_eq!(
            from_binary::<TotalClaimedResponse>(
                &query(deps.as_ref(), env.clone(), QueryMsg::TotalClaimed {},).unwrap()
            )
            .unwrap()
            .total_claimed,
            test_data.amount
        );

        // Check address is claimed
        assert!(
            from_binary::<IsClaimedResponse>(
                &query(
                    deps.as_ref(),
                    env.clone(),
                    QueryMsg::IsClaimed {
                        address: test_data.account,
                    },
                )
                .unwrap()
            )
            .unwrap()
            .is_claimed
        );

        // check error on double claim
        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(res, ContractError::Claimed {});
    }

    const TEST_DATA_1_MULTI: &[u8] = include_bytes!("../testdata/airdrop_test_multi_data.json");

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    struct Proof {
        account: String,
        amount: Uint128,
        proofs: Vec<String>,
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
    struct MultipleData {
        total_claimed_amount: Uint128,
        root: String,
        accounts: Vec<Proof>,
    }

    #[test]
    fn multiple_claim() {
        // Run test 1
        let mut deps = mock_dependencies();
        let test_data: MultipleData = from_slice(TEST_DATA_1_MULTI).unwrap();

        let msg = InstantiateMsg {
            owner: Some("owner0000".to_string()),
            credits_address: Some("credits0000".to_string()),
            reserve_address: Some("reserve0000".to_string()),
            neutron_denom: "untrn".to_string(),
        };

        let env = mock_env();
        let info = mock_info("addr0000", &[]);
        let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let msg = ExecuteMsg::RegisterMerkleRoot {
            merkle_root: test_data.root,
            expiration: Some(Expiration::AtTime(env.block.time.plus_seconds(10_000))),
            start: None,
            total_amount: None,
            hrp: None,
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // Loop accounts and claim
        for account in test_data.accounts.iter() {
            let msg = ExecuteMsg::Claim {
                amount: account.amount,
                proof: account.proofs.clone(),
                sig_info: None,
            };

            let env = mock_env();
            let info = mock_info(account.account.as_str(), &[]);
            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
            let expected = vec![
                SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: "credits0000".to_string(),
                    funds: vec![],
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: account.account.clone(),
                        amount: account.amount,
                    })
                    .unwrap(),
                })),
                SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: "credits0000".to_string(),
                    msg: to_binary(&AddVesting {
                        address: account.account.clone(),
                        amount: account.amount,
                        start_time: env.block.time.plus_seconds(10_000).seconds(),
                        duration: VESTING_DURATION_SECONDS,
                    })
                    .unwrap(),
                    funds: vec![],
                })),
            ];
            assert_eq!(res.messages, expected);

            assert_eq!(
                res.attributes,
                vec![
                    attr("action", "claim"),
                    attr("address", account.account.clone()),
                    attr("amount", account.amount),
                ]
            );
        }

        // Check total claimed
        let env = mock_env();
        assert_eq!(
            from_binary::<TotalClaimedResponse>(
                &query(deps.as_ref(), env, QueryMsg::TotalClaimed {}).unwrap()
            )
            .unwrap()
            .total_claimed,
            test_data.total_claimed_amount
        );
    }

    // Check expiration. Chain height in tests is 12345
    #[test]
    fn expiration() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            owner: Some("owner0000".to_string()),
            credits_address: Some("credits0000".to_string()),
            reserve_address: Some("reserve0000".to_string()),
            neutron_denom: "untrn".to_string(),
        };

        let env = mock_env();
        let info = mock_info("addr0000", &[]);
        let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

        // can register merkle root
        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let msg = ExecuteMsg::RegisterMerkleRoot {
            merkle_root: "5d4f48f147cb6cb742b376dce5626b2a036f69faec10cd73631c791780e150fc"
                .to_string(),
            expiration: Some(Expiration::AtHeight(100)),
            start: None,
            total_amount: None,
            hrp: None,
        };
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // can't claim expired
        let msg = ExecuteMsg::Claim {
            amount: Uint128::new(5),
            proof: vec![],
            sig_info: None,
        };

        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(
            res,
            ContractError::Expired {
                expiration: Expiration::AtHeight(100),
            }
        )
    }

    #[test]
    fn cant_withdraw_all_without_reserve_address() {
        let mut deps = mock_dependencies_with_balance(&[Coin {
            denom: "ujunox".to_string(),
            amount: Uint128::new(10000),
        }]);
        let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();

        let msg = InstantiateMsg {
            owner: Some("owner0000".to_string()),
            credits_address: Some("credits0000".to_string()),
            reserve_address: None,
            neutron_denom: "untrn".to_string(),
        };

        let mut env = mock_env();
        let info = mock_info("addr0000", &[]);
        let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        let info = mock_info("owner0000", &[]);
        let msg = ExecuteMsg::RegisterMerkleRoot {
            merkle_root: test_data.root,
            expiration: Some(Expiration::AtTime(env.block.time.plus_seconds(100))),
            start: None,
            total_amount: Some(Uint128::new(10000)),
            hrp: None,
        };
        execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        // makes withdraw_all available
        env.block.time = env.block.time.plus_seconds(VESTING_DURATION_SECONDS + 101);

        let info = mock_info("owner0000", &[]);
        let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawAll {}).unwrap_err();
        assert_eq!(res, ContractError::ReserveAddress {});
    }

    #[test]
    fn withdraw_all() {
        let mut router = mock_app();
        router
            .init_modules(|router, _api, storage| {
                router.bank = BankKeeper::new();
                router.bank.init_balance(
                    storage,
                    &Addr::unchecked("neutron_holder"),
                    vec![coin(10000, "untrn")],
                )
            })
            .unwrap();
        let block_info = BlockInfo {
            height: 12345,
            time: Timestamp::from_seconds(12345),
            chain_id: "testing".to_string(),
        };
        router.set_block(block_info);

        let merkle_airdrop_id = router.store_code(contract_merkle_airdrop());
        let cw20_id = router.store_code(contract_cw20());

        let cw20_instantiate_msg = cw20_base::msg::InstantiateMsg {
            name: "Airdrop Token".parse().unwrap(),
            symbol: "ADT".parse().unwrap(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: "minter0000".to_string(),
                cap: None,
            }),
            marketing: None,
        };
        let cw20_addr = router
            .instantiate_contract(
                cw20_id,
                Addr::unchecked("minter0000".to_string()),
                &cw20_instantiate_msg,
                &[],
                "Airdrop Test",
                None,
            )
            .unwrap();

        let merkle_airdrop_instantiate_msg = InstantiateMsg {
            owner: Some("owner0000".to_string()),
            credits_address: Some(cw20_addr.to_string()),
            reserve_address: Some("reserve0000".to_string()),
            neutron_denom: "untrn".to_string(),
        };

        let merkle_airdrop_addr = router
            .instantiate_contract(
                merkle_airdrop_id,
                Addr::unchecked("owner0000".to_string()),
                &merkle_airdrop_instantiate_msg,
                &[],
                "Airdrop Test",
                None,
            )
            .unwrap();

        let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();
        //register airdrop
        let register_msg = ExecuteMsg::RegisterMerkleRoot {
            merkle_root: test_data.root,
            expiration: Some(Expiration::AtTime(
                router.block_info().time.plus_seconds(10),
            )),
            start: None,
            total_amount: Some(Uint128::new(10000)),
            hrp: None,
        };
        router
            .execute_contract(
                Addr::unchecked("owner0000".to_string()),
                merkle_airdrop_addr.clone(),
                &register_msg,
                &[],
            )
            .unwrap();

        //mint cw20 tokens
        let mint_recipient = Addr::unchecked(merkle_airdrop_addr.to_string());
        let mint_amount = Uint128::new(10000);
        let cw20_mint_msg = cw20_base::msg::ExecuteMsg::Mint {
            recipient: mint_recipient.to_string(),
            amount: mint_amount,
        };
        //execute mint
        router
            .execute_contract(
                Addr::unchecked("minter0000".to_string()),
                cw20_addr.clone(),
                &cw20_mint_msg,
                &[],
            )
            .unwrap();

        //check airdrop contract balance
        let response: BalanceResponse = router
            .wrap()
            .query_wasm_smart(
                &cw20_addr,
                &cw20_base::msg::QueryMsg::Balance {
                    address: mint_recipient.to_string(),
                },
            )
            .unwrap();
        assert_eq!(Uint128::new(10000), response.balance);
        //withdraw before expiration
        let withdraw_msg = ExecuteMsg::WithdrawAll {};
        let err = router
            .execute_contract(
                Addr::unchecked("owner0000".to_string()),
                merkle_airdrop_addr.clone(),
                &withdraw_msg,
                &[],
            )
            .unwrap_err()
            .downcast::<ContractError>()
            .unwrap();
        assert_eq!(
            err,
            ContractError::Std(StdError::generic_err(
                "withdraw_all only works 3 months after the end of the event"
            ))
        );

        //update block height
        let block_info = BlockInfo {
            height: 12501,
            time: Timestamp::from_seconds(12501).plus_seconds(VESTING_DURATION_SECONDS),
            chain_id: "testing".to_string(),
        };
        router.set_block(block_info);

        // We expect credits contract to send 10000 untrn to merkle airdrop contract
        // during processing of this message, so we mimic this behaviour manually
        router
            .send_tokens(
                Addr::unchecked("neutron_holder"),
                merkle_airdrop_addr.clone(),
                &[coin(10000, "untrn")],
            )
            .unwrap();

        // withdraw after expiration
        let partial_withdraw_msg = ExecuteMsg::WithdrawAll {};
        let res = router
            .execute_contract(
                Addr::unchecked("owner0000".to_string()),
                merkle_airdrop_addr.clone(),
                &partial_withdraw_msg,
                &[],
            )
            .unwrap();

        assert_eq!(
            res.events[1].attributes,
            vec![
                Attribute {
                    key: "_contract_addr".to_string(),
                    value: "contract1".to_string()
                },
                Attribute {
                    key: "action".to_string(),
                    value: "withdraw_all".to_string()
                },
                Attribute {
                    key: "address".to_string(),
                    value: "owner0000".to_string()
                },
                Attribute {
                    key: "amount".to_string(),
                    value: "10000".to_string()
                },
                Attribute {
                    key: "recipient".to_string(),
                    value: "reserve0000".to_string()
                }
            ]
        );
        //check airdrop contract cw20 balance
        let new_balance: BalanceResponse = router
            .wrap()
            .query_wasm_smart(
                &cw20_addr,
                &cw20_base::msg::QueryMsg::Balance {
                    address: mint_recipient.to_string(),
                },
            )
            .unwrap();
        assert_eq!(Uint128::zero(), new_balance.balance);
        //check airdrop contract balance
        let recipient_balance = router
            .wrap()
            .query_balance(merkle_airdrop_addr.to_string(), "untrn")
            .unwrap();
        assert_eq!(Uint128::new(0), recipient_balance.amount);
        //check reserve contract balance
        let recipient_balance = router.wrap().query_balance("reserve0000", "untrn").unwrap();
        assert_eq!(Uint128::new(10000), recipient_balance.amount);
    }

    #[test]
    fn starts() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            owner: Some("owner0000".to_string()),
            credits_address: Some("credits0000".to_string()),
            reserve_address: Some("reserve0000".to_string()),
            neutron_denom: "untrn".to_string(),
        };

        let env = mock_env();
        let info = mock_info("addr0000", &[]);
        let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

        // can register merkle root
        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let msg = ExecuteMsg::RegisterMerkleRoot {
            merkle_root: "5d4f48f147cb6cb742b376dce5626b2a036f69faec10cd73631c791780e150fc"
                .to_string(),
            expiration: None,
            start: Some(Scheduled::AtHeight(200_000)),
            total_amount: None,
            hrp: None,
        };
        execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        // can't claim, airdrop has not started yet
        let msg = ExecuteMsg::Claim {
            amount: Uint128::new(5),
            proof: vec![],
            sig_info: None,
        };

        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(
            res,
            ContractError::NotBegun {
                start: Scheduled::AtHeight(200_000),
            }
        )
    }

    #[test]
    fn owner_freeze() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {
            owner: Some("owner0000".to_string()),
            credits_address: Some("credits0000".to_string()),
            reserve_address: Some("reserve0000".to_string()),
            neutron_denom: "untrn".to_string(),
        };

        let env = mock_env();
        let info = mock_info("addr0000", &[]);
        let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

        // can register merkle root
        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let msg = ExecuteMsg::RegisterMerkleRoot {
            merkle_root: "5d4f48f147cb6cb742b376dce5626b2a036f69faec10cd73631c791780e150fc"
                .to_string(),
            expiration: None,
            start: None,
            total_amount: None,
            hrp: None,
        };
        let _res = execute(deps.as_mut(), env, info, msg).unwrap();

        // can update owner
        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let msg = ExecuteMsg::UpdateConfig {
            new_owner: Some("owner0001".to_string()),
            new_credits_address: None,
            new_reserve_address: None,
            new_neutron_denom: None,
        };

        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // freeze contract
        let env = mock_env();
        let info = mock_info("owner0001", &[]);
        let msg = ExecuteMsg::UpdateConfig {
            new_owner: None,
            new_credits_address: None,
            new_reserve_address: None,
            new_neutron_denom: None,
        };

        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // cannot register new drop
        let env = mock_env();
        let info = mock_info("owner0001", &[]);
        let msg = ExecuteMsg::RegisterMerkleRoot {
            merkle_root: "ebaa83c7eaf7467c378d2f37b5e46752d904d2d17acd380b24b02e3b398b3e5a"
                .to_string(),
            expiration: None,
            start: None,
            total_amount: None,
            hrp: None,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(res, ContractError::Unauthorized {});

        // cannot update config
        let env = mock_env();
        let info = mock_info("owner0001", &[]);
        let msg = ExecuteMsg::UpdateConfig {
            new_owner: Some("owner0001".to_string()),
            new_credits_address: None,
            new_reserve_address: None,
            new_neutron_denom: None,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(res, ContractError::Unauthorized {});
    }

    mod external_sig {
        use super::*;
        use crate::msg::SignatureInfo;
        use cw_utils::Expiration::AtHeight;

        const TEST_DATA_EXTERNAL_SIG: &[u8] =
            include_bytes!("../testdata/airdrop_external_sig_test_data.json");

        #[test]
        fn test_cosmos_sig_verify() {
            let deps = mock_dependencies();
            let signature_raw = Binary::from_base64("eyJwdWJfa2V5IjoiQWhOZ2UxV01aVXl1ODZ5VGx5ZWpEdVVxUFZTdURONUJhQzArdkw4b3RkSnYiLCJzaWduYXR1cmUiOiJQY1FPczhXSDVPMndXL3Z3ZzZBTElqaW9VNGorMUZYNTZKU1R1MzdIb2lGbThJck5aem5HaGlIRFV1R1VTUmlhVnZRZ2s4Q0tURmNyeVpuYjZLNVhyQT09In0=");

            let sig = SignatureInfo {
                claim_msg: Binary::from_base64("eyJhY2NvdW50X251bWJlciI6IjExMjM2IiwiY2hhaW5faWQiOiJwaXNjby0xIiwiZmVlIjp7ImFtb3VudCI6W3siYW1vdW50IjoiMTU4MTIiLCJkZW5vbSI6InVsdW5hIn1dLCJnYXMiOiIxMDU0MDcifSwibWVtbyI6Imp1bm8xMHMydXU5MjY0ZWhscWw1ZnB5cmg5dW5kbmw1bmxhdzYzdGQwaGgiLCJtc2dzIjpbeyJ0eXBlIjoiY29zbW9zLXNkay9Nc2dTZW5kIiwidmFsdWUiOnsiYW1vdW50IjpbeyJhbW91bnQiOiIxIiwiZGVub20iOiJ1bHVuYSJ9XSwiZnJvbV9hZGRyZXNzIjoidGVycmExZmV6NTlzdjh1cjk3MzRmZnJwdndwY2phZHg3bjB4Nno2eHdwN3oiLCJ0b19hZGRyZXNzIjoidGVycmExZmV6NTlzdjh1cjk3MzRmZnJwdndwY2phZHg3bjB4Nno2eHdwN3oifX1dLCJzZXF1ZW5jZSI6IjAifQ==").unwrap(),
                signature: signature_raw.unwrap(),
            };
            let cosmos_signature: CosmosSignature = from_binary(&sig.signature).unwrap();
            let res = cosmos_signature
                .verify(deps.as_ref(), &sig.claim_msg)
                .unwrap();
            assert!(res);
        }

        #[test]
        fn test_derive_addr_from_pubkey() {
            let test_data: Encoded = from_slice(TEST_DATA_EXTERNAL_SIG).unwrap();
            let cosmos_signature: CosmosSignature =
                from_binary(&test_data.signed_msg.unwrap().signature).unwrap();
            let derived_addr = cosmos_signature
                .derive_addr_from_pubkey(&test_data.hrp.unwrap())
                .unwrap();
            assert_eq!(test_data.account, derived_addr);
        }

        #[test]
        fn claim_with_external_sigs() {
            let mut deps = mock_dependencies_with_balance(&[Coin {
                denom: "ujunox".to_string(),
                amount: Uint128::new(1234567),
            }]);
            let test_data: Encoded = from_slice(TEST_DATA_EXTERNAL_SIG).unwrap();
            let claim_addr = test_data
                .signed_msg
                .clone()
                .unwrap()
                .extract_addr()
                .unwrap();

            let msg = InstantiateMsg {
                owner: Some("owner0000".to_string()),
                credits_address: Some("credits0000".to_string()),
                reserve_address: Some("reserve0000".to_string()),
                neutron_denom: "untrn".to_string(),
            };

            let env = mock_env();
            let info = mock_info("addr0000", &[]);
            let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

            let env = mock_env();
            let info = mock_info("owner0000", &[]);
            let msg = ExecuteMsg::RegisterMerkleRoot {
                merkle_root: test_data.root,
                expiration: Some(Expiration::AtTime(env.block.time.plus_seconds(10_000))),
                start: None,
                total_amount: None,
                hrp: Some(test_data.hrp.unwrap()),
            };
            let _res = execute(deps.as_mut(), env, info, msg).unwrap();

            // cant claim without sig, info.sender is not present in the root
            let msg = ExecuteMsg::Claim {
                amount: test_data.amount,
                proof: test_data.proofs.clone(),
                sig_info: None,
            };

            let env = mock_env();
            let info = mock_info(claim_addr.as_str(), &[]);
            let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
            assert_eq!(res, ContractError::VerificationFailed {});

            // can claim with sig
            let msg = ExecuteMsg::Claim {
                amount: test_data.amount,
                proof: test_data.proofs,
                sig_info: test_data.signed_msg,
            };

            let env = mock_env();
            let info = mock_info(claim_addr.as_str(), &[]);
            let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
            let expected = vec![
                SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: "credits0000".to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: claim_addr.to_string(),
                        amount: test_data.amount,
                    })
                    .unwrap(),
                    funds: vec![],
                })),
                SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: "credits0000".to_string(),
                    msg: to_binary(&AddVesting {
                        address: claim_addr.clone(),
                        amount: test_data.amount,
                        start_time: env.block.time.plus_seconds(10_000).seconds(),
                        duration: VESTING_DURATION_SECONDS,
                    })
                    .unwrap(),
                    funds: vec![],
                })),
            ];

            assert_eq!(res.messages, expected);
            assert_eq!(
                res.attributes,
                vec![
                    attr("action", "claim"),
                    attr("address", claim_addr.clone()),
                    attr("amount", test_data.amount),
                ]
            );

            // Check total claimed
            assert_eq!(
                from_binary::<TotalClaimedResponse>(
                    &query(deps.as_ref(), env.clone(), QueryMsg::TotalClaimed {},).unwrap()
                )
                .unwrap()
                .total_claimed,
                test_data.amount
            );

            // Check address is claimed
            assert!(
                from_binary::<IsClaimedResponse>(
                    &query(
                        deps.as_ref(),
                        env.clone(),
                        QueryMsg::IsClaimed {
                            address: test_data.account.clone(),
                        },
                    )
                    .unwrap()
                )
                .unwrap()
                .is_claimed
            );

            // check error on double claim
            let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
            assert_eq!(res, ContractError::Claimed {});

            // query map

            let map = from_binary::<AccountMapResponse>(
                &query(
                    deps.as_ref(),
                    env,
                    QueryMsg::AccountMap {
                        external_address: test_data.account.clone(),
                    },
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(map.external_address, test_data.account);
            assert_eq!(map.host_address, claim_addr);
        }

        #[test]
        fn claim_paused_airdrop() {
            let mut deps = mock_dependencies_with_balance(&[Coin {
                denom: "ujunox".to_string(),
                amount: Uint128::new(1234567),
            }]);
            let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();

            let msg = InstantiateMsg {
                owner: Some("owner0000".to_string()),
                credits_address: Some("credits0000".to_string()),
                reserve_address: Some("reserve0000".to_string()),
                neutron_denom: "untrn".to_string(),
            };

            let env = mock_env();
            let info = mock_info("addr0000", &[]);
            let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

            let env = mock_env();
            let info = mock_info("owner0000", &[]);
            let msg = ExecuteMsg::RegisterMerkleRoot {
                merkle_root: test_data.root,
                expiration: None,
                start: None,
                total_amount: None,
                hrp: None,
            };
            let _res = execute(deps.as_mut(), env, info, msg).unwrap();

            let pause_msg = ExecuteMsg::Pause {};
            let env = mock_env();
            let info = mock_info("owner0000", &[]);
            let result = execute(deps.as_mut(), env, info, pause_msg).unwrap();

            assert_eq!(
                result.attributes,
                vec![attr("action", "pause"), attr("paused", "true"),]
            );

            let msg = ExecuteMsg::Claim {
                amount: test_data.amount,
                proof: test_data.proofs.clone(),
                sig_info: None,
            };

            let env = mock_env();
            let info = mock_info(test_data.account.as_str(), &[]);
            let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();

            assert_eq!(res, ContractError::Paused {});

            let resume_msg = ExecuteMsg::Resume {
                new_expiration: Some(Expiration::AtTime(env.block.time.plus_seconds(5_000))),
            };
            let env = mock_env();
            let info = mock_info("owner0000", &[]);
            let result = execute(deps.as_mut(), env, info, resume_msg).unwrap();

            assert_eq!(
                result.attributes,
                vec![attr("action", "resume"), attr("paused", "false"),]
            );
            let msg = ExecuteMsg::Claim {
                amount: test_data.amount,
                proof: test_data.proofs.clone(),
                sig_info: None,
            };
            let env = mock_env();
            let info = mock_info(test_data.account.as_str(), &[]);
            let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
            let expected = vec![
                SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: "credits0000".to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: test_data.account.to_string(),
                        amount: test_data.amount,
                    })
                    .unwrap(),
                    funds: vec![],
                })),
                SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: "credits0000".to_string(),
                    msg: to_binary(&AddVesting {
                        address: test_data.account.clone(),
                        amount: test_data.amount,
                        start_time: env.block.time.plus_seconds(5_000).seconds(),
                        duration: VESTING_DURATION_SECONDS,
                    })
                    .unwrap(),
                    funds: vec![],
                })),
            ];
            assert_eq!(res.messages, expected);

            assert_eq!(
                res.attributes,
                vec![
                    attr("action", "claim"),
                    attr("address", test_data.account.clone()),
                    attr("amount", test_data.amount),
                ]
            );
        }

        #[test]
        fn withdraw_all_paused_airdrop() {
            let mut router = mock_app();
            router
                .init_modules(|router, _api, storage| {
                    router.bank = BankKeeper::new();
                    router.bank.init_balance(
                        storage,
                        &Addr::unchecked("neutron_holder"),
                        vec![coin(10000, "untrn")],
                    )
                })
                .unwrap();
            let block_info = BlockInfo {
                height: 12345,
                time: Timestamp::from_seconds(12345),
                chain_id: "testing".to_string(),
            };
            router.set_block(block_info);

            let merkle_airdrop_id = router.store_code(contract_merkle_airdrop());
            let cw20_id = router.store_code(contract_cw20());

            let cw20_instantiate_msg = cw20_base::msg::InstantiateMsg {
                name: "Airdrop Token".parse().unwrap(),
                symbol: "ADT".parse().unwrap(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: "minter0000".to_string(),
                    cap: None,
                }),
                marketing: None,
            };
            let cw20_addr = router
                .instantiate_contract(
                    cw20_id,
                    Addr::unchecked("minter0000".to_string()),
                    &cw20_instantiate_msg,
                    &[],
                    "Airdrop Test",
                    None,
                )
                .unwrap();

            let merkle_airdrop_instantiate_msg = InstantiateMsg {
                owner: Some("owner0000".to_string()),
                credits_address: Some(cw20_addr.to_string()),
                reserve_address: Some("reserve0000".to_string()),
                neutron_denom: "untrn".to_string(),
            };

            let merkle_airdrop_addr = router
                .instantiate_contract(
                    merkle_airdrop_id,
                    Addr::unchecked("owner0000".to_string()),
                    &merkle_airdrop_instantiate_msg,
                    &[],
                    "Airdrop Test",
                    None,
                )
                .unwrap();

            let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();
            //register airdrop
            let register_msg = ExecuteMsg::RegisterMerkleRoot {
                merkle_root: test_data.root,
                expiration: Some(AtHeight(12500)),
                start: None,
                total_amount: Some(Uint128::new(10000)),
                hrp: None,
            };
            router
                .execute_contract(
                    Addr::unchecked("owner0000".to_string()),
                    merkle_airdrop_addr.clone(),
                    &register_msg,
                    &[],
                )
                .unwrap();

            //mint cw20 tokens
            let mint_recipient = Addr::unchecked(merkle_airdrop_addr.to_string());
            let mint_amount = Uint128::new(10000);
            let cw20_mint_msg = cw20_base::msg::ExecuteMsg::Mint {
                recipient: mint_recipient.to_string(),
                amount: mint_amount,
            };
            //execute mint
            router
                .execute_contract(
                    Addr::unchecked("minter0000".to_string()),
                    cw20_addr.clone(),
                    &cw20_mint_msg,
                    &[],
                )
                .unwrap();

            //check airdrop contract balance
            let response: BalanceResponse = router
                .wrap()
                .query_wasm_smart(
                    &cw20_addr,
                    &cw20_base::msg::QueryMsg::Balance {
                        address: mint_recipient.to_string(),
                    },
                )
                .unwrap();
            assert_eq!(Uint128::new(10000), response.balance);

            // Can't withdraw before pause
            let msg = ExecuteMsg::WithdrawAll {};
            router
                .execute_contract(
                    Addr::unchecked("owner0000"),
                    merkle_airdrop_addr.clone(),
                    &msg,
                    &[],
                )
                .unwrap_err()
                .downcast::<ContractError>()
                .unwrap();

            let pause_msg = ExecuteMsg::Pause {};
            let result = router
                .execute_contract(
                    Addr::unchecked("owner0000"),
                    merkle_airdrop_addr.clone(),
                    &pause_msg,
                    &[],
                )
                .unwrap()
                .events
                .into_iter()
                .find(|event| event.ty == "wasm")
                .unwrap()
                .attributes
                .into_iter()
                .filter(|attribute| ["action", "paused"].contains(&attribute.key.as_str()))
                .collect::<Vec<_>>();
            assert_eq!(
                result,
                vec![attr("action", "pause"), attr("paused", "true")]
            );

            // We expect credits contract to send 10000 untrn to merkle airdrop contract
            // during processing of this message, so we mimic this behaviour manually
            router
                .send_tokens(
                    Addr::unchecked("neutron_holder"),
                    merkle_airdrop_addr.clone(),
                    &[coin(10000, "untrn")],
                )
                .unwrap();

            //Withdraw when paused
            let msg = ExecuteMsg::WithdrawAll {};
            let res = router
                .execute_contract(
                    Addr::unchecked("owner0000"),
                    merkle_airdrop_addr.clone(),
                    &msg,
                    &[],
                )
                .unwrap();

            assert_eq!(
                res.events[1].attributes,
                vec![
                    Attribute {
                        key: "_contract_addr".to_string(),
                        value: "contract1".to_string()
                    },
                    Attribute {
                        key: "action".to_string(),
                        value: "withdraw_all".to_string()
                    },
                    Attribute {
                        key: "address".to_string(),
                        value: "owner0000".to_string()
                    },
                    Attribute {
                        key: "amount".to_string(),
                        value: "10000".to_string()
                    },
                    Attribute {
                        key: "recipient".to_string(),
                        value: "reserve0000".to_string()
                    }
                ]
            );
            //check airdrop contract cw20 balance
            let new_balance: BalanceResponse = router
                .wrap()
                .query_wasm_smart(
                    &cw20_addr,
                    &cw20_base::msg::QueryMsg::Balance {
                        address: mint_recipient.to_string(),
                    },
                )
                .unwrap();
            assert_eq!(Uint128::zero(), new_balance.balance);
            //check airdrop contract balance
            let recipient_balance = router
                .wrap()
                .query_balance(merkle_airdrop_addr.to_string(), "untrn")
                .unwrap();
            assert_eq!(Uint128::new(0), recipient_balance.amount);
            //check reserve contract balance
            let recipient_balance = router.wrap().query_balance("reserve0000", "untrn").unwrap();
            assert_eq!(Uint128::new(10000), recipient_balance.amount);
        }
    }
}
