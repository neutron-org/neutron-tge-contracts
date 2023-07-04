use crate::types::{
    Config, MigrationState, OrderBy, VestingInfo, VestingState, XykToClMigrationConfig,
};
use astroport::common::OwnershipProposal;
use cosmwasm_std::{Addr, Deps, StdResult};
use cw_storage_plus::{Bound, Item, Map, SnapshotItem, SnapshotMap, Strategy};

pub(crate) const CONFIG: Item<Config> = Item::new("config");
/// Migration status
pub(crate) const MIGRATION_STATUS: Item<MigrationState> = Item::new("migration_status");
pub(crate) const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
pub(crate) const VESTING_MANAGERS: Map<Addr, ()> = Map::new("vesting_managers");
pub(crate) const VESTING_STATE_OLD: Item<VestingState> = Item::new("vesting_state_old");
pub(crate) const VESTING_STATE: SnapshotItem<VestingState> = SnapshotItem::new(
    "vesting_state",
    "vesting_state__checkpoints",
    "vesting_state__changelog",
    Strategy::Never,
);
pub(crate) const VESTING_INFO: SnapshotMap<Addr, VestingInfo> = SnapshotMap::new(
    "vesting_info",
    "vesting_info__checkpoints",
    "vesting_info__changelog",
    Strategy::Never,
);
pub(crate) const VESTING_STATE_HISTORICAL: SnapshotItem<VestingState> = SnapshotItem::new(
    "vesting_state",
    "vesting_state__checkpoints",
    "vesting_state__changelog",
    Strategy::EveryBlock,
);
pub(crate) const VESTING_INFO_HISTORICAL: SnapshotMap<Addr, VestingInfo> = SnapshotMap::new(
    "vesting_info",
    "vesting_info__checkpoints",
    "vesting_info__changelog",
    Strategy::EveryBlock,
);

pub(crate) fn vesting_state(historical: bool) -> SnapshotItem<'static, VestingState> {
    if historical {
        return VESTING_STATE_HISTORICAL;
    }
    VESTING_STATE
}

pub(crate) fn vesting_info(historical: bool) -> SnapshotMap<'static, Addr, VestingInfo> {
    if historical {
        return VESTING_INFO_HISTORICAL;
    }
    VESTING_INFO
}

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

/// Returns an empty vector if it does not find data, otherwise returns a vector that
/// contains objects of type [`VESTING_INFO`].
/// ## Params
///
/// * **start_after** index from which to start reading vesting schedules.
///
/// * **limit** amount of vesting schedules to read.
///
/// * **order_by** whether results should be returned in an ascending or descending order.
pub(crate) fn read_vesting_infos(
    deps: Deps,
    start_after: Option<Addr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<Vec<(Addr, VestingInfo)>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start_after = start_after.map(Bound::exclusive);

    let (start, end) = match &order_by {
        Some(OrderBy::Asc) => (start_after, None),
        _ => (None, start_after),
    };

    let info: Vec<(Addr, VestingInfo)> = VESTING_INFO
        .range(
            deps.storage,
            start,
            end,
            order_by.unwrap_or(OrderBy::Desc).into(),
        )
        .take(limit)
        .filter_map(|v| v.ok())
        .collect();

    Ok(info)
}

#[cfg(test)]
mod testing {
    use super::*;

    #[test]
    fn read_vesting_infos_as_expected() {
        use cosmwasm_std::{testing::mock_dependencies, Uint128};
        let mut deps = mock_dependencies();
        let historical = false;

        let vi_mock = VestingInfo {
            released_amount: Uint128::zero(),
            schedules: vec![],
        };

        for i in 1..5 {
            let key = Addr::unchecked(format! {"address{}", i});

            vesting_info(historical)
                .save(&mut deps.storage, key, &vi_mock, 1)
                .unwrap();
        }

        let res = read_vesting_infos(
            deps.as_ref(),
            Some(Addr::unchecked("address2")),
            None,
            Some(OrderBy::Asc),
        )
        .unwrap();
        assert_eq!(
            res,
            vec![
                (Addr::unchecked("address3"), vi_mock.clone()),
                (Addr::unchecked("address4"), vi_mock.clone()),
            ]
        );

        let res = read_vesting_infos(
            deps.as_ref(),
            Some(Addr::unchecked("address2")),
            Some(1),
            Some(OrderBy::Asc),
        )
        .unwrap();
        assert_eq!(res, vec![(Addr::unchecked("address3"), vi_mock.clone())]);

        let res = read_vesting_infos(
            deps.as_ref(),
            Some(Addr::unchecked("address3")),
            None,
            Some(OrderBy::Desc),
        )
        .unwrap();
        assert_eq!(
            res,
            vec![
                (Addr::unchecked("address2"), vi_mock.clone()),
                (Addr::unchecked("address1"), vi_mock.clone()),
            ]
        );

        let res = read_vesting_infos(
            deps.as_ref(),
            Some(Addr::unchecked("address3")),
            Some(1),
            Some(OrderBy::Desc),
        )
        .unwrap();
        assert_eq!(res, vec![(Addr::unchecked("address2"), vi_mock.clone())]);
    }
}

pub const XYK_TO_CL_MIGRATION_CONFIG: Item<XykToClMigrationConfig> =
    Item::new("xyk_to_cl_migration_config");
