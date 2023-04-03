use astroport::{asset::AssetInfo, vesting::VestingAccount};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;
use cw20::Cw20ReceiveMsg;

/// This structure describes the execute messages available in the contract.
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
    /// Removes vesting targets/accounts.
    /// ## Executor
    /// Only the current owner can execute this
    RemoveVestingAccounts {
        vesting_accounts: Vec<String>,
        /// Specifies the account that will receive the funds taken from the vesting accounts.
        clawback_account: String,
    },
    SetVestingToken {
        vesting_token: AssetInfo,
    },
}
