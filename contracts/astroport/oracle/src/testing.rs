use crate::contract::{execute, instantiate, query};
use crate::error::ContractError;
use crate::mock_querier::mock_dependencies;
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::oracle::{Config, ExecuteMsg, InstantiateMsg, QueryMsg};
use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_json, Addr, Decimal256, DepsMut, Env, MessageInfo, Uint128, Uint256, Uint64,
};
use std::ops::{Add, Mul};

#[test]
fn decimal_overflow() {
    let price_cumulative_current = Uint128::from(100u128);
    let price_cumulative_last = Uint128::from(192738282u128);
    let time_elapsed: u64 = 86400;
    let amount = Uint128::from(1000u128);
    let price_average = Decimal256::from_ratio(
        Uint256::from(price_cumulative_current.wrapping_sub(price_cumulative_last)),
        time_elapsed,
    );

    println!("{}", price_average);

    let res: Uint128 = price_average.mul(Uint256::from(amount)).try_into().unwrap();
    println!("{}", res);
}

#[test]
fn oracle_overflow() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);

    let mut env = mock_env();
    let factory = Addr::unchecked("factory");
    let astro_token_contract = Addr::unchecked("astro-token");
    let usdc_token_contract = Addr::unchecked("usdc-token");

    let astro_asset_info = AssetInfo::Token {
        contract_addr: astro_token_contract,
    };
    let usdc_asset_info = AssetInfo::Token {
        contract_addr: usdc_token_contract,
    };
    let astro_asset = Asset {
        info: astro_asset_info.clone(),
        amount: Uint128::zero(),
    };
    let usdc_asset = Asset {
        info: usdc_asset_info.clone(),
        amount: Uint128::zero(),
    };

    let asset = vec![astro_asset, usdc_asset];

    let instantiate_msg = InstantiateMsg {
        factory_contract: factory.to_string(),
        period: 1,
        manager: String::from("manager"),
    };

    // Set cumulative price to 192738282u128
    deps.querier.set_cumulative_price(
        Addr::unchecked("pair"),
        asset.clone(),
        Uint128::from(192738282u128),
        vec![
            (
                asset[0].info.clone(),
                asset[1].info.clone(),
                Uint128::from(192738282u128),
            ),
            (
                asset[1].info.clone(),
                asset[0].info.clone(),
                Uint128::from(192738282u128),
            ),
        ],
    );
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());
    execute(
        deps.as_mut(),
        env.clone(),
        mock_info("manager", &[]),
        ExecuteMsg::SetAssetInfos(vec![astro_asset_info, usdc_asset_info]),
    )
    .unwrap();
    // Set cumulative price to 100 (overflow)
    deps.querier.set_cumulative_price(
        Addr::unchecked("pair"),
        asset.clone(),
        Uint128::from(100u128),
        vec![
            (
                asset[0].info.clone(),
                asset[1].info.clone(),
                Uint128::from(100u128),
            ),
            (
                asset[1].info.clone(),
                asset[0].info.clone(),
                Uint128::from(100u128),
            ),
        ],
    );
    env.block.time = env.block.time.plus_seconds(86400);
    execute(deps.as_mut(), env, info, ExecuteMsg::Update {}).unwrap();
}

#[test]
fn update_does_not_work_without_pair_info() {
    let mut deps = mock_dependencies(&[]);
    let env = mock_env();
    setup(deps.as_mut(), env.clone(), mock_info("dao", &[]));

    for caller in ["someone", "dao"] {
        let res = execute(
            deps.as_mut(),
            env.clone(),
            mock_info(caller, &[]),
            ExecuteMsg::Update {},
        )
        .unwrap_err();
        assert_eq!(res, ContractError::AssetInfosNotSet {});
    }
}

#[test]
fn update_period_works_without_pair_info() {
    let mut deps = mock_dependencies(&[]);
    let env = mock_env();
    setup(deps.as_mut(), env.clone(), mock_info("dao", &[]));

    execute(
        deps.as_mut(),
        env,
        mock_info("dao", &[]),
        ExecuteMsg::UpdatePeriod { new_period: 0 },
    )
    .unwrap();
}

#[test]
fn cfg_and_last_update_ts() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);

    let mut env = mock_env();
    let factory = Addr::unchecked("factory");
    let astro_token_contract = Addr::unchecked("astro-token");
    let usdc_token_contract = Addr::unchecked("usdc-token");

    let astro_asset_info = AssetInfo::Token {
        contract_addr: astro_token_contract,
    };
    let usdc_asset_info = AssetInfo::Token {
        contract_addr: usdc_token_contract,
    };
    let astro_asset = Asset {
        info: astro_asset_info.clone(),
        amount: Uint128::zero(),
    };
    let usdc_asset = Asset {
        info: usdc_asset_info.clone(),
        amount: Uint128::zero(),
    };

    let asset = vec![astro_asset, usdc_asset];

    let instantiate_msg = InstantiateMsg {
        factory_contract: factory.to_string(),
        manager: String::from("manager"),
        period: 100,
    };

    // Set cumulative price to 192738282u128
    deps.querier.set_cumulative_price(
        Addr::unchecked("pair"),
        asset.clone(),
        Uint128::from(192738282u128),
        vec![
            (
                asset[0].info.clone(),
                asset[1].info.clone(),
                Uint128::from(192738282u128),
            ),
            (
                asset[1].info.clone(),
                asset[0].info.clone(),
                Uint128::from(192738282u128),
            ),
        ],
    );
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());
    execute(
        deps.as_mut(),
        env.clone(),
        mock_info("manager", &[]),
        ExecuteMsg::SetAssetInfos(vec![astro_asset_info, usdc_asset_info]),
    )
    .unwrap();
    assert_eq!(0, res.messages.len());

    // make sure config query works as expected
    let cfg: Config =
        from_json(query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!(
        cfg,
        Config {
            owner: Addr::unchecked("addr0000"),
            factory: Addr::unchecked("factory"),
            asset_infos: Some(vec![
                AssetInfo::Token {
                    contract_addr: Addr::unchecked("astro-token")
                },
                AssetInfo::Token {
                    contract_addr: Addr::unchecked("usdc-token")
                },
            ]),
            pair: Some(PairInfo {
                asset_infos: vec![
                    AssetInfo::Token {
                        contract_addr: Addr::unchecked("astro-token")
                    },
                    AssetInfo::Token {
                        contract_addr: Addr::unchecked("usdc-token")
                    },
                ],
                contract_addr: Addr::unchecked("pair"),
                liquidity_token: Addr::unchecked("lp_token"),
                pair_type: astroport::factory::PairType::Xyk {}
            }),
            period: 100u64,
            manager: Addr::unchecked("manager")
        }
    );
    let e = query(deps.as_ref(), env.clone(), QueryMsg::LastUpdateTimestamp {}).unwrap_err();
    assert_eq!(e, ContractError::PricesNotFound {});

    // make first update and check how last update height works
    let first_update_ts = env.block.time.seconds();
    execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Update {},
    )
    .unwrap();
    let last_update_ts: u64 =
        from_json(query(deps.as_ref(), env.clone(), QueryMsg::LastUpdateTimestamp {}).unwrap())
            .unwrap();
    assert_eq!(last_update_ts, first_update_ts);
    env.block.time = env.block.time.plus_seconds(100);
    execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Update {},
    )
    .unwrap();
    let last_update_ts: u64 =
        from_json(query(deps.as_ref(), env.clone(), QueryMsg::LastUpdateTimestamp {}).unwrap())
            .unwrap();
    assert_eq!(last_update_ts, first_update_ts.add(100));

    // increase update period
    execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::UpdatePeriod { new_period: 500u64 },
    )
    .unwrap();
    let cfg: Config =
        from_json(query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!(cfg.period, 500u64);

    // make sure premature update doesn't work
    let last_update_ts: u64 =
        from_json(query(deps.as_ref(), env.clone(), QueryMsg::LastUpdateTimestamp {}).unwrap())
            .unwrap();
    assert_eq!(last_update_ts, first_update_ts.add(100));
    env.block.time = env.block.time.plus_seconds(100);
    execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        ExecuteMsg::Update {},
    )
    .unwrap_err();

    // make sure update works at the right time
    env.block.time = env.block.time.plus_seconds(500);
    execute(deps.as_mut(), env.clone(), info, ExecuteMsg::Update {}).unwrap();
    let last_update_ts: u64 =
        from_json(query(deps.as_ref(), env, QueryMsg::LastUpdateTimestamp {}).unwrap()).unwrap();
    assert_eq!(last_update_ts, first_update_ts.add(700));
}

fn setup(deps: DepsMut, env: Env, info: MessageInfo) {
    instantiate(
        deps,
        env,
        info,
        InstantiateMsg {
            factory_contract: String::from("factory"),
            period: 0,
            manager: String::from("manager"),
        },
    )
    .unwrap();
}

#[test]
fn queries_do_not_work_without_prices_calculated() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);

    let mut env = mock_env();
    let factory = Addr::unchecked("factory");
    let astro_token_contract = Addr::unchecked("astro-token");
    let usdc_token_contract = Addr::unchecked("usdc-token");

    deps.querier.with_token_balances(&[
        (
            &astro_token_contract.to_string(),
            &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(10000))],
        ),
        (
            &usdc_token_contract.to_string(),
            &[(&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(10000))],
        ),
    ]);

    let astro_asset_info = AssetInfo::Token {
        contract_addr: astro_token_contract.clone(),
    };
    let usdc_asset_info = AssetInfo::Token {
        contract_addr: usdc_token_contract.clone(),
    };
    let astro_asset = Asset {
        info: astro_asset_info.clone(),
        amount: Uint128::zero(),
    };
    let usdc_asset = Asset {
        info: usdc_asset_info.clone(),
        amount: Uint128::zero(),
    };

    let asset = vec![astro_asset, usdc_asset];

    let instantiate_msg = InstantiateMsg {
        factory_contract: factory.to_string(),
        period: 1,
        manager: String::from("manager"),
    };

    // Set cumulative price to 192738282u128
    deps.querier.set_cumulative_price(
        Addr::unchecked("pair"),
        asset.clone(),
        Uint128::from(192738282u128),
        vec![
            (
                asset[0].info.clone(),
                asset[1].info.clone(),
                Uint128::from(192738282u128),
            ),
            (
                asset[1].info.clone(),
                asset[0].info.clone(),
                Uint128::from(192738282u128),
            ),
        ],
    );
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());
    execute(
        deps.as_mut(),
        env.clone(),
        mock_info("manager", &[]),
        ExecuteMsg::SetAssetInfos(vec![astro_asset_info, usdc_asset_info]),
    )
    .unwrap();

    // no update called — no queries available
    let e = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::Consult {
            token: AssetInfo::Token {
                contract_addr: usdc_token_contract.clone(),
            },
            amount: Uint128::from(100u128),
        },
    )
    .unwrap_err();
    assert_eq!(e, ContractError::PricesNotFound {});
    let e = query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::TWAPAtHeight {
            token: AssetInfo::Token {
                contract_addr: astro_token_contract.clone(),
            },
            height: Uint64::from(env.block.height),
        },
    )
    .unwrap_err();
    assert_eq!(e, ContractError::PricesNotFound {});

    // call update and make sure queries work
    env.block.time = env.block.time.plus_seconds(5);
    env.block.height += 1;
    execute(deps.as_mut(), env.clone(), info, ExecuteMsg::Update {}).unwrap();
    env.block.time = env.block.time.plus_seconds(5);
    env.block.height += 1;
    query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::Consult {
            token: AssetInfo::Token {
                contract_addr: usdc_token_contract,
            },
            amount: Uint128::from(100u128),
        },
    )
    .unwrap();
    query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::TWAPAtHeight {
            token: AssetInfo::Token {
                contract_addr: astro_token_contract,
            },
            height: Uint64::from(env.block.height),
        },
    )
    .unwrap();
}

#[test]
fn set_asset_infos_works_only_once() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);

    let env = mock_env();
    let factory = Addr::unchecked("factory");
    let astro_token_contract = Addr::unchecked("astro-token");
    let usdc_token_contract = Addr::unchecked("usdc-token");

    let astro_asset_info = AssetInfo::Token {
        contract_addr: astro_token_contract,
    };
    let usdc_asset_info = AssetInfo::Token {
        contract_addr: usdc_token_contract,
    };

    let instantiate_msg = InstantiateMsg {
        factory_contract: factory.to_string(),
        period: 1,
        manager: String::from("manager"),
    };

    let res = instantiate(deps.as_mut(), env.clone(), info, instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());
    execute(
        deps.as_mut(),
        env.clone(),
        mock_info("manager", &[]),
        ExecuteMsg::SetAssetInfos(vec![astro_asset_info.clone(), usdc_asset_info.clone()]),
    )
    .unwrap();

    let err = execute(
        deps.as_mut(),
        env,
        mock_info("manager", &[]),
        ExecuteMsg::SetAssetInfos(vec![astro_asset_info, usdc_asset_info]),
    )
    .unwrap_err();
    assert_eq!(err, ContractError::AssetInfosAlreadySet {});
}
