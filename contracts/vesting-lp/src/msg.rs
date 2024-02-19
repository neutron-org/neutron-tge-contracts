use astroport::asset::AssetInfo;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_binary, Addr, CosmosMsg, Decimal, Env, StdResult, Uint128, WasmMsg};
use cw20::Cw20ReceiveMsg;
use vesting_base::msg::{
    ExecuteMsg as BaseExecute, ExecuteMsgHistorical, ExecuteMsgManaged, ExecuteMsgWithManagers,
};
use vesting_base::types::{VestingAccount, VestingAccountResponse};

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Address allowed to change contract parameters
    pub owner: String,
    /// Initial list of whitelisted vesting managers
    pub vesting_managers: Vec<String>,
    /// Token info manager address
    pub token_info_manager: String,
}

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
    /// Sets vesting token
    /// ## Executor
    /// Only the current owner or token info manager can execute this
    SetVestingToken { vesting_token: AssetInfo },
    /// Contains messages associated with the managed extension for vesting contracts.
    ManagedExtension { msg: ExecuteMsgManaged },
    /// Contains messages associated with the with_managers extension for vesting contracts.
    WithManagersExtension { msg: ExecuteMsgWithManagers },
    /// Contains messages associated with the historical extension for vesting contracts.
    HistoricalExtension { msg: ExecuteMsgHistorical },
    #[serde(rename = "migrate_liquidity_to_pcl_pool")]
    MigrateLiquidityToPCLPool { user: Option<String> },
    /// Callbacks; only callable by the contract itself.
    Callback(CallbackMsg),
}

#[cw_serde]
pub enum CallbackMsg {
    MigrateLiquidityToClPair {
        xyk_pair: Addr,
        xyk_lp_token: Addr,
        amount: Uint128,
        slippage_tolerance: Decimal,
        cl_pair: Addr,
        ntrn_denom: String,
        paired_asset_denom: String,
        user: VestingAccountResponse,
    },
    ProvideLiquidityToClPairAfterWithdrawal {
        ntrn_denom: String,
        ntrn_init_balance: Uint128,
        paired_asset_denom: String,
        paired_asset_init_balance: Uint128,
        cl_pair: Addr,
        slippage_tolerance: Decimal,
        user: VestingAccountResponse,
    },
    PostMigrationVestingReschedule {
        user: VestingAccountResponse,
        init_balance_pcl_lp: Uint128,
    },
}

// Modified from
// https://github.com/CosmWasm/cosmwasm-plus/blob/v0.2.3/packages/cw20/src/receiver.rs#L15
impl CallbackMsg {
    pub fn to_cosmos_msg(self, env: &Env) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::Callback(self))?,
            funds: vec![],
        }))
    }
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[cw_serde]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {
    pub max_slippage: Decimal,
    pub ntrn_denom: String,
    pub paired_denom: String,
    pub xyk_pair: String,
    pub cl_pair: String,
    pub new_lp_token: String,
    pub pcl_vesting: String,
    pub dust_threshold: Uint128,
}

impl From<ExecuteMsg> for BaseExecute {
    fn from(item: ExecuteMsg) -> Self {
        match item {
            ExecuteMsg::Claim { recipient, amount } => BaseExecute::Claim { recipient, amount },
            ExecuteMsg::Receive(msg) => BaseExecute::Receive(msg),
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
