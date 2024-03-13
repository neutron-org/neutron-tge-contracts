use astroport::common::OwnershipProposal;
use astroport_periphery::lockdrop_pcl::{Config, LockupInfo, PoolInfo, PoolType, State, UserInfo};
use astroport_periphery::U64Key;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map, SnapshotMap, Strategy};

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");

/// Key is an Terraswap LP token address
pub const ASSET_POOLS: SnapshotMap<PoolType, PoolInfo> = SnapshotMap::new(
    "LiquidityPools",
    "LiquitidyPools_checkpoints",
    "LiquidityPools_changelog",
    Strategy::EveryBlock,
);
/// Key is an user address
pub const USER_INFO: Map<&Addr, UserInfo> = Map::new("users");
/// Key consists of an Terraswap LP token address, an user address, and a duration
pub const LOCKUP_INFO: Map<(PoolType, &Addr, U64Key), LockupInfo> = Map::new("lockup_position");

pub const TOTAL_USER_LOCKUP_AMOUNT: SnapshotMap<(PoolType, &Addr), Uint128> = SnapshotMap::new(
    "total_user_lockup_info",
    "total_user_lockup_info_checkpoints",
    "total_lockup_info_changelog",
    Strategy::EveryBlock,
);

pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
