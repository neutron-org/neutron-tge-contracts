use cosmwasm_schema::cw_serde;

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Address allowed to change contract parameters
    pub owner: String,
    /// Initial list of whitelisted vesting managers
    pub vesting_managers: Vec<String>,
    /// Token info manager address
    pub token_info_manager: String,
    pub xyk_vesting_lp_contract: String,
}
