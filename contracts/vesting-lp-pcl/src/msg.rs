use astroport::asset::AssetInfo;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;
use vesting_base::msg::{
    ExecuteMsg as BaseExecute, ExecuteMsgHistorical, ExecuteMsgManaged, ExecuteMsgWithManagers,
};
use vesting_base::types::{VestingAccount, VestingInfo};

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
    pub vesting_token: AssetInfo,
}

#[cw_serde]
pub enum ExecuteMsg {
    // Claim claims vested tokens and sends them to a recipient
    Claim {
        /// The address that receives the vested tokens
        recipient: Option<String>,
        /// The amount of tokens to claim
        amount: Option<Uint128>,
    },
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
    /// Sets vesting token
    /// ## Executor
    /// Only the current owner or token info manager can execute this
    SetVestingToken {
        vesting_token: AssetInfo,
    },
    /// Contains messages associated with the managed extension for vesting contracts.
    ManagedExtension {
        msg: ExecuteMsgManaged,
    },
    /// Contains messages associated with the with_managers extension for vesting contracts.
    WithManagersExtension {
        msg: ExecuteMsgWithManagers,
    },
    /// Contains messages associated with the historical extension for vesting contracts.
    HistoricalExtension {
        msg: ExecuteMsgHistorical,
    },
    #[serde(rename = "migrate_liquidity_to_pcl_pool")]
    MigrateLiquidityToPCLPool {
        user: Option<String>,
    },
    Receive(Cw20ReceiveMsg),
}

/// This structure describes a CW20 hook message.
#[cw_serde]
pub enum Cw20HookMsg {
    /// RegisterVestingAccounts registers vesting targets/accounts
    RegisterVestingAccounts {
        vesting_accounts: Vec<VestingAccount>,
    },
    #[serde(rename = "migrate_xyk_liquidity")]
    MigrateXYKLiquidity {
        /// The address of the user which owns the vested tokens.
        user_address_raw: Addr,
        user_vesting_info: VestingInfo,
    },
}

impl From<ExecuteMsg> for BaseExecute {
    fn from(item: ExecuteMsg) -> Self {
        match item {
            ExecuteMsg::Claim { recipient, amount } => BaseExecute::Claim { recipient, amount },
            ExecuteMsg::RegisterVestingAccounts { vesting_accounts } => {
                BaseExecute::RegisterVestingAccounts { vesting_accounts }
            }
            ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
                BaseExecute::ProposeNewOwner { owner, expires_in }
            }
            ExecuteMsg::DropOwnershipProposal {} => BaseExecute::DropOwnershipProposal {},
            ExecuteMsg::ClaimOwnership {} => BaseExecute::ClaimOwnership {},
            ExecuteMsg::SetVestingToken { vesting_token } => {
                BaseExecute::SetVestingToken { vesting_token }
            }
            ExecuteMsg::ManagedExtension { msg } => BaseExecute::ManagedExtension { msg },
            ExecuteMsg::WithManagersExtension { msg } => BaseExecute::WithManagersExtension { msg },
            ExecuteMsg::HistoricalExtension { msg } => BaseExecute::HistoricalExtension { msg },
            _ => panic!("Unhandled ExecuteMsg variant"),
        }
    }
}
