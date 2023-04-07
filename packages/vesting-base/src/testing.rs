use astroport::vesting::{ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg};

use astroport::asset::token_asset_info;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_binary, Addr};
use cw_storage_plus::Strategy;

use crate::state::BaseVesting;

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies();
    let vest_app = BaseVesting::new(Strategy::Never);

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        token_info_manager: "token_info_manager".to_string(),
        vesting_managers: vec!["manager1".to_string(), "manager2".to_string()],
    };

    let env = mock_env();
    let info = mock_info("addr0000", &[]);
    let _res = vest_app.instantiate(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::SetVestingToken {
        vesting_token: token_asset_info(Addr::unchecked("ntrn_token")),
    };
    let env = mock_env();
    let info = mock_info("token_info_manager", &[]);
    let _res = vest_app
        .execute(deps.as_mut(), env.clone(), info, msg)
        .unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(
            &vest_app
                .query(deps.as_ref(), env.clone(), QueryMsg::Config {})
                .unwrap()
        )
        .unwrap(),
        ConfigResponse {
            owner: Addr::unchecked("owner"),
            token_info_manager: Addr::unchecked("token_info_manager"),
            vesting_token: token_asset_info(Addr::unchecked("ntrn_token")),
        }
    );

    assert_eq!(
        from_binary::<Vec<Addr>>(
            &vest_app
                .query(deps.as_ref(), env, QueryMsg::VestingManagers {})
                .unwrap()
        )
        .unwrap(),
        vec![Addr::unchecked("manager1"), Addr::unchecked("manager2")],
    );
}
