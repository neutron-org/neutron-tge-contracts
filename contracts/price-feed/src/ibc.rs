#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, from_slice, Binary, DepsMut, Env, Ibc3ChannelOpenResponse, IbcBasicResponse, IbcChannel,
    IbcChannelCloseMsg, IbcChannelConnectMsg, IbcChannelOpenMsg, IbcChannelOpenResponse, IbcOrder,
    IbcPacket, IbcPacketAckMsg, IbcPacketReceiveMsg, IbcPacketTimeoutMsg, IbcReceiveResponse,
    StdResult, Uint64,
};
use obi::dec::OBIDecode;

use crate::error::{ContractError, Never};
use crate::state::{PriceFeedRate, BAND_CONFIG, ENDPOINT, ERROR, RATES};

use cw_band::{ack_fail, ack_success, OracleResponsePacketData, ResolveStatus, IBC_APP_VERSION};

#[cfg_attr(not(feature = "library"), entry_point)]
/// enforces ordering and versioning constraints
pub fn ibc_channel_open(
    _deps: DepsMut,
    _env: Env,
    msg: IbcChannelOpenMsg,
) -> Result<IbcChannelOpenResponse, ContractError> {
    enforce_order_and_version(msg.channel(), msg.counterparty_version())?;

    Ok(Some(Ibc3ChannelOpenResponse {
        version: msg.channel().version.clone(),
    }))
}

#[cfg_attr(not(feature = "library"), entry_point)]
/// record the channel in ENDPOINT
pub fn ibc_channel_connect(
    deps: DepsMut,
    _env: Env,
    msg: IbcChannelConnectMsg,
) -> Result<IbcBasicResponse, ContractError> {
    // we need to check the counter party version in try and ack (sometimes here)
    enforce_order_and_version(msg.channel(), msg.counterparty_version())?;

    ENDPOINT.save(deps.storage, &msg.channel().endpoint)?;
    Ok(IbcBasicResponse::default())
}

fn enforce_order_and_version(
    channel: &IbcChannel,
    counterparty_version: Option<&str>,
) -> Result<(), ContractError> {
    if channel.version != IBC_APP_VERSION {
        return Err(ContractError::InvalidIbcVersion {
            version: channel.version.clone(),
        });
    }
    if let Some(version) = counterparty_version {
        if version != IBC_APP_VERSION {
            return Err(ContractError::InvalidIbcVersion {
                version: version.to_string(),
            });
        }
    }
    if channel.order != IbcOrder::Unordered {
        return Err(ContractError::OnlyUnorderedChannel {});
    }
    Ok(())
}

#[entry_point]
pub fn ibc_channel_close(
    _deps: DepsMut,
    _env: Env,
    _msg: IbcChannelCloseMsg,
) -> StdResult<IbcBasicResponse> {
    unimplemented!();
}

#[entry_point]
pub fn ibc_packet_receive(
    deps: DepsMut,
    env: Env,
    msg: IbcPacketReceiveMsg,
) -> Result<IbcReceiveResponse, Never> {
    let packet = msg.packet;
    do_ibc_packet_receive(deps, env, &packet).or_else(|err| {
        Ok(IbcReceiveResponse::new()
            .set_ack(ack_fail(err.to_string()))
            .add_attributes(vec![
                attr("action", "receive"),
                attr("success", "false"),
                attr("error", err.to_string()),
            ]))
    })
}

fn do_ibc_packet_receive(
    deps: DepsMut,
    env: Env,
    packet: &IbcPacket,
) -> Result<IbcReceiveResponse, ContractError> {
    let resp: OracleResponsePacketData = from_slice(&packet.data)?;
    let config = BAND_CONFIG.load(deps.storage)?;
    let symbols = config.symbols;
    deps.api
        .debug(&format!("WASMDEBUG symbols: {:?} {:?}", symbols, resp));

    if resp.resolve_status != ResolveStatus::Success {
        ERROR.save(deps.storage, &resp.result.to_string())?;
        return Err(ContractError::RequestNotSuccess {});
    }
    let bin_res = Binary::from_base64(&resp.result.to_string())?;
    let rates: Vec<u64> = OBIDecode::decode(&mut bin_res.as_slice())
        .map_err(|_| ContractError::RequestNotSuccess {})?;
    // load request
    deps.api.debug("WASMDEBUG rates");

    for i in 0..rates.len() {
        let rate = PriceFeedRate {
            rate: Uint64::from(rates[i]),
            resolve_time: Uint64::from(env.block.time.seconds()),
            request_id: resp.request_id,
        };
        RATES.save(deps.storage, &symbols[i], &rate)?;
    }
    deps.api.debug("WASMDEBUG done");

    Ok(IbcReceiveResponse::new()
        .set_ack(ack_success())
        .add_attribute("action", "ibc_packet_received"))
}

#[entry_point]
pub fn ibc_packet_ack(
    _deps: DepsMut,
    _env: Env,
    _msg: IbcPacketAckMsg,
) -> StdResult<IbcBasicResponse> {
    // We ignore acknowledgement from BandChain becuase it doesn't neccessary to know request id when handle result.
    Ok(IbcBasicResponse::new().add_attribute("action", "ibc_packet_ack"))
}

#[entry_point]
/// TODO: Handle when didn't get response packet in time
pub fn ibc_packet_timeout(
    _deps: DepsMut,
    _env: Env,
    _msg: IbcPacketTimeoutMsg,
) -> StdResult<IbcBasicResponse> {
    Ok(IbcBasicResponse::new().add_attribute("action", "ibc_packet_timeout"))
}

#[test]
fn ttt() {
    let x = "AAAAAgAAAAAA0+hvAAAAAAAPQdw=";
    let b = Binary::from_base64(x).unwrap();
    let res: Vec<u64> = OBIDecode::decode(&mut b.as_slice()).unwrap();

    println!("{:?}\n{:?}", res, b.len());
    // let result: Result<Vec<i128>, _> = from_slice(b.as_slice());
    // println!("{:?}", result);
}
