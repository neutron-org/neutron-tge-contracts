use crate::types::{
    Config, OrderBy, VestingAccount, VestingAccountResponse, VestingAccountsResponse, VestingState,
};
use astroport::asset::AssetInfo;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;

/// This structure describes the execute messages available in a vesting contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Claim claims vested tokens and sends them to a recipient
    Claim {
        /// The address that receives the vested tokens
        recipient: Option<String>,
        /// The amount of tokens to claim
        amount: Option<Uint128>,
    },
    /// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template
    Receive(Cw20ReceiveMsg),
    /// RegisterVestingAccounts registers vesting targets/accounts
    RegisterVestingAccounts {
        vesting_accounts: Vec<VestingAccount>,
    },
    /// Creates a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the offer to change the owner
        expires_in: u64,
    },
    /// Removes a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this
    DropOwnershipProposal {},
    /// Claims contract ownership
    /// ## Executor
    /// Only the newly proposed owner can execute this
    ClaimOwnership {},
    /// Contains messages associated with the managed extension for vesting contracts.
    ManagedExtension { msg: ExecuteMsgManaged },
    /// Contains messages associated with the with_managers extension for vesting contracts.
    WithManagersExtension { msg: ExecuteMsgWithManagers },
    /// Contains messages associated with the historical extension for vesting contracts.
    HistoricalExtension { msg: ExecuteMsgHistorical },
    /// Sets vesting token
    /// ## Executor
    /// Only the current owner or token info manager can execute this
    SetVestingToken { vesting_token: AssetInfo },
}

/// This structure describes the execute messages available in a managed vesting contract.
#[cw_serde]
pub enum ExecuteMsgManaged {
    /// Removes vesting targets/accounts.
    /// ## Executor
    /// Only the current owner can execute this
    RemoveVestingAccounts {
        vesting_accounts: Vec<String>,
        /// Specifies the account that will receive the funds taken from the vesting accounts.
        clawback_account: String,
    },
}

/// This structure describes the execute messages available in a with_managers vesting contract.
#[cw_serde]
pub enum ExecuteMsgWithManagers {
    /// Adds vesting managers
    /// ## Executor
    /// Only the current owner can execute this
    AddVestingManagers { managers: Vec<String> },
    /// Removes vesting managers
    /// ## Executor
    /// Only the current owner can execute this
    RemoveVestingManagers { managers: Vec<String> },
}

/// This structure describes the execute messages available in a historical vesting contract.
#[cw_serde]
pub enum ExecuteMsgHistorical {}

/// This structure describes the query messages available in a vesting contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the configuration for the contract using a [`ConfigResponse`] object.
    #[returns(Config)]
    Config {},
    /// Returns information about an address vesting tokens using a [`VestingAccountResponse`] object.
    #[returns(VestingAccountResponse)]
    VestingAccount { address: String },
    /// Returns a list of addresses that are vesting tokens using a [`VestingAccountsResponse`] object.
    #[returns(VestingAccountsResponse)]
    VestingAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    },
    /// Returns the total unvested amount of tokens for a specific address.
    #[returns(Uint128)]
    AvailableAmount { address: String },
    /// Timestamp returns the current timestamp
    #[returns(u64)]
    Timestamp {},
    /// VestingState returns the current vesting state.
    #[returns(VestingState)]
    VestingState {},
    /// Contains messages associated with the managed extension for vesting contracts.
    #[returns(QueryMsgManaged)]
    ManagedExtension { msg: QueryMsgManaged },
    /// Contains messages associated with the with_managers extension for vesting contracts.
    #[returns(QueryMsgWithManagers)]
    WithManagersExtension { msg: QueryMsgWithManagers },
    /// Contains messages associated with the historical extension for vesting contracts.
    #[returns(QueryMsgHistorical)]
    HistoricalExtension { msg: QueryMsgHistorical },
}

/// This structure describes the query messages available in a managed vesting contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsgManaged {}

/// This structure describes the query messages available in a with_managers vesting contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsgWithManagers {
    /// Returns list of vesting managers
    /// (the persons who are able to add/remove vesting schedules)
    #[returns(Vec<Addr>)]
    VestingManagers {},
}

/// This structure describes the query messages available in a historical vesting contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsgHistorical {
    /// Returns the total unclaimed amount of tokens for a specific address at certain height.
    #[returns(Uint128)]
    UnclaimedAmountAtHeight { address: String, height: u64 },
    /// Returns the total unclaimed amount of tokens for all the users at certain height.
    #[returns(Uint128)]
    UnclaimedTotalAmountAtHeight { height: u64 },
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {}

/// This structure describes a CW20 hook message.
#[cw_serde]
pub enum Cw20HookMsg {
    /// RegisterVestingAccounts registers vesting targets/accounts
    RegisterVestingAccounts {
        vesting_accounts: Vec<VestingAccount>,
    },
}
