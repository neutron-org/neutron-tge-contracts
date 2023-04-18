use cw_storage_plus::Map;

/// The first key is denom, the second key is a precision.
pub const COINS_INFO: Map<String, u8> = Map::new("coins_info");
