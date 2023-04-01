use astroport::asset::AssetInfo;
use cosmwasm_schema::cw_serde;

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Address allowed to change contract parameters
    pub owner: String,
    /// [`AssetInfo`] of the token that's being vested
    pub vesting_token: AssetInfo,
    /// Initial list of whitelisted vesting managers
    pub vesting_managers: Vec<String>,
}
