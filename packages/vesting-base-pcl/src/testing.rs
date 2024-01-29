use crate::builder::VestingBaseBuilder;
use crate::error::{ext_unsupported_err, ContractError};
use crate::handlers::{execute, query};
use crate::msg::{
    ExecuteMsg, ExecuteMsgManaged, QueryMsg, QueryMsgHistorical, QueryMsgWithManagers,
};
use crate::types::{Config, Extensions};
use astroport::asset::token_asset_info;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_binary, Addr};

#[test]
fn set_vesting_token() {
    let mut deps = mock_dependencies();
    let owner = String::from("owner");
    let token_info_manager = "token_info_manager";
    let env = mock_env();
    VestingBaseBuilder::default()
        .build(deps.as_mut(), owner, String::from(token_info_manager))
        .unwrap();

    // check initialisation
    assert_eq!(
        from_binary::<Config>(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap())
            .unwrap(),
        Config {
            owner: Addr::unchecked("owner"),
            token_info_manager: Addr::unchecked(token_info_manager),
            vesting_token: None,
            extensions: Extensions {
                historical: false,
                managed: false,
                with_managers: false
            }
        }
    );

    let info = mock_info("stranger", &[]);
    // set vesting token by a stranger -> Unauthorized
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::SetVestingToken {
                vesting_token: token_asset_info(Addr::unchecked("ntrn_token")),
            },
        )
        .unwrap_err(),
        ContractError::Unauthorized {},
    );

    // set vesting token by the manager -> Success
    let info = mock_info("token_info_manager", &[]);
    execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::SetVestingToken {
            vesting_token: token_asset_info(Addr::unchecked("ntrn_token")),
        },
    )
    .unwrap();

    assert_eq!(
        from_binary::<Config>(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap())
            .unwrap(),
        Config {
            owner: Addr::unchecked("owner"),
            token_info_manager: Addr::unchecked(token_info_manager),
            vesting_token: Some(token_asset_info(Addr::unchecked("ntrn_token"))),
            extensions: Extensions {
                historical: false,
                managed: false,
                with_managers: false
            }
        }
    );

    // set vesting token second time by the owner -> VestingTokenAlreadySet
    assert_eq!(
        execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::SetVestingToken {
                vesting_token: token_asset_info(Addr::unchecked("not_a_ntrn_token")),
            },
        )
        .unwrap_err(),
        ContractError::VestingTokenAlreadySet {},
    );

    assert_eq!(
        from_binary::<Config>(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap()).unwrap(),
        Config {
            owner: Addr::unchecked("owner"),
            token_info_manager: Addr::unchecked(token_info_manager),
            vesting_token: Some(token_asset_info(Addr::unchecked("ntrn_token"))),
            extensions: Extensions {
                historical: false,
                managed: false,
                with_managers: false
            }
        }
    );
}

#[test]
fn proper_building_standard() {
    let mut deps = mock_dependencies();
    let owner = String::from("owner");
    let token_info_manager = "token_info_manager";
    let env = mock_env();
    let info = mock_info("owner", &[]);
    VestingBaseBuilder::default()
        .build(deps.as_mut(), owner, String::from(token_info_manager))
        .unwrap();

    // check initialisation
    assert_eq!(
        from_binary::<Config>(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap())
            .unwrap(),
        Config {
            owner: Addr::unchecked("owner"),
            token_info_manager: Addr::unchecked(token_info_manager),
            vesting_token: None,
            extensions: Extensions {
                historical: false,
                managed: false,
                with_managers: false
            }
        }
    );

    // make sure with_managers extension is not enabled
    assert_eq!(
        query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::WithManagersExtension {
                msg: QueryMsgWithManagers::VestingManagers {}
            }
        )
        .unwrap_err(),
        ext_unsupported_err("with_managers")
    );

    // make sure historical extension is not enabled
    assert_eq!(
        query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::HistoricalExtension {
                msg: QueryMsgHistorical::UnclaimedTotalAmountAtHeight { height: 1000u64 }
            }
        )
        .unwrap_err(),
        ext_unsupported_err("historical")
    );

    // make sure managed extension is not enabled
    assert_eq!(
        execute(
            deps.as_mut(),
            env,
            info,
            ExecuteMsg::ManagedExtension {
                msg: ExecuteMsgManaged::RemoveVestingAccounts {
                    vesting_accounts: vec![],
                    clawback_account: String::from("clawback")
                }
            },
        )
        .unwrap_err(),
        ext_unsupported_err("managed").into()
    );
}

#[test]
fn proper_building_managers() {
    let mut deps = mock_dependencies();
    let owner = String::from("owner");
    let token_info_manager = "token_info_manager";
    let env = mock_env();
    let info = mock_info("owner", &[]);
    let vesting_managers = vec!["manager1".to_string(), "manager2".to_string()];
    VestingBaseBuilder::default()
        .with_managers(vesting_managers.clone())
        .build(deps.as_mut(), owner, String::from(token_info_manager))
        .unwrap();

    // check initialisation
    assert_eq!(
        from_binary::<Config>(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap())
            .unwrap(),
        Config {
            owner: Addr::unchecked("owner"),
            token_info_manager: Addr::unchecked(token_info_manager),
            vesting_token: None,
            extensions: Extensions {
                historical: false,
                managed: false,
                with_managers: true
            }
        }
    );

    // make sure with_managers extension is enabled
    assert_eq!(
        from_binary::<Vec<String>>(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::WithManagersExtension {
                    msg: QueryMsgWithManagers::VestingManagers {},
                },
            )
            .unwrap()
        )
        .unwrap(),
        vesting_managers
    );

    // make sure historical extension is not enabled
    assert_eq!(
        query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::HistoricalExtension {
                msg: QueryMsgHistorical::UnclaimedTotalAmountAtHeight { height: 1000u64 }
            }
        )
        .unwrap_err(),
        ext_unsupported_err("historical")
    );

    // make sure managed extension is not enabled
    assert_eq!(
        execute(
            deps.as_mut(),
            env,
            info,
            ExecuteMsg::ManagedExtension {
                msg: ExecuteMsgManaged::RemoveVestingAccounts {
                    vesting_accounts: vec![],
                    clawback_account: String::from("clawback"),
                },
            },
        )
        .unwrap_err(),
        ext_unsupported_err("managed").into()
    );
}

#[test]
fn proper_building_historical() {
    let mut deps = mock_dependencies();
    let owner = String::from("owner");
    let token_info_manager = "token_info_manager";
    let env = mock_env();
    let info = mock_info("owner", &[]);
    VestingBaseBuilder::default()
        .historical()
        .build(deps.as_mut(), owner, String::from(token_info_manager))
        .unwrap();

    // check initialisation
    assert_eq!(
        from_binary::<Config>(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap())
            .unwrap(),
        Config {
            owner: Addr::unchecked("owner"),
            token_info_manager: Addr::unchecked(token_info_manager),
            vesting_token: None,
            extensions: Extensions {
                historical: true,
                managed: false,
                with_managers: false
            }
        }
    );

    // make sure with_managers extension is not enabled
    assert_eq!(
        query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::WithManagersExtension {
                msg: QueryMsgWithManagers::VestingManagers {}
            }
        )
        .unwrap_err(),
        ext_unsupported_err("with_managers")
    );

    // make sure historical extension is enabled
    query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::HistoricalExtension {
            msg: QueryMsgHistorical::UnclaimedTotalAmountAtHeight { height: 1000u64 },
        },
    )
    .unwrap();

    // make sure managed extension is not enabled
    assert_eq!(
        execute(
            deps.as_mut(),
            env,
            info,
            ExecuteMsg::ManagedExtension {
                msg: ExecuteMsgManaged::RemoveVestingAccounts {
                    vesting_accounts: vec![],
                    clawback_account: String::from("clawback")
                }
            },
        )
        .unwrap_err(),
        ext_unsupported_err("managed").into()
    );
}

#[test]
fn proper_building_managed() {
    let mut deps = mock_dependencies();
    let owner = String::from("owner");
    let token_info_manager = "token_info_manager";
    let env = mock_env();
    VestingBaseBuilder::default()
        .managed()
        .build(deps.as_mut(), owner, String::from(token_info_manager))
        .unwrap();

    // check initialisation and set vesting token
    assert_eq!(
        from_binary::<Config>(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap())
            .unwrap(),
        Config {
            owner: Addr::unchecked("owner"),
            token_info_manager: Addr::unchecked(token_info_manager),
            vesting_token: None,
            extensions: Extensions {
                historical: false,
                managed: true,
                with_managers: false
            }
        }
    );
    let info = mock_info("token_info_manager", &[]);
    execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::SetVestingToken {
            vesting_token: token_asset_info(Addr::unchecked("ntrn_token")),
        },
    )
    .unwrap();

    // make sure with_managers extension is not enabled
    assert_eq!(
        query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::WithManagersExtension {
                msg: QueryMsgWithManagers::VestingManagers {}
            }
        )
        .unwrap_err(),
        ext_unsupported_err("with_managers")
    );

    // make sure historical extension is not enabled
    assert_eq!(
        query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::HistoricalExtension {
                msg: QueryMsgHistorical::UnclaimedTotalAmountAtHeight { height: 1000u64 }
            }
        )
        .unwrap_err(),
        ext_unsupported_err("historical")
    );

    // make sure managed extension is enabled
    let info = mock_info("owner", &[]);
    execute(
        deps.as_mut(),
        env,
        info,
        ExecuteMsg::ManagedExtension {
            msg: ExecuteMsgManaged::RemoveVestingAccounts {
                vesting_accounts: vec![],
                clawback_account: String::from("clawback"),
            },
        },
    )
    .unwrap();
}

#[test]
fn proper_building_all_extensions() {
    let mut deps = mock_dependencies();
    let owner = String::from("owner");
    let token_info_manager = "token_info_manager";
    let env = mock_env();
    let vesting_managers = vec!["manager1".to_string(), "manager2".to_string()];
    VestingBaseBuilder::default()
        .historical()
        .managed()
        .with_managers(vesting_managers.clone())
        .build(deps.as_mut(), owner, String::from(token_info_manager))
        .unwrap();

    // check initialisation and set vesting token
    assert_eq!(
        from_binary::<Config>(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap())
            .unwrap(),
        Config {
            owner: Addr::unchecked("owner"),
            token_info_manager: Addr::unchecked(token_info_manager),
            vesting_token: None,
            extensions: Extensions {
                historical: true,
                managed: true,
                with_managers: true
            }
        }
    );
    let info = mock_info("token_info_manager", &[]);
    execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::SetVestingToken {
            vesting_token: token_asset_info(Addr::unchecked("ntrn_token")),
        },
    )
    .unwrap();

    // make sure with_managers extension is enabled
    assert_eq!(
        from_binary::<Vec<String>>(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::WithManagersExtension {
                    msg: QueryMsgWithManagers::VestingManagers {},
                },
            )
            .unwrap()
        )
        .unwrap(),
        vesting_managers
    );

    // make sure historical extension is enabled
    query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::HistoricalExtension {
            msg: QueryMsgHistorical::UnclaimedTotalAmountAtHeight { height: 1000u64 },
        },
    )
    .unwrap();

    // make sure managed extension is enabled
    let info = mock_info("owner", &[]);
    execute(
        deps.as_mut(),
        env,
        info,
        ExecuteMsg::ManagedExtension {
            msg: ExecuteMsgManaged::RemoveVestingAccounts {
                vesting_accounts: vec![],
                clawback_account: String::from("clawback"),
            },
        },
    )
    .unwrap();
}
