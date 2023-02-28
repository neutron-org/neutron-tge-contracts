use astroport::asset::AssetInfo;
use astroport::generator::{PoolInfoResponse, QueryMsg as GenQueryMsg};
use astroport_periphery::lockdrop::MigrationInfo;
use cosmwasm_std::{Addr, Decimal, DepsMut, StdResult, Uint128, Uint256};
use cw_storage_plus::Map;

use crate::raw_queries::raw_proxy_asset;
use astroport::restricted_vector::RestrictedVector;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfoV101 {
    pub terraswap_pool: Addr,
    pub terraswap_amount_in_lockups: Uint128,
    pub migration_info: Option<MigrationInfo>,
    /// Share of total ASTRO incentives allocated to this pool
    pub incentives_share: u64,
    /// Weighted LP Token balance used to calculate ASTRO rewards a particular user can claim
    pub weighted_amount: Uint256,
    /// Ratio of Generator ASTRO rewards accured to astroport pool share
    pub generator_astro_per_share: Decimal,
    /// Ratio of Generator Proxy rewards accured to astroport pool share
    pub generator_proxy_per_share: Decimal,
    /// Boolean value indicating if the LP Tokens are staked with the Generator contract or not
    pub is_staked: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfoV111 {
    pub terraswap_pool: Addr,
    pub terraswap_amount_in_lockups: Uint128,
    pub migration_info: Option<MigrationInfo>,
    /// Share of total ASTRO incentives allocated to this pool
    pub incentives_share: u64,
    /// Weighted LP Token balance used to calculate ASTRO rewards a particular user can claim
    pub weighted_amount: Uint256,
    /// Ratio of Generator ASTRO rewards accured to astroport pool share
    pub generator_astro_per_share: Decimal,
    /// Ratio of Generator Proxy rewards accured to astroport pool share
    pub generator_proxy_per_share: Decimal,
    /// Boolean value indicating if the LP Tokens are staked with the Generator contract or not
    pub is_staked: bool,
    /// Flag defines whether the asset has rewards or not
    pub has_asset_rewards: bool,
}

pub const ASSET_POOLS_V101: Map<&Addr, PoolInfoV101> = Map::new("LiquidityPools");
pub const ASSET_POOLS_V111: Map<&Addr, PoolInfoV111> = Map::new("LiquidityPools");

pub fn migrate_generator_proxy_per_share_to_v120(
    deps: &DepsMut,
    generator_proxy_per_share_old: Decimal,
    generator: &Addr,
    migration_info: Option<MigrationInfo>,
) -> StdResult<RestrictedVector<AssetInfo, Decimal>> {
    let mut generator_proxy_per_share = RestrictedVector::default();
    if !generator_proxy_per_share_old.is_zero() {
        let pool_info: PoolInfoResponse = deps.querier.query_wasm_smart(
            generator,
            &GenQueryMsg::PoolInfo {
                lp_token: migration_info
                    .expect("Should be migrated!")
                    .astroport_lp_token
                    .to_string(),
            },
        )?;
        let (proxy, _) = pool_info
            .accumulated_proxy_rewards_per_share
            .first()
            .expect("Proxy reward should be set!");
        let reward_asset = raw_proxy_asset(deps.querier, generator, proxy.as_bytes())?;
        generator_proxy_per_share.update(&reward_asset, generator_proxy_per_share_old)?;
    }

    Ok(generator_proxy_per_share)
}
