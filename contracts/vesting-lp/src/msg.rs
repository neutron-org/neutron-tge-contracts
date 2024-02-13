use cosmwasm_schema::cw_serde;
use cosmwasm_std::{to_binary, Addr, CosmosMsg, Decimal, Env, StdResult, Uint128, WasmMsg};
use vesting_base::msg::ExecuteMsg as BaseExecute;
use vesting_base::types::VestingAccountResponse;

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
    Base(BaseExecute),
    #[serde(rename = "migrate_liquidity_to_pcl_pool")]
    MigrateLiquidityToPCLPool {
        user: Option<String>,
    },
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
}
