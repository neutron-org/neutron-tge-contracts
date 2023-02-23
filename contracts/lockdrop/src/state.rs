use astroport::common::OwnershipProposal;
use astroport::generator::PoolInfoResponse;
use astroport::generator::QueryMsg as GenQueryMsg;
use astroport::restricted_vector::RestrictedVector;
use astroport_periphery::lockdrop::PoolType;
use astroport_periphery::lockdrop::{
    Config, LockupInfoV1, LockupInfoV2, PoolInfo, State, UserInfo,
};
use astroport_periphery::U64Key;
use cosmwasm_std::{Addr, Deps, StdError, StdResult};
use cw_storage_plus::{Item, Map};

use crate::raw_queries::raw_proxy_asset;

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");

/// Key is an Terraswap LP token address
pub const ASSET_POOLS: Map<PoolType, PoolInfo> = Map::new("LiquidityPools");
/// Key is an user address
pub const USER_INFO: Map<&Addr, UserInfo> = Map::new("users");
/// Key consists of an Terraswap LP token address, an user address, and a duration
pub const LOCKUP_INFO: Map<(PoolType, &Addr, U64Key), LockupInfoV2> = Map::new("lockup_position");
/// Old LOCKUP_INFO storage interface for backward compatibility
pub const OLD_LOCKUP_INFO: Map<(PoolType, &Addr, U64Key), LockupInfoV1> = Map::new("lockup_position");

pub trait CompatibleLoader<K, R> {
    fn compatible_load(&self, deps: Deps, key: K, generator: &Option<Addr>) -> StdResult<R>;

    fn compatible_may_load(
        &self,
        deps: Deps,
        key: K,
        generator: &Option<Addr>,
    ) -> StdResult<Option<R>>;
}

impl CompatibleLoader<(PoolType, &Addr, U64Key), LockupInfoV2>
    for Map<'_, (PoolType, &Addr, U64Key), LockupInfoV2>
{
    fn compatible_load(
        &self,
        deps: Deps,
        key: (PoolType, &Addr, U64Key),
        generator: &Option<Addr>,
    ) -> StdResult<LockupInfoV2> {
        self.load(deps.storage, key.clone()).or_else(|_| {
            let old_lockup_info = OLD_LOCKUP_INFO.load(deps.storage, key.clone())?;
            let mut generator_proxy_debt = RestrictedVector::default();
            let generator = generator.as_ref().expect("Generator should be set!");

            if !old_lockup_info.generator_proxy_debt.is_zero() {
                let asset = ASSET_POOLS.load(deps.storage, key.0)?;
                let astro_lp = asset
                    .pool;
                let pool_info: PoolInfoResponse = deps.querier.query_wasm_smart(
                    generator,
                    &GenQueryMsg::PoolInfo {
                        lp_token: astro_lp.to_string(),
                    },
                )?;
                let (proxy, _) = pool_info
                    .accumulated_proxy_rewards_per_share
                    .first()
                    .ok_or_else(|| {
                        StdError::generic_err(format!("Proxy rewards not found: {}", astro_lp))
                    })?;
                let reward_asset = raw_proxy_asset(deps.querier, generator, proxy.as_bytes())?;

                generator_proxy_debt.update(&reward_asset, old_lockup_info.generator_proxy_debt)?;
            }

            let lockup_info = LockupInfoV2 {
                lp_units_locked: old_lockup_info.lp_units_locked,
                astroport_lp_transferred: old_lockup_info.astroport_lp_transferred,
                withdrawal_flag: old_lockup_info.withdrawal_flag,
                ntrn_rewards: old_lockup_info.ntrn_rewards,
                generator_ntrn_debt: old_lockup_info.generator_ntrn_debt,
                generator_proxy_debt,
                unlock_timestamp: old_lockup_info.unlock_timestamp,
            };

            Ok(lockup_info)
        })
    }

    fn compatible_may_load(
        &self,
        deps: Deps,
        key: (PoolType, &Addr, U64Key),
        generator: &Option<Addr>,
    ) -> StdResult<Option<LockupInfoV2>> {
        if !OLD_LOCKUP_INFO.has(deps.storage, key.clone()) {
            return Ok(None);
        }
        Some(self.compatible_load(deps, key, generator)).transpose()
    }
}

pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
