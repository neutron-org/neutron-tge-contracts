use crate::error::ContractError;
use crate::state::{PriceFeedRate, CONFIG, ENDPOINT, ERROR, LAST_UPDATE, RATES};
use astroport_periphery::pricefeed::{
    Config, ExecuteMsg, InstantiateMsg, QueryMsg, UpdateConfigMsg,
};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Empty, Env, IbcMsg, IbcTimeout, MessageInfo, Response,
    StdResult,
};
use cw2::set_contract_version;
use obi::OBIEncode as OBIEncodeEnc;

use cw_band::OracleRequestPacketData;

#[derive(OBIEncodeEnc)]
pub struct Input {
    pub symbols: Vec<String>,
    pub multiplier: u64,
}

// Version info for migration
const CONTRACT_NAME: &str = "crates.io:band-ibc-price-feed";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_MAX_UPDATE_INTERVAL: u64 = 120u64;

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
        .map_or(Ok(info.sender), |a| deps.api.addr_validate(&a))?;
    let max_update_interval = msg
        .max_update_interval
        .unwrap_or(DEFAULT_MAX_UPDATE_INTERVAL);

    CONFIG.save(
        deps.storage,
        &Config {
            owner,
            client_id: msg.client_id,
            oracle_script_id: msg.oracle_script_id,
            ask_count: msg.ask_count,
            min_count: msg.min_count,
            fee_limit: msg.fee_limit,
            prepare_gas: msg.prepare_gas,
            execute_gas: msg.execute_gas,
            multiplier: msg.multiplier,
            max_update_interval,
            symbols: msg.symbols,
        },
    )?;

    Ok(Response::new().add_attribute("method", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Request {} => try_request(deps, env),
        ExecuteMsg::UpdateConfig { new_config } => try_update_config(deps, new_config),
        ExecuteMsg::UpdateOwner { new_owner } => try_update_owner(deps, new_owner),
    }
}

pub fn try_request(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let endpoint = ENDPOINT.load(deps.storage)?;
    let config: Config = CONFIG.load(deps.storage)?;
    let symbols = config.symbols;
    let last_update = LAST_UPDATE.load(deps.storage).unwrap_or(0u64);
    if env.block.time.seconds() - last_update < config.max_update_interval {
        return Err(ContractError::TooEarly {});
    }
    let raw_calldata = Input {
        symbols,
        multiplier: config.multiplier.into(),
    }
    .try_to_vec()
    .map(Binary)
    .map_err(|err| ContractError::CustomError {
        val: err.to_string(),
    })?;

    let packet = OracleRequestPacketData {
        client_id: config.client_id,
        oracle_script_id: config.oracle_script_id,
        calldata: raw_calldata,
        ask_count: config.ask_count,
        min_count: config.min_count,
        prepare_gas: config.prepare_gas,
        execute_gas: config.execute_gas,
        fee_limit: config.fee_limit,
    };

    let msg = IbcMsg::SendPacket {
        channel_id: endpoint.channel_id,
        data: to_binary(&packet)?,
        timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(60)),
    };
    LAST_UPDATE.save(deps.storage, &env.block.time.seconds())?;
    Ok(Response::default().add_message(msg))
}

pub fn try_update_config(
    deps: DepsMut,
    new_config: UpdateConfigMsg,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if let Some(client_id) = new_config.client_id {
        config.client_id = client_id;
    }
    if let Some(oracle_script_id) = new_config.oracle_script_id {
        config.oracle_script_id = oracle_script_id;
    }
    if let Some(ask_count) = new_config.ask_count {
        config.ask_count = ask_count;
    }
    if let Some(min_count) = new_config.min_count {
        config.min_count = min_count;
    }
    if let Some(fee_limit) = new_config.fee_limit {
        config.fee_limit = fee_limit;
    }
    if let Some(prepare_gas) = new_config.prepare_gas {
        config.prepare_gas = prepare_gas;
    }
    if let Some(execute_gas) = new_config.execute_gas {
        config.execute_gas = execute_gas;
    }
    if let Some(multiplier) = new_config.multiplier {
        config.multiplier = multiplier;
    }
    if let Some(symbols) = new_config.symbols {
        config.symbols = symbols;
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

pub fn try_update_owner(deps: DepsMut, new_owner: String) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    config.owner = deps.api.addr_validate(&new_owner)?;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

/// this is a no-op
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: Empty) -> StdResult<Response> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetRate {} => to_binary(&query_rate(deps)?),
        QueryMsg::GetError {} => to_binary(&query_error(deps)?),
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
    }
}

fn query_error(deps: Deps) -> StdResult<String> {
    ERROR.load(deps.storage)
}

fn query_config(deps: Deps) -> StdResult<Config> {
    CONFIG.load(deps.storage)
}

fn query_rate(deps: Deps) -> StdResult<Vec<PriceFeedRate>> {
    let config = CONFIG.load(deps.storage)?;
    let mut symbols = config.symbols;
    symbols.sort();
    let mut out = vec![];
    for s in symbols {
        out.push(RATES.load(deps.storage, &s)?);
    }
    Ok(out)
}
