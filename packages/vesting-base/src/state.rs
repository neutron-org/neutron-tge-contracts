use cosmwasm_schema::cw_serde;

use astroport::asset::AssetInfo;
use astroport::common::OwnershipProposal;
use astroport::vesting::{OrderBy, VestingInfo, VestingState};
use cosmwasm_std::{Addr, Deps, StdResult};
use cw_storage_plus::{Bound, Item, SnapshotItem, SnapshotMap, Strategy};

pub struct BaseVesting {
    /// Stores the total granted/claimed amount of tokens
    pub vesting_state: SnapshotItem<'static, VestingState>,
    /// The first key is the address of an account that's vesting, the second key is an object of type [`VestingInfo`].
    pub vesting_info: SnapshotMap<'static, &'static Addr, VestingInfo>,
    /// Stores the contract config at the given key.
    pub config: Item<'static, Config>,
    /// Contains a proposal to change contract ownership.
    pub ownership_proposal: Item<'static, OwnershipProposal>,
}

impl BaseVesting {
    pub fn new(snapshot_strategy: Strategy) -> Self {
        BaseVesting {
            vesting_state: SnapshotItem::new(
                "vesting_state",
                "vesting_state__checkpoints",
                "vesting_state__changelog",
                snapshot_strategy,
            ),
            vesting_info: SnapshotMap::new(
                "vesting_info",
                "vesting_info__checkpoints",
                "vesting_info__changelog",
                snapshot_strategy,
            ),
            config: Item::new("config"),
            ownership_proposal: Item::new("ownership_proposal"),
        }
    }
}

/// This structure stores the main parameters for the generator vesting contract.
#[cw_serde]
pub struct Config {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// [`AssetInfo`] of the ASTRO token
    pub vesting_token: AssetInfo,
}

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

impl BaseVesting {
    /// Returns an empty vector if it does not find data, otherwise returns a vector that
    /// contains objects of type [`VESTING_INFO`].
    /// ## Params
    ///
    /// * **start_after** index from which to start reading vesting schedules.
    ///
    /// * **limit** amount of vesting schedules to read.
    ///
    /// * **order_by** whether results should be returned in an ascending or descending order.
    pub fn read_vesting_infos(
        &self,
        deps: Deps,
        start_after: Option<Addr>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    ) -> StdResult<Vec<(Addr, VestingInfo)>> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
        let start_after = start_after.as_ref().map(Bound::exclusive);

        let (start, end) = match &order_by {
            Some(OrderBy::Asc) => (start_after, None),
            _ => (None, start_after),
        };

        let info: Vec<(Addr, VestingInfo)> = self
            .vesting_info
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
}

#[cfg(test)]
mod testing {
    use super::*;

    #[test]
    fn read_vesting_infos_as_expected() {
        use cosmwasm_std::{testing::mock_dependencies, Uint128};
        let vest_app = BaseVesting::new(Strategy::Never);

        let mut deps = mock_dependencies();

        let vi_mock = VestingInfo {
            released_amount: Uint128::zero(),
            schedules: vec![],
        };

        for i in 1..5 {
            let key = Addr::unchecked(format! {"address{}", i});

            vest_app
                .vesting_info
                .save(&mut deps.storage, &key, &vi_mock, 1)
                .unwrap();
        }

        let res = vest_app
            .read_vesting_infos(
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

        let res = vest_app
            .read_vesting_infos(
                deps.as_ref(),
                Some(Addr::unchecked("address2")),
                Some(1),
                Some(OrderBy::Asc),
            )
            .unwrap();
        assert_eq!(res, vec![(Addr::unchecked("address3"), vi_mock.clone())]);

        let res = vest_app
            .read_vesting_infos(
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

        let res = vest_app
            .read_vesting_infos(
                deps.as_ref(),
                Some(Addr::unchecked("address3")),
                Some(1),
                Some(OrderBy::Desc),
            )
            .unwrap();
        assert_eq!(res, vec![(Addr::unchecked("address2"), vi_mock.clone())]);
    }
}
