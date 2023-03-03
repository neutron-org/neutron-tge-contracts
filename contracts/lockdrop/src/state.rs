use astroport::common::OwnershipProposal;
use astroport::generator::PoolInfoResponse;
use astroport::generator::QueryMsg as GenQueryMsg;
use astroport::restricted_vector::RestrictedVector;
use astroport_periphery::lockdrop::{
    Config, LockupInfoV1, LockupInfoV2, PoolInfo, State, UserInfo,
};
use astroport_periphery::U64Key;
use cosmwasm_std::{Addr, Decimal256, Deps, StdError, StdResult};
use cw_storage_plus::{Item, Map};

use crate::raw_queries::raw_proxy_asset;

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");

/// Key is an Terraswap LP token address
pub const ASSET_POOLS: Map<&Addr, PoolInfo> = Map::new("LiquidityPools");
/// Key is an user address
pub const USER_INFO: Map<&Addr, UserInfo> = Map::new("users");
/// Key consists of an Terraswap LP token address, an user address, and a duration
pub const LOCKUP_INFO: Map<(&Addr, &Addr, U64Key), LockupInfoV2> = Map::new("lockup_position");
/// Old LOCKUP_INFO storage interface for backward compatibility
pub const OLD_LOCKUP_INFO: Map<(&Addr, &Addr, U64Key), LockupInfoV1> = Map::new("lockup_position");
/// Total received asset reward by lockdrop contract per lp token share
pub const TOTAL_ASSET_REWARD_INDEX: Map<&Addr, Decimal256> = Map::new("total_asset_reward_index");
/// Last used total asset reward index for user claim ( lp_addr -> user -> duration )
pub const USERS_ASSET_REWARD_INDEX: Map<(&Addr, &Addr, U64Key), Decimal256> =
    Map::new("users_asset_reward_index");

pub trait CompatibleLoader<K, R> {
    fn compatible_load(&self, deps: Deps, key: K, generator: &Option<Addr>) -> StdResult<R>;

    fn compatible_may_load(
        &self,
        deps: Deps,
        key: K,
        generator: &Option<Addr>,
    ) -> StdResult<Option<R>>;
}

impl CompatibleLoader<(&Addr, &Addr, U64Key), LockupInfoV2>
    for Map<'_, (&Addr, &Addr, U64Key), LockupInfoV2>
{
    fn compatible_load(
        &self,
        deps: Deps,
        key: (&Addr, &Addr, U64Key),
        generator: &Option<Addr>,
    ) -> StdResult<LockupInfoV2> {
        self.load(deps.storage, key).or_else(|_| {
            let old_lockup_info = OLD_LOCKUP_INFO.load(deps.storage, key)?;
            let mut generator_proxy_debt = RestrictedVector::default();
            let generator = generator.as_ref().expect("Generator should be set!");

            if !old_lockup_info.generator_proxy_debt.is_zero() {
                let asset = ASSET_POOLS.load(deps.storage, key.0)?;
                let astro_lp = asset
                    .migration_info
                    .expect("Pool should be migrated!")
                    .astroport_lp_token;
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
                astro_rewards: old_lockup_info.astro_rewards,
                generator_astro_debt: old_lockup_info.generator_astro_debt,
                generator_proxy_debt,
                unlock_timestamp: old_lockup_info.unlock_timestamp,
            };

            Ok(lockup_info)
        })
    }

    fn compatible_may_load(
        &self,
        deps: Deps,
        key: (&Addr, &Addr, U64Key),
        generator: &Option<Addr>,
    ) -> StdResult<Option<LockupInfoV2>> {
        if !OLD_LOCKUP_INFO.has(deps.storage, key) {
            return Ok(None);
        }
        Some(self.compatible_load(deps, key, generator)).transpose()
    }
}

pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
