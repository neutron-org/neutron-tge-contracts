use astroport::asset::AssetInfo;
use cosmwasm_std::{from_json, Addr, Empty, QuerierWrapper, StdError, StdResult, Uint128};
use cw_storage_plus::Path;
use serde::Deserialize;

/// Returns generator deposit of tokens for the specified address
pub fn raw_generator_deposit(
    querier: QuerierWrapper,
    generator: &Addr,
    lp_token: &[u8],
    address: &[u8],
) -> StdResult<Uint128> {
    #[derive(Deserialize)]
    struct UserInfo {
        amount: Uint128,
    }

    let key: Path<Empty> = Path::new(b"user_info", &[lp_token, address]);
    if let Some(res) = &querier.query_wasm_raw(generator, key.to_vec())? {
        let UserInfo { amount } = from_json(res)?;
        Ok(amount)
    } else {
        Ok(Uint128::zero())
    }
}

/// Returns balance of tokens for the specified address
pub fn raw_balance(querier: QuerierWrapper, token: &Addr, address: &[u8]) -> StdResult<Uint128> {
    let key: Path<Empty> = Path::new(b"balance", &[address]);
    if let Some(res) = &querier.query_wasm_raw(token, key.to_vec())? {
        let res: Uint128 = from_json(res)?;
        Ok(res)
    } else {
        Ok(Uint128::zero())
    }
}

/// Returns AssetInfo for the specified proxy address from generator storage
pub fn raw_proxy_asset(
    querier: QuerierWrapper,
    generator: &Addr,
    address: &[u8],
) -> StdResult<AssetInfo> {
    let key: Path<Empty> = Path::new(b"proxy_reward_asset", &[address]);
    if let Some(res) = &querier.query_wasm_raw(generator, key.to_vec())? {
        let res: AssetInfo = from_json(res)?;
        return Ok(res);
    }
    Err(StdError::generic_err(format!(
        "Proxy asset not found: {}",
        String::from_utf8(address.to_vec())?
    )))
}
