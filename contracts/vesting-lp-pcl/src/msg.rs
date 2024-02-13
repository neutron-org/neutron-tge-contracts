use astroport::asset::AssetInfo;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw20::Cw20ReceiveMsg;
use vesting_base::msg::ExecuteMsg as BaseExecute;
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

pub enum ExecuteMsg {
    Base(BaseExecute),
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
