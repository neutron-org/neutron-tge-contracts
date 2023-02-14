use cosmwasm_schema::{cw_serde, QueryResponses};

use astroport::asset::{Asset, AssetInfo, PairInfo};

use cosmwasm_std::{from_slice, Addr, Binary, Decimal, QuerierWrapper, StdResult, Uint128};
use cw20::Cw20ReceiveMsg;

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Information about assets in the pool
    pub asset_infos: Vec<AssetInfo>,
    /// The token contract code ID used for the tokens in the pool
    pub token_code_id: u64,
    /// The factory contract address
    pub factory_addr: String,
    /// Optional binary serialised parameters for custom pool types
    pub init_params: Option<Binary>,
    /// Lockdrop contract address
    pub lockdrop_addr: Addr,
    /// Auction address
    pub auction_addr: Addr,
}
