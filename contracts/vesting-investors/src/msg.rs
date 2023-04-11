use cosmwasm_schema::cw_serde;

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Address allowed to change contract parameters
    pub owner: String,
    /// Token info manager address
    pub token_info_manager: String,
}
