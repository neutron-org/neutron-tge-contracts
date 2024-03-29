use cosmwasm_schema::cw_serde;

use astroport::asset::AssetInfo;
use astroport::oracle::Config;
use cosmwasm_std::{Addr, Decimal256, DepsMut, StdResult, Storage, Uint128};
use cw_storage_plus::{Item, Map, SnapshotItem, Strategy};

/// Stores the contract config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
/// Stores the latest cumulative and average prices at the given key and height
pub const PRICE_LAST: SnapshotItem<PriceCumulativeLast> = SnapshotItem::new(
    "price_last",
    "price_last_checkpoints",
    "price_last_changelog",
    Strategy::EveryBlock,
);

/// This structure stores the latest cumulative and average token prices for the target pool
#[cw_serde]
pub struct PriceCumulativeLast {
    /// The vector contains last cumulative prices for each pair of assets in the pool
    pub cumulative_prices: Vec<(AssetInfo, AssetInfo, Uint128)>,
    /// The vector contains average prices for each pair of assets in the pool
    pub average_prices: Vec<(AssetInfo, AssetInfo, Decimal256)>,
    /// The last timestamp block in pool
    pub block_timestamp_last: u64,
}

/// Stores map of AssetInfo (as String) -> precision
const PRECISIONS: Map<String, u8> = Map::new("precisions");

/// Store all token precisions and return the greatest one.
pub(crate) fn store_precisions(
    deps: DepsMut,
    asset_info: &AssetInfo,
    factory_address: &Addr,
) -> StdResult<()> {
    let precision = asset_info.decimals(&deps.querier, factory_address)?;
    PRECISIONS.save(deps.storage, asset_info.to_string(), &precision)?;

    Ok(())
}

/// Loads precision of the given asset info.
pub(crate) fn get_precision(storage: &dyn Storage, asset_info: &AssetInfo) -> StdResult<u8> {
    PRECISIONS.load(storage, asset_info.to_string())
}
