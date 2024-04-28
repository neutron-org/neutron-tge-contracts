use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal};
use cw_storage_plus::Item;

/// Config for xyk->CL liquidity migration.
#[cw_serde]
pub struct XykToClMigrationConfig {
    /// The maximum allowed slippage tolerance for xyk to CL liquidity migration calls.
    pub max_slippage: Decimal,
    pub ntrn_denom: String,
    pub xyk_pair: Addr,
    pub paired_denom: String,
    pub cl_pair: Addr,
    pub new_lp_token: Addr,
    pub pcl_vesting: Addr,
}

pub const XYK_TO_CL_MIGRATION_CONFIG: Item<XykToClMigrationConfig> =
    Item::new("xyk_to_cl_migration_config");
