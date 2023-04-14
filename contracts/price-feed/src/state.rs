use astroport_periphery::pricefeed::Config;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::IbcEndpoint;
use cosmwasm_std::{Uint256, Uint64};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct PriceFeedRate {
    // Rate of an asset relative to USD
    pub rate: Uint64,
    // The resolve time of the request ID
    pub resolve_time: Uint64,
    // The request ID where the rate was derived from
    pub request_id: Uint64,
}

impl PriceFeedRate {
    pub fn new(rate: Uint64, resolve_time: Uint64, request_id: Uint64) -> Self {
        PriceFeedRate {
            rate,
            resolve_time,
            request_id,
        }
    }
}

pub const RATES: Map<&str, PriceFeedRate> = Map::new("rates");
pub const ERROR: Item<String> = Item::new("error");
pub const ENDPOINT: Item<IbcEndpoint> = Item::new("endpoint");
pub const CONFIG: Item<Config> = Item::new("config");
pub const LAST_UPDATE: Item<u64> = Item::new("last_update");

#[cw_serde]
pub struct ReferenceData {
    // Pair rate e.g. rate of BTC/USD
    pub rate: Uint256,
    // Unix time of when the base asset was last updated. e.g. Last update time of BTC in Unix time
    pub last_updated_base: Uint64,
    // Unix time of when the quote asset was last updated. e.g. Last update time of USD in Unix time
    pub last_updated_quote: Uint64,
}

impl ReferenceData {
    pub fn new(rate: Uint256, last_updated_base: Uint64, last_updated_quote: Uint64) -> Self {
        ReferenceData {
            rate,
            last_updated_base,
            last_updated_quote,
        }
    }
}
