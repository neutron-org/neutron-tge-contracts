use cosmwasm_schema::{cw_serde};

use astroport::asset::{AssetInfo};

use cosmwasm_std::{Addr, Binary, };

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
    /// Lockdrop contract address. Auction will share LP tokens w lockdrop, which is able to withdraw them
    pub lockdrop_addr: Addr,
    /// Auction address. Only auction can provide liquidity
    pub auction_addr: Addr,
}
