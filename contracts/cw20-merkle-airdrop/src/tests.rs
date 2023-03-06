use crate::{
    contract::{execute, instantiate, query, NEUTRON_DENOM, VESTING_DURATION_SECONDS},
    error::ContractError,
    helpers::CosmosSignature,
    msg::{
        AccountMapResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, IsClaimedResponse,
        MerkleRootResponse, QueryMsg, SignatureInfo, TotalClaimedResponse,
    },
};
use cosmwasm_std::{
    attr, coin, from_binary, from_slice,
    testing::{mock_dependencies, mock_env, mock_info},
    to_binary, Addr, Attribute, Binary, BlockInfo, CosmosMsg, Empty, SubMsg, Timestamp, Uint128,
    WasmMsg,
};
use credits::msg::ExecuteMsg::AddVesting;
use cw20::{BalanceResponse, Cw20ExecuteMsg, MinterResponse};
use cw_multi_test::{App, BankKeeper, Contract, ContractWrapper, Executor};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn mock_app() -> App {
    App::default()
}

pub fn contract_merkle_airdrop() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(execute, instantiate, query))
}

pub fn contract_cw20() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    );
    Box::new(contract)
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let msg = InstantiateMsg {
        credits_address: Some(String::from("credits0000")),
        reserve_address: Some(String::from("reserve0000")),
        merkle_root: "634de21cde1044f41d90373733b0f0fb1c1c71f9652b905cdf159e73c4cf0d37".to_string(),
        expiration: env.block.time.plus_seconds(10_000),
        start: env.block.time.plus_seconds(5_000),
        total_amount: None,
        hrp: None,
    };

    let info = mock_info("owner0000", &[]);
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    // update owner
    let env = mock_env();
    let info = mock_info("owner0000", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        new_owner: Some("owner0001".to_string()),
        new_credits_address: None,
        new_reserve_address: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!("owner0001", config.owner);
    assert_eq!("credits0000", config.credits_address.unwrap());

    // Unauthorized err
    let env = mock_env();
    let info = mock_info("owner0000", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        new_owner: None,
        new_credits_address: None,
        new_reserve_address: None,
    };

    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::Unauthorized {});

    // update credits and reserve addresses
    let env = mock_env();
    let info = mock_info("owner0001", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        new_owner: Some("owner0001".to_string()),
        new_credits_address: Some("credits0001".to_string()),
        new_reserve_address: Some("reserve0001".to_string()),
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!("owner0001", config.owner);
    assert_eq!("credits0001", config.credits_address.unwrap());
    assert_eq!("reserve0001", config.reserve_address.unwrap());

    // update neutron denom
    let env = mock_env();
    let info = mock_info("owner0001", &[]);
    let msg = ExecuteMsg::UpdateConfig {
        new_owner: Some("owner0001".to_string()),
        new_credits_address: None,
        new_reserve_address: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // it worked, let's query the state
    let res = query(deps.as_ref(), env, QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!("owner0001", config.owner);
    assert_eq!("credits0001", config.credits_address.unwrap());
    assert_eq!("reserve0001", config.reserve_address.unwrap());
}

#[test]
fn proper_instantiation() {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let msg = InstantiateMsg {
        credits_address: Some("credits0000".to_string()),
        reserve_address: Some("reserve0000".to_string()),
        merkle_root: "634de21cde1044f41d90373733b0f0fb1c1c71f9652b905cdf159e73c4cf0d37".to_string(),
        expiration: env.block.time.plus_seconds(10_000),
        start: env.block.time.plus_seconds(5_000),
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

    let res = query(deps.as_ref(), env, QueryMsg::MerkleRoot {}).unwrap();
    let merkle_root: MerkleRootResponse = from_binary(&res).unwrap();
    assert_eq!(
        "634de21cde1044f41d90373733b0f0fb1c1c71f9652b905cdf159e73c4cf0d37".to_string(),
        merkle_root.merkle_root
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
fn cant_claim_without_credits_address() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();

    let msg = InstantiateMsg {
        credits_address: None,
        reserve_address: Some("reserve0000".to_string()),
        merkle_root: test_data.root,
        expiration: env.block.time.plus_seconds(10_000),
        start: env.block.time.minus_seconds(10_000),
        total_amount: None,
        hrp: None,
    };

    let info = mock_info("owner0000", &[]);
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::Claim {
        amount: test_data.amount,
        proof: test_data.proofs,
        sig_info: None,
    };

    let env = mock_env();
    let info = mock_info(test_data.account.as_str(), &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::CreditsAddress {});
}

#[test]
fn claim() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();

    let msg = InstantiateMsg {
        credits_address: Some("credits0000".to_string()),
        reserve_address: Some("reserve0000".to_string()),
        merkle_root: test_data.root,
        expiration: env.block.time.plus_seconds(10_000),
        start: env.block.time.minus_seconds(10_000),
        total_amount: None,
        hrp: None,
    };

    let info = mock_info("owner0000", &[]);
    let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

    let msg = ExecuteMsg::Claim {
        amount: test_data.amount,
        proof: test_data.proofs,
        sig_info: None,
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
                start_time: env.block.time.plus_seconds(10_000).seconds(),
                duration: VESTING_DURATION_SECONDS,
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
    let test_data: MultipleData = from_slice(TEST_DATA_1_MULTI).unwrap();

    let msg = InstantiateMsg {
        credits_address: Some("credits0000".to_string()),
        reserve_address: Some("reserve0000".to_string()),
        merkle_root: test_data.root,
        expiration: env.block.time.plus_seconds(10_000),
        start: env.block.time.minus_seconds(10_000),
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
            sig_info: None,
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
                    start_time: env.block.time.plus_seconds(10_000).seconds(),
                    duration: VESTING_DURATION_SECONDS,
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
    let info = mock_info("owner0000", &[]);
    let expiration = env.block.time.plus_seconds(100);
    let start = env.block.time.plus_seconds(50);

    let msg = InstantiateMsg {
        credits_address: Some("credits0000".to_string()),
        reserve_address: Some("reserve0000".to_string()),
        merkle_root: "5d4f48f147cb6cb742b376dce5626b2a036f69faec10cd73631c791780e150fc".to_string(),
        expiration,
        start,
        total_amount: None,
        hrp: None,
    };
    let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    // make expired
    env.block.time = env.block.time.plus_seconds(200);

    // can't claim expired
    let msg = ExecuteMsg::Claim {
        amount: Uint128::new(5),
        proof: vec![],
        sig_info: None,
    };

    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::Expired { expiration })
}

#[test]
fn cant_withdraw_all_without_reserve_address() {
    let mut deps = mock_dependencies();
    let mut env = mock_env();
    let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();

    let msg = InstantiateMsg {
        credits_address: Some("credits0000".to_string()),
        reserve_address: None,
        merkle_root: test_data.root,
        expiration: env.block.time.plus_seconds(100),
        start: env.block.time.plus_seconds(50),
        total_amount: Some(Uint128::new(10000)),
        hrp: None,
    };

    let info = mock_info("owner0000", &[]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    // makes withdraw_all available
    env.block.time = env.block.time.plus_seconds(VESTING_DURATION_SECONDS + 101);

    let info = mock_info("owner0000", &[]);
    let res = execute(deps.as_mut(), env, info, ExecuteMsg::WithdrawAll {}).unwrap_err();
    assert_eq!(res, ContractError::ReserveAddress {});
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
    let cw20_id = router.store_code(contract_cw20());

    let cw20_instantiate_msg = cw20_base::msg::InstantiateMsg {
        name: "Airdrop Token".parse().unwrap(),
        symbol: "ADT".parse().unwrap(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: "minter0000".to_string(),
            cap: None,
        }),
        marketing: None,
    };
    let cw20_addr = router
        .instantiate_contract(
            cw20_id,
            Addr::unchecked("minter0000".to_string()),
            &cw20_instantiate_msg,
            &[],
            "Airdrop Test",
            None,
        )
        .unwrap();

    let merkle_airdrop_instantiate_msg = InstantiateMsg {
        credits_address: Some(cw20_addr.to_string()),
        reserve_address: Some("reserve0000".to_string()),
        merkle_root: test_data.root,
        expiration: router.block_info().time.plus_seconds(10),
        start: router.block_info().time.plus_seconds(5),
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

    //mint cw20 tokens
    let mint_recipient = Addr::unchecked(merkle_airdrop_addr.to_string());
    let mint_amount = Uint128::new(10000);
    let cw20_mint_msg = cw20_base::msg::ExecuteMsg::Mint {
        recipient: mint_recipient.to_string(),
        amount: mint_amount,
    };
    //execute mint
    router
        .execute_contract(
            Addr::unchecked("minter0000".to_string()),
            cw20_addr.clone(),
            &cw20_mint_msg,
            &[],
        )
        .unwrap();

    //check airdrop contract balance
    let response: BalanceResponse = router
        .wrap()
        .query_wasm_smart(
            &cw20_addr,
            &cw20_base::msg::QueryMsg::Balance {
                address: mint_recipient.to_string(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::new(10000), response.balance);
    //withdraw before expiration
    let withdraw_msg = ExecuteMsg::WithdrawAll {};
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
    assert_eq!(
        err,
        ContractError::WithdrawAllUnavailable {
            available_at: router
                .block_info()
                .time
                .plus_seconds(10)
                .plus_seconds(VESTING_DURATION_SECONDS)
        }
    );

    //update block height
    let block_info = BlockInfo {
        height: 12501,
        time: Timestamp::from_seconds(12501).plus_seconds(VESTING_DURATION_SECONDS),
        chain_id: "testing".to_string(),
    };
    router.set_block(block_info);

    // We expect credits contract to send 10000 untrn to merkle airdrop contract
    // during processing of this message, so we mimic this behaviour manually
    router
        .send_tokens(
            Addr::unchecked("neutron_holder"),
            merkle_airdrop_addr.clone(),
            &[coin(10000, NEUTRON_DENOM)],
        )
        .unwrap();

    // withdraw after expiration
    let partial_withdraw_msg = ExecuteMsg::WithdrawAll {};
    let res = router
        .execute_contract(
            Addr::unchecked("owner0000".to_string()),
            merkle_airdrop_addr.clone(),
            &partial_withdraw_msg,
            &[],
        )
        .unwrap();

    assert_eq!(
        res.events[1].attributes,
        vec![
            Attribute {
                key: "_contract_addr".to_string(),
                value: "contract1".to_string()
            },
            Attribute {
                key: "action".to_string(),
                value: "withdraw_all".to_string()
            },
            Attribute {
                key: "address".to_string(),
                value: "owner0000".to_string()
            },
            Attribute {
                key: "amount".to_string(),
                value: "10000".to_string()
            },
            Attribute {
                key: "recipient".to_string(),
                value: "reserve0000".to_string()
            }
        ]
    );
    //check airdrop contract cw20 balance
    let new_balance: BalanceResponse = router
        .wrap()
        .query_wasm_smart(
            &cw20_addr,
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
    let start = env.block.time.plus_seconds(5_000);

    let msg = InstantiateMsg {
        credits_address: Some("credits0000".to_string()),
        reserve_address: Some("reserve0000".to_string()),
        merkle_root: "5d4f48f147cb6cb742b376dce5626b2a036f69faec10cd73631c791780e150fc".to_string(),
        expiration: env.block.time.plus_seconds(10_000),
        start,
        total_amount: None,
        hrp: None,
    };

    let info = mock_info("owner0000", &[]);
    let _res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    // can't claim, airdrop has not started yet
    let msg = ExecuteMsg::Claim {
        amount: Uint128::new(5),
        proof: vec![],
        sig_info: None,
    };

    let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(res, ContractError::NotBegun { start })
}

mod external_sig {
    use super::*;
    use crate::msg::SignatureInfo;

    const TEST_DATA_EXTERNAL_SIG: &[u8] =
        include_bytes!("../testdata/airdrop_external_sig_test_data.json");

    #[test]
    fn test_cosmos_sig_verify() {
        let deps = mock_dependencies();
        let signature_raw = Binary::from_base64("eyJwdWJfa2V5IjoiQWhOZ2UxV01aVXl1ODZ5VGx5ZWpEdVVxUFZTdURONUJhQzArdkw4b3RkSnYiLCJzaWduYXR1cmUiOiJQY1FPczhXSDVPMndXL3Z3ZzZBTElqaW9VNGorMUZYNTZKU1R1MzdIb2lGbThJck5aem5HaGlIRFV1R1VTUmlhVnZRZ2s4Q0tURmNyeVpuYjZLNVhyQT09In0=");

        let sig = SignatureInfo {
            claim_msg: Binary::from_base64("eyJhY2NvdW50X251bWJlciI6IjExMjM2IiwiY2hhaW5faWQiOiJwaXNjby0xIiwiZmVlIjp7ImFtb3VudCI6W3siYW1vdW50IjoiMTU4MTIiLCJkZW5vbSI6InVsdW5hIn1dLCJnYXMiOiIxMDU0MDcifSwibWVtbyI6Imp1bm8xMHMydXU5MjY0ZWhscWw1ZnB5cmg5dW5kbmw1bmxhdzYzdGQwaGgiLCJtc2dzIjpbeyJ0eXBlIjoiY29zbW9zLXNkay9Nc2dTZW5kIiwidmFsdWUiOnsiYW1vdW50IjpbeyJhbW91bnQiOiIxIiwiZGVub20iOiJ1bHVuYSJ9XSwiZnJvbV9hZGRyZXNzIjoidGVycmExZmV6NTlzdjh1cjk3MzRmZnJwdndwY2phZHg3bjB4Nno2eHdwN3oiLCJ0b19hZGRyZXNzIjoidGVycmExZmV6NTlzdjh1cjk3MzRmZnJwdndwY2phZHg3bjB4Nno2eHdwN3oifX1dLCJzZXF1ZW5jZSI6IjAifQ==").unwrap(),
            signature: signature_raw.unwrap(),
        };
        let cosmos_signature: CosmosSignature = from_binary(&sig.signature).unwrap();
        let res = cosmos_signature
            .verify(deps.as_ref(), &sig.claim_msg)
            .unwrap();
        assert!(res);
    }

    #[test]
    fn test_derive_addr_from_pubkey() {
        let test_data: Encoded = from_slice(TEST_DATA_EXTERNAL_SIG).unwrap();
        let cosmos_signature: CosmosSignature =
            from_binary(&test_data.signed_msg.unwrap().signature).unwrap();
        let derived_addr = cosmos_signature
            .derive_addr_from_pubkey(&test_data.hrp.unwrap())
            .unwrap();
        assert_eq!(test_data.account, derived_addr);
    }

    #[test]
    fn claim_with_external_sigs() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let test_data: Encoded = from_slice(TEST_DATA_EXTERNAL_SIG).unwrap();
        let claim_addr = test_data
            .signed_msg
            .clone()
            .unwrap()
            .extract_addr()
            .unwrap();

        let msg = InstantiateMsg {
            credits_address: Some("credits0000".to_string()),
            reserve_address: Some("reserve0000".to_string()),
            merkle_root: test_data.root,
            expiration: env.block.time.plus_seconds(10_000),
            start: env.block.time.minus_seconds(10_000),
            total_amount: None,
            hrp: Some(test_data.hrp.unwrap()),
        };

        let info = mock_info("owner0000", &[]);
        let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

        // cant claim without sig, info.sender is not present in the root
        let msg = ExecuteMsg::Claim {
            amount: test_data.amount,
            proof: test_data.proofs.clone(),
            sig_info: None,
        };

        let env = mock_env();
        let info = mock_info(claim_addr.as_str(), &[]);
        let res = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(res, ContractError::VerificationFailed {});

        // can claim with sig
        let msg = ExecuteMsg::Claim {
            amount: test_data.amount,
            proof: test_data.proofs,
            sig_info: test_data.signed_msg,
        };

        let env = mock_env();
        let info = mock_info(claim_addr.as_str(), &[]);
        let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone()).unwrap();
        let expected = vec![
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "credits0000".to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: claim_addr.to_string(),
                    amount: test_data.amount,
                })
                .unwrap(),
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "credits0000".to_string(),
                msg: to_binary(&AddVesting {
                    address: claim_addr.clone(),
                    amount: test_data.amount,
                    start_time: env.block.time.plus_seconds(10_000).seconds(),
                    duration: VESTING_DURATION_SECONDS,
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
                attr("address", claim_addr.clone()),
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
                        address: test_data.account.clone(),
                    },
                )
                .unwrap()
            )
            .unwrap()
            .is_claimed
        );

        // check error on double claim
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();
        assert_eq!(res, ContractError::Claimed {});

        // query map

        let map = from_binary::<AccountMapResponse>(
            &query(
                deps.as_ref(),
                env,
                QueryMsg::AccountMap {
                    external_address: test_data.account.clone(),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(map.external_address, test_data.account);
        assert_eq!(map.host_address, claim_addr);
    }

    #[test]
    fn claim_paused_airdrop() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let test_data: Encoded = from_slice(TEST_DATA_1).unwrap();

        let msg = InstantiateMsg {
            credits_address: Some("credits0000".to_string()),
            reserve_address: Some("reserve0000".to_string()),
            merkle_root: test_data.root,
            expiration: env.block.time.plus_seconds(10_000),
            start: env.block.time.minus_seconds(10_000),
            total_amount: None,
            hrp: None,
        };

        let info = mock_info("owner0000", &[]);
        let _res = instantiate(deps.as_mut(), env, info, msg).unwrap();

        let pause_msg = ExecuteMsg::Pause {};
        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let result = execute(deps.as_mut(), env, info, pause_msg).unwrap();

        assert_eq!(
            result.attributes,
            vec![attr("action", "pause"), attr("paused", "true"),]
        );

        let msg = ExecuteMsg::Claim {
            amount: test_data.amount,
            proof: test_data.proofs.clone(),
            sig_info: None,
        };

        let env = mock_env();
        let info = mock_info(test_data.account.as_str(), &[]);
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap_err();

        assert_eq!(res, ContractError::Paused {});

        let resume_msg = ExecuteMsg::Resume {
            new_expiration: Some(env.block.time.plus_seconds(5_000)),
        };
        let env = mock_env();
        let info = mock_info("owner0000", &[]);
        let result = execute(deps.as_mut(), env, info, resume_msg).unwrap();

        assert_eq!(
            result.attributes,
            vec![attr("action", "resume"), attr("paused", "false"),]
        );
        let msg = ExecuteMsg::Claim {
            amount: test_data.amount,
            proof: test_data.proofs.clone(),
            sig_info: None,
        };
        let env = mock_env();
        let info = mock_info(test_data.account.as_str(), &[]);
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        let expected = vec![
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "credits0000".to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: test_data.account.to_string(),
                    amount: test_data.amount,
                })
                .unwrap(),
                funds: vec![],
            })),
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "credits0000".to_string(),
                msg: to_binary(&AddVesting {
                    address: test_data.account.clone(),
                    amount: test_data.amount,
                    start_time: env.block.time.plus_seconds(5_000).seconds(),
                    duration: VESTING_DURATION_SECONDS,
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
    }

    #[test]
    fn withdraw_all_paused_airdrop() {
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
        let cw20_id = router.store_code(contract_cw20());

        let cw20_instantiate_msg = cw20_base::msg::InstantiateMsg {
            name: "Airdrop Token".parse().unwrap(),
            symbol: "ADT".parse().unwrap(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: "minter0000".to_string(),
                cap: None,
            }),
            marketing: None,
        };
        let cw20_addr = router
            .instantiate_contract(
                cw20_id,
                Addr::unchecked("minter0000".to_string()),
                &cw20_instantiate_msg,
                &[],
                "Airdrop Test",
                None,
            )
            .unwrap();

        let merkle_airdrop_instantiate_msg = InstantiateMsg {
            credits_address: Some(cw20_addr.to_string()),
            reserve_address: Some("reserve0000".to_string()),
            merkle_root: test_data.root,
            expiration: router.block_info().time.plus_seconds(10_000),
            start: router.block_info().time.minus_seconds(10_000),
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

        //mint cw20 tokens
        let mint_recipient = Addr::unchecked(merkle_airdrop_addr.to_string());
        let mint_amount = Uint128::new(10000);
        let cw20_mint_msg = cw20_base::msg::ExecuteMsg::Mint {
            recipient: mint_recipient.to_string(),
            amount: mint_amount,
        };
        //execute mint
        router
            .execute_contract(
                Addr::unchecked("minter0000".to_string()),
                cw20_addr.clone(),
                &cw20_mint_msg,
                &[],
            )
            .unwrap();

        //check airdrop contract balance
        let response: BalanceResponse = router
            .wrap()
            .query_wasm_smart(
                &cw20_addr,
                &cw20_base::msg::QueryMsg::Balance {
                    address: mint_recipient.to_string(),
                },
            )
            .unwrap();
        assert_eq!(Uint128::new(10000), response.balance);

        // Can't withdraw before pause
        let msg = ExecuteMsg::WithdrawAll {};
        router
            .execute_contract(
                Addr::unchecked("owner0000"),
                merkle_airdrop_addr.clone(),
                &msg,
                &[],
            )
            .unwrap_err()
            .downcast::<ContractError>()
            .unwrap();

        let pause_msg = ExecuteMsg::Pause {};
        let result = router
            .execute_contract(
                Addr::unchecked("owner0000"),
                merkle_airdrop_addr.clone(),
                &pause_msg,
                &[],
            )
            .unwrap()
            .events
            .into_iter()
            .find(|event| event.ty == "wasm")
            .unwrap()
            .attributes
            .into_iter()
            .filter(|attribute| ["action", "paused"].contains(&attribute.key.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            result,
            vec![attr("action", "pause"), attr("paused", "true")]
        );

        // We expect credits contract to send 10000 neutrons to merkle airdrop contract
        // during processing of this message, so we mimic this behaviour manually
        router
            .send_tokens(
                Addr::unchecked("neutron_holder"),
                merkle_airdrop_addr.clone(),
                &[coin(10000, NEUTRON_DENOM)],
            )
            .unwrap();

        //Withdraw when paused
        let msg = ExecuteMsg::WithdrawAll {};
        let res = router
            .execute_contract(
                Addr::unchecked("owner0000"),
                merkle_airdrop_addr.clone(),
                &msg,
                &[],
            )
            .unwrap();

        assert_eq!(
            res.events[1].attributes,
            vec![
                Attribute {
                    key: "_contract_addr".to_string(),
                    value: "contract1".to_string()
                },
                Attribute {
                    key: "action".to_string(),
                    value: "withdraw_all".to_string()
                },
                Attribute {
                    key: "address".to_string(),
                    value: "owner0000".to_string()
                },
                Attribute {
                    key: "amount".to_string(),
                    value: "10000".to_string()
                },
                Attribute {
                    key: "recipient".to_string(),
                    value: "reserve0000".to_string()
                }
            ]
        );
        //check airdrop contract cw20 balance
        let new_balance: BalanceResponse = router
            .wrap()
            .query_wasm_smart(
                &cw20_addr,
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
}
