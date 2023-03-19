use crate::error::ContractError;
use crate::querier::{query_cumulative_prices, query_prices};
use crate::state::{Config, PriceCumulativeLast, CONFIG, LAST_UPDATE_HEIGHT, PRICE_LAST};
use astroport::asset::{addr_validate_to_lower, Asset, AssetInfo, Decimal256Ext};
use astroport::cosmwasm_ext::IntegerToDecimal;
use astroport::oracle::{ExecuteMsg, InstantiateMsg, QueryMsg};
use astroport::pair::TWAP_PRECISION;
use astroport::querier::{query_pair_info, query_token_precision};
use cosmwasm_std::{
    entry_point, to_binary, Binary, Decimal256, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Uint128, Uint256, Uint64,
};
use cw2::set_contract_version;
use std::ops::Div;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-oracle";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    msg.asset_infos[0].check(deps.api)?;
    msg.asset_infos[1].check(deps.api)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let factory_contract = addr_validate_to_lower(deps.api, &msg.factory_contract)?;
    let pair_info = query_pair_info(&deps.querier, &factory_contract, &msg.asset_infos)?;

    let config = Config {
        owner: info.sender,
        factory: factory_contract,
        asset_infos: msg.asset_infos,
        pair: pair_info.clone(),
        period: msg.period,
    };
    CONFIG.save(deps.storage, &config)?;
    let prices = query_cumulative_prices(deps.querier, pair_info.contract_addr)?;
    let average_prices = prices
        .cumulative_prices
        .iter()
        .cloned()
        .map(|(from, to, _)| (from, to, Decimal256::zero()))
        .collect();

    let price = PriceCumulativeLast {
        cumulative_prices: prices.cumulative_prices,
        average_prices,
        block_timestamp_last: env.block.time.seconds(),
    };
    PRICE_LAST.save(deps.storage, &price, env.block.height)?;
    LAST_UPDATE_HEIGHT.save(deps.storage, &Uint64::zero())?;
    Ok(Response::default())
}

/// Exposes all the execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::Update {}** Updates the local TWAP values for the assets in the Astroport pool.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Update {} => update(deps, env),
        ExecuteMsg::UpdatePeriod { new_period } => update_period(deps, env, info, new_period),
    }
}

pub fn update_period(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_period: u64,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    config.period = new_period;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_config")
        .add_attribute("new_period", config.period.to_string()))
}

/// Updates the local TWAP values for the tokens in the target Astroport pool.
pub fn update(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let price_last = PRICE_LAST.load(deps.storage)?;

    let prices = query_cumulative_prices(deps.querier, config.pair.contract_addr)?;
    let time_elapsed = env.block.time.seconds() - price_last.block_timestamp_last;

    // Ensure that at least one full period has passed since the last update
    if time_elapsed < config.period {
        return Err(ContractError::WrongPeriod {});
    }

    let mut average_prices = vec![];
    for (asset1_last, asset2_last, price_last) in price_last.cumulative_prices.iter() {
        for (asset1, asset2, price) in prices.cumulative_prices.iter() {
            if asset1.equal(asset1_last) && asset2.equal(asset2_last) {
                average_prices.push((
                    asset1.clone(),
                    asset2.clone(),
                    Decimal256::from_ratio(
                        Uint256::from(price.wrapping_sub(*price_last)),
                        time_elapsed,
                    ),
                ));
            }
        }
    }

    let prices = PriceCumulativeLast {
        cumulative_prices: prices.cumulative_prices,
        average_prices,
        block_timestamp_last: env.block.time.seconds(),
    };
    LAST_UPDATE_HEIGHT.save(deps.storage, &Uint64::from(env.block.height))?;
    PRICE_LAST.save(deps.storage, &prices, env.block.height)?;
    Ok(Response::default())
}

/// Exposes all the queries available in the contract.
///
/// ## Queries
/// * **QueryMsg::Consult { token, amount }** Validates assets and calculates a new average
/// amount with updated precision
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Consult { token, amount } => to_binary(&consult(deps, token, amount)?),
        QueryMsg::TWAPAtHeight { token, height } => {
            to_binary(&twap_at_height(deps, token, height)?)
        }
    }
}

/// Multiplies a token amount by its latest TWAP value.
/// * **token** token for which we multiply its TWAP value by an amount.
///
/// * **amount** amount of tokens we multiply the TWAP by.
fn consult(
    deps: Deps,
    token: AssetInfo,
    amount: Uint128,
) -> Result<Vec<(AssetInfo, Uint256)>, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let price_last = PRICE_LAST.load(deps.storage)?;

    let mut average_prices = vec![];
    for (from, to, value) in price_last.average_prices {
        if from.equal(&token) {
            average_prices.push((to, value));
        }
    }

    if average_prices.is_empty() {
        return Err(StdError::generic_err("Invalid Token"));
    }

    // Get the token's precision
    let p = query_token_precision(&deps.querier, &token)?;
    let one = Uint128::new(10_u128.pow(p.into()));

    average_prices
        .iter()
        .map(|(asset, price_average)| {
            if price_average.is_zero() {
                let price = query_prices(
                    deps.querier,
                    config.pair.contract_addr.clone(),
                    Asset {
                        info: token.clone(),
                        amount: one,
                    },
                    Some(asset.clone()),
                )?
                .return_amount;
                Ok((
                    asset.clone(),
                    Uint256::from(price).multiply_ratio(Uint256::from(amount), Uint256::from(one)),
                ))
            } else {
                let price_precision = Uint256::from(10_u128.pow(TWAP_PRECISION.into()));
                Ok((
                    asset.clone(),
                    Uint256::from(amount) * *price_average / price_precision,
                ))
            }
        })
        .collect::<Result<Vec<(AssetInfo, Uint256)>, StdError>>()
}

/// Returns token TWAP value for given height.
/// * **token** token for which we getting its historicalTWAP value.
///
/// * **height** height, on which we receive TWAP
fn twap_at_height(
    deps: Deps,
    token: AssetInfo,
    height: Uint64,
) -> Result<Vec<(AssetInfo, Decimal256)>, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let last_height = LAST_UPDATE_HEIGHT.load(deps.storage)?;
    let mut query_height = height;
    // if requested height > last snapshoted time, SnapshotItem.may_load_at_height() will return primary (default) value
    // which is very first stored data. To avoid that, in such cases we just query TWAP for last known height.
    if height > last_height {
        query_height = last_height
    }
    let price_last = PRICE_LAST
        .may_load_at_height(deps.storage, u64::from(query_height))
        .unwrap()
        .unwrap();
    let mut average_prices = vec![];
    for (from, to, value) in price_last.average_prices {
        if from.equal(&token) {
            average_prices.push((to, value));
        }
    }

    if average_prices.is_empty() {
        return Err(StdError::generic_err("Invalid Token"));
    }

    // Get the token's precision
    let p = query_token_precision(&deps.querier, &token)?;
    let one = Uint128::new(10_u128.pow(p.into()));

    average_prices
        .iter()
        .map(|(asset, price_average)| {
            if price_average.is_zero() {
                let price = query_prices(
                    deps.querier,
                    config.pair.contract_addr.clone(),
                    Asset {
                        info: token.clone(),
                        amount: one,
                    },
                    Some(asset.clone()),
                )?
                .return_amount;
                Ok((
                    asset.clone(),
                    Decimal256::from_integer(Uint256::from(price))
                        .div(Decimal256::from(one.to_decimal())),
                ))
            } else {
                let price_precision = Uint256::from(10_u128.pow(TWAP_PRECISION.into()));
                Ok((asset.clone(), *price_average / price_precision))
            }
        })
        .collect::<Result<Vec<(AssetInfo, Decimal256)>, StdError>>()
}
