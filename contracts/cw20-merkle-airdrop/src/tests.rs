use crate::{
    contract::{execute, instantiate, query, NEUTRON_DENOM},
    error::ContractError,
    msg::{
        ConfigResponse, ExecuteMsg, InstantiateMsg, IsClaimedResponse, MerkleRootResponse,
        QueryMsg, SignatureInfo, TotalClaimedResponse,
    },
};
use cosmwasm_std::{
    attr, coin, from_binary, from_slice,
    testing::{mock_dependencies, mock_env, mock_info},
    to_binary, Addr, BlockInfo, CosmosMsg, Empty, SubMsg, Timestamp, Uint128, WasmMsg,
};
use credits::msg::ExecuteMsg::AddVesting;
use cw20::{BalanceResponse, Cw20ExecuteMsg};
use cw_multi_test::{App, BankKeeper, Contract, ContractWrapper, Executor};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn mock_app() -> App {
    App::default()
}

pub fn contract_merkle_airdrop() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(execute, instantiate, query))
}

pub fn contract_credits() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        credits::contract::execute,
        credits::contract::instantiate,
        credits::contract::query,
    );
    Box::new(contract)
}

#[test]
fn proper_instantiation() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let airdrop_start = env.block.time.plus_seconds(5_000).seconds();
    let vesting_start = env.block.time.plus_seconds(10_000).seconds();
    let vesting_duration_seconds = 20_000;

    let msg = InstantiateMsg {
        credits_address: "credits0000".to_string(),
        reserve_address: "reserve0000".to_string(),
        merkle_root: "634de21cde1044f41d90373733b0f0fb1c1c71f9652b905cdf159e73c4cf0d37".to_string(),
        airdrop_start,
        vesting_start,
        vesting_duration_seconds,
        total_amount: None,
        hrp: None,
    };

    let info = mock_info("owner0000", &[]);
    let res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "instantiate"),
            attr(
                "merkle_root",
                "634de21cde1044f41d90373733b0f0fb1c1c71f9652b905cdf159e73c4cf0d37",
            ),
            attr("total_amount", "0"),
        ]
    );

    let res = query(deps.as_ref(), env.clone(), QueryMsg::MerkleRoot {}).unwrap();
    let merkle_root: MerkleRootResponse = from_binary(&res).unwrap();
    assert_eq!(
        merkle_root,
        MerkleRootResponse {
            merkle_root: "634de21cde1044f41d90373733b0f0fb1c1c71f9652b905cdf159e73c4cf0d37"
                .to_string(),
            airdrop_start,
            vesting_start,
            vesting_duration_seconds,
            total_amount: Uint128::zero(),
        }
    );

    let res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(
        config,
        ConfigResponse {
            owner: "owner0000".to_string(),
            credits_address: "credits0000".to_string(),
            reserve_address: "reserve0000".to_string(),
        }
    );
}

const TEST_DATA_1: &[u8] = include_bytes!("../testdata/airdrop_test_data.json");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
struct Encoded {
    account: String,
    amount: Uint128,
    root: String,
    proofs: Vec<String>,
    signed_msg: Option<SignatureInfo>,
    hrp: Option<String>,
}

#[test]
fn claim() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let airdrop_start = env.block.time.minus_seconds(5_000).seconds();
    let vesting_start = env.block.time.plus_seconds(10_000).seconds();
    let vesting_duration_seconds = 20_000;
    let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();

    let msg = InstantiateMsg {
        credits_address: "credits0000".to_string(),
        reserve_address: "reserve0000".to_string(),
        merkle_root: test_data.root,
        airdrop_start,
        vesting_start,
        vesting_duration_seconds,
        total_amount: None,
        hrp: None,
    };

    let info = mock_info("owner0000", &[]);
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::Claim {
        amount: test_data.amount,
        proof: test_data.proofs,
    };

    let env = mock_env();
    let info = mock_info(test_data.account.as_str(), &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
    let expected = vec![
        SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "credits0000".to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: test_data.account.clone(),
                amount: test_data.amount,
            })
            .unwrap(),
        })),
        SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "credits0000".to_string(),
            msg: to_binary(&AddVesting {
                address: test_data.account.clone(),
                amount: test_data.amount,
                start_time: vesting_start,
                duration: vesting_duration_seconds,
            })
            .unwrap(),
            funds: vec![],
        })),
    ];
    assert_eq!(res.messages, expected);

    assert_eq!(
        res.attributes,
        vec![
            attr("action", "claim"),
            attr("address", test_data.account.clone()),
            attr("amount", test_data.amount),
        ]
    );

    // Check total claimed
    assert_eq!(
        from_binary::<TotalClaimedResponse>(
            &query(deps.as_ref(), env.clone(), QueryMsg::TotalClaimed {},).unwrap()
        )
        .unwrap()
        .total_claimed,
        test_data.amount
    );

    // Check address is claimed
    assert!(
        from_binary::<IsClaimedResponse>(
            &query(
                deps.as_ref(),
                env.clone(),
                QueryMsg::IsClaimed {
                    address: test_data.account,
                },
            )
            .unwrap()
        )
        .unwrap()
        .is_claimed
    );

    // check error on double claim
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::Claimed {});
}

const TEST_DATA_1_MULTI: &[u8] = include_bytes!("../testdata/airdrop_test_multi_data.json");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
struct Proof {
    account: String,
    amount: Uint128,
    proofs: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct MultipleData {
    total_claimed_amount: Uint128,
    root: String,
    accounts: Vec<Proof>,
}

#[test]
fn multiple_claim() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let airdrop_start = env.block.time.minus_seconds(5_000).seconds();
    let vesting_start = env.block.time.plus_seconds(10_000).seconds();
    let vesting_duration_seconds = 20_000;
    let test_data: MultipleData = from_slice(TEST_DATA_1_MULTI).unwrap();

    let msg = InstantiateMsg {
        credits_address: "credits0000".to_string(),
        reserve_address: "reserve0000".to_string(),
        merkle_root: test_data.root,
        airdrop_start,
        vesting_start,
        vesting_duration_seconds,
        total_amount: None,
        hrp: None,
    };

    let info = mock_info("owner0000", &[]);
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // Loop accounts and claim
    for account in test_data.accounts.iter() {
        let msg = ExecuteMsg::Claim {
            amount: account.amount,
            proof: account.proofs.clone(),
        };

        let env = mock_env();
        let info = mock_info(account.account.as_str(), &[]);
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
        let expected = vec![
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "credits0000".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: account.account.clone(),
                    amount: account.amount,
                })
                .unwrap(),
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "credits0000".to_string(),
                msg: to_binary(&AddVesting {
                    address: account.account.clone(),
                    amount: account.amount,
                    start_time: vesting_start,
                    duration: vesting_duration_seconds,
                })
                .unwrap(),
                funds: vec![],
            })),
        ];
        assert_eq!(res.messages, expected);

        assert_eq!(
            res.attributes,
            vec![
                attr("action", "claim"),
                attr("address", account.account.clone()),
                attr("amount", account.amount),
            ]
        );
    }

    // Check total claimed
    let env = mock_env();
    assert_eq!(
        from_binary::<TotalClaimedResponse>(
            &query(deps.as_ref(), env, QueryMsg::TotalClaimed {}).unwrap()
        )
        .unwrap()
        .total_claimed,
        test_data.total_claimed_amount
    );
}

// Check expiration.
#[test]
fn expiration() {
    let mut deps = mock_dependencies();
    let mut env = mock_env();
    let airdrop_start = env.block.time.minus_seconds(5_000).seconds();
    let vesting_start = env.block.time.plus_seconds(10_000).seconds();
    let vesting_duration_seconds = 20_000;
    let info = mock_info("owner0000", &[]);

    let msg = InstantiateMsg {
        credits_address: "credits0000".to_string(),
        reserve_address: "reserve0000".to_string(),
        merkle_root: "5d4f48f147cb6cb742b376dce5626b2a036f69faec10cd73631c791780e150fc".to_string(),
        airdrop_start,
        vesting_start,
        vesting_duration_seconds,
        total_amount: None,
        hrp: None,
    };
    let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    // make expired
    env.block.time = env.block.time.plus_seconds(40_000);

    // can't claim expired
    let msg = ExecuteMsg::Claim {
        amount: Uint128::new(5),
        proof: vec![],
    };

    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(
        res,
        ContractError::Expired {
            expiration: vesting_start + vesting_duration_seconds
        }
    )
}

#[test]
fn update_reserve_address() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let airdrop_start = env.block.time.minus_seconds(5_000).seconds();
    let vesting_start = env.block.time.plus_seconds(10_000).seconds();
    let vesting_duration_seconds = 20_000;
    let info = mock_info("owner0000", &[]);

    let msg = InstantiateMsg {
        credits_address: "credits0000".to_string(),
        reserve_address: "reserve0000".to_string(),
        merkle_root: "5d4f48f147cb6cb742b376dce5626b2a036f69faec10cd73631c791780e150fc".to_string(),
        airdrop_start,
        vesting_start,
        vesting_duration_seconds,
        total_amount: None,
        hrp: None,
    };
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    // can't claim expired
    let msg = ExecuteMsg::UpdateReserve {
        address: "reserve0001".to_string(),
    };

    let res = execute(
        deps.as_mut(),
        env.clone(),
        mock_info("regularaddress", &[]),
        msg.clone(),
    )
    .unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});

    let res = execute(
        deps.as_mut(),
        env.clone(),
        mock_info("owner0000", &[]),
        msg.clone(),
    )
    .unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "update_reserve"),
            attr("address", "reserve0001"),
        ]
    );

    // old reserve is unauthorized now
    let res = execute(
        deps.as_mut(),
        env.clone(),
        mock_info("reserve0000", &[]),
        msg,
    )
    .unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});

    let res = execute(
        deps.as_mut(),
        env.clone(),
        mock_info("reserve0001", &[]),
        ExecuteMsg::UpdateReserve {
            address: "reserve0002".to_string(),
        },
    )
    .unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "update_reserve"),
            attr("address", "reserve0002"),
        ]
    );

    assert_eq!(
        from_binary::<ConfigResponse>(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap())
            .unwrap()
            .reserve_address,
        "reserve0002"
    );
}

#[test]
fn withdraw_all() {
    let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();
    let mut router = mock_app();
    router
        .init_modules(|router, _api, storage| {
            router.bank = BankKeeper::new();
            router.bank.init_balance(
                storage,
                &Addr::unchecked("neutron_holder"),
                vec![coin(10000, NEUTRON_DENOM)],
            )
        })
        .unwrap();
    let block_info = BlockInfo {
        height: 12345,
        time: Timestamp::from_seconds(12345),
        chain_id: "testing".to_string(),
    };
    router.set_block(block_info);

    let merkle_airdrop_id = router.store_code(contract_merkle_airdrop());
    let credits_id = router.store_code(contract_credits());

    let credits_instantiate_msg = credits::msg::InstantiateMsg {
        dao_address: "neutron_holder".to_string(),
    };

    let credits_addr = router
        .instantiate_contract(
            credits_id,
            Addr::unchecked("neutron_holder".to_string()),
            &credits_instantiate_msg,
            &[],
            "Airdrop Test",
            None,
        )
        .unwrap();

    let _res = router.execute_contract(
        Addr::unchecked("neutron_holder".to_string()),
        credits_addr.clone(),
        &credits::msg::ExecuteMsg::UpdateConfig {
            config: credits::msg::UpdateConfigMsg {
                airdrop_address: Some("contract1".to_string()),
                lockdrop_address: Some("contract2".to_string()),
                when_withdrawable: Some(Default::default()),
            },
        },
        &[],
    );

    let merkle_airdrop_instantiate_msg = InstantiateMsg {
        credits_address: credits_addr.to_string(),
        reserve_address: "reserve0000".to_string(),
        merkle_root: test_data.root,
        airdrop_start: router.block_info().time.plus_seconds(5).seconds(),
        vesting_start: router.block_info().time.plus_seconds(10).seconds(),
        vesting_duration_seconds: 10,
        total_amount: Some(Uint128::new(10000)),
        hrp: None,
    };

    let merkle_airdrop_addr = router
        .instantiate_contract(
            merkle_airdrop_id,
            Addr::unchecked("owner0000".to_string()),
            &merkle_airdrop_instantiate_msg,
            &[],
            "Airdrop Test",
            None,
        )
        .unwrap();

    let _res = router.execute_contract(
        Addr::unchecked("neutron_holder".to_string()),
        credits_addr.clone(),
        &credits::msg::ExecuteMsg::UpdateConfig {
            config: credits::msg::UpdateConfigMsg {
                airdrop_address: Some("contract1".to_string()),
                lockdrop_address: Some("contract2".to_string()),
                when_withdrawable: Some(Default::default()),
            },
        },
        &[],
    );

    //mint cw20 tokens
    let mint_recipient = Addr::unchecked(merkle_airdrop_addr.to_string());
    let mint_amount = Uint128::new(10000);
    let credits_mint_msg = credits::msg::ExecuteMsg::Mint {};
    //execute mint
    router
        .execute_contract(
            Addr::unchecked("neutron_holder".to_string()),
            credits_addr.clone(),
            &credits_mint_msg,
            &[coin(mint_amount.u128(), NEUTRON_DENOM)],
        )
        .unwrap();

    //check airdrop contract balance
    let response: BalanceResponse = router
        .wrap()
        .query_wasm_smart(
            &credits_addr,
            &cw20_base::msg::QueryMsg::Balance {
                address: mint_recipient.to_string(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::new(10000), response.balance);

    let withdraw_msg = ExecuteMsg::WithdrawAll {};
    //unauthorized
    let err = router
        .execute_contract(
            Addr::unchecked("owner0000".to_string()),
            merkle_airdrop_addr.clone(),
            &withdraw_msg,
            &[],
        )
        .unwrap_err()
        .downcast::<ContractError>()
        .unwrap();
    assert_eq!(err, ContractError::Unauthorized {});
    //withdraw before expiration
    let err = router
        .execute_contract(
            Addr::unchecked("reserve0000".to_string()),
            merkle_airdrop_addr.clone(),
            &withdraw_msg,
            &[],
        )
        .unwrap_err()
        .downcast::<ContractError>()
        .unwrap();
    assert_eq!(
        err,
        ContractError::WithdrawAllUnavailable {
            available_at: router.block_info().time.plus_seconds(20).seconds()
        }
    );

    //update block height
    let block_info = BlockInfo {
        height: 12501,
        time: Timestamp::from_seconds(12501),
        chain_id: "testing".to_string(),
    };
    router.set_block(block_info);

    // withdraw after expiration
    let withdraw_all_msg = ExecuteMsg::WithdrawAll {};
    router
        .execute_contract(
            Addr::unchecked("reserve0000".to_string()),
            merkle_airdrop_addr.clone(),
            &withdraw_all_msg,
            &[],
        )
        .unwrap();

    //check airdrop contract cw20 balance
    let new_balance: BalanceResponse = router
        .wrap()
        .query_wasm_smart(
            &credits_addr,
            &cw20_base::msg::QueryMsg::Balance {
                address: mint_recipient.to_string(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::zero(), new_balance.balance);
    //check airdrop contract balance
    let recipient_balance = router
        .wrap()
        .query_balance(merkle_airdrop_addr.to_string(), NEUTRON_DENOM)
        .unwrap();
    assert_eq!(Uint128::new(0), recipient_balance.amount);
    //check reserve contract balance
    let recipient_balance = router
        .wrap()
        .query_balance("reserve0000", NEUTRON_DENOM)
        .unwrap();
    assert_eq!(Uint128::new(10000), recipient_balance.amount);
}

#[test]
fn starts() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let airdrop_start = env.block.time.plus_seconds(5_000).seconds();
    let vesting_start = env.block.time.plus_seconds(10_000).seconds();
    let vesting_duration_seconds = 20_000;

    let msg = InstantiateMsg {
        credits_address: "credits0000".to_string(),
        reserve_address: "reserve0000".to_string(),
        merkle_root: "5d4f48f147cb6cb742b376dce5626b2a036f69faec10cd73631c791780e150fc".to_string(),
        airdrop_start,
        vesting_start,
        vesting_duration_seconds,
        total_amount: None,
        hrp: None,
    };

    let info = mock_info("owner0000", &[]);
    let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    // can't claim, airdrop has not started yet
    let msg = ExecuteMsg::Claim {
        amount: Uint128::new(5),
        proof: vec![],
    };

    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(
        res,
        ContractError::NotBegun {
            start: airdrop_start
        }
    )
}
