use crate::error::ContractError;
use crate::state::{Config, PriceFeedRate, BAND_CONFIG, ENDPOINT, ERROR, RATES};
use astroport_periphery::pricefeed::{ExecuteMsg, InstantiateMsg, QueryMsg};
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    BAND_CONFIG.save(
        deps.storage,
        &Config {
            client_id: msg.client_id,
            oracle_script_id: msg.oracle_script_id,
            ask_count: msg.ask_count,
            min_count: msg.min_count,
            fee_limit: msg.fee_limit,
            prepare_gas: msg.prepare_gas,
            execute_gas: msg.execute_gas,
            multiplier: msg.multiplier,
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
    }
}

pub fn try_request(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let endpoint = ENDPOINT.load(deps.storage)?;
    let config = BAND_CONFIG.load(deps.storage)?;
    let symbols = config.symbols;

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

    Ok(Response::default().add_message(msg))
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
    }
}

fn query_error(deps: Deps) -> StdResult<String> {
    ERROR.load(deps.storage)
}

fn query_rate(deps: Deps) -> StdResult<Vec<PriceFeedRate>> {
    let config = BAND_CONFIG.load(deps.storage)?;
    let mut symbols = config.symbols;
    symbols.sort();
    let mut out = vec![];
    for s in symbols {
        out.push(RATES.load(deps.storage, &s)?);
    }
    Ok(out)
}
