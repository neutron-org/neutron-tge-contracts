use crate::msg::QueryMsg::{UnclaimedAmountAtHeight, UnclaimedTotalAmountAtHeight};
use astroport::asset::{native_asset_info, token_asset_info};
use astroport::querier::query_balance;
use astroport::vesting::{QueryMsg, VestingAccountResponse};
use astroport::{
    token::InstantiateMsg as TokenInstantiateMsg,
    vesting::{
        Cw20HookMsg, ExecuteMsg, InstantiateMsg, VestingAccount, VestingSchedule,
        VestingSchedulePoint,
    },
};
use cosmwasm_std::{coin, coins, to_binary, Addr, StdResult, Timestamp, Uint128};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use cw_multi_test::{App, ContractWrapper, Executor};
use cw_utils::PaymentError;
use vesting_base::error::ContractError;
use vesting_base::state::Config;

const OWNER1: &str = "owner1";
const TOKEN_MANAGER: &str = "token_manager";
const USER1: &str = "user1";
const USER2: &str = "user2";
const TOKEN_INITIAL_AMOUNT: u128 = 1_000_000_000_000_000;
const VESTING_TOKEN: &str = "vesting_token";
const BLOCK_TIME: u64 = 5;

#[test]
fn claim() {
    let user1 = Addr::unchecked(USER1);
    let owner = Addr::unchecked(OWNER1);

    let mut app = mock_app(&owner);

    let token_code_id = store_token_code(&mut app);

    let cw20_token_instance =
        instantiate_token(&mut app, token_code_id, "NTRN", Some(1_000_000_000_000_000));

    let vesting_instance = instantiate_vesting(&mut app, &cw20_token_instance);

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(101).seconds(),
                            amount: Uint128::new(200),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(110).seconds(),
                            amount: Uint128::new(100),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(200).seconds(),
                            amount: Uint128::new(100),
                        }),
                    },
                ],
            }],
        })
        .unwrap(),
        amount: Uint128::from(300u128),
    };

    let res = app
        .execute_contract(owner.clone(), cw20_token_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(res.root_cause().to_string(), "Vesting schedule amount error. The total amount should be equal to the CW20 receive amount.");

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(101).seconds(),
                            amount: Uint128::new(100),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(110).seconds(),
                            amount: Uint128::new(100),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: Timestamp::from_seconds(100).seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: Timestamp::from_seconds(200).seconds(),
                            amount: Uint128::new(100),
                        }),
                    },
                ],
            }],
        })
        .unwrap(),
        amount: Uint128::from(300u128),
    };

    app.execute_contract(owner.clone(), cw20_token_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let user1_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(user1_vesting_amount.clone(), Uint128::new(300u128));

    // Check owner balance
    check_token_balance(
        &mut app,
        &cw20_token_instance,
        &owner,
        TOKEN_INITIAL_AMOUNT - 300u128,
    );

    // Check vesting balance
    check_token_balance(&mut app, &cw20_token_instance, &vesting_instance, 300u128);

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    let _res = app
        .execute_contract(user1.clone(), vesting_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::VestingAccount {
        address: user1.to_string(),
    };

    let vesting_res: VestingAccountResponse = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(vesting_res.info.released_amount, Uint128::from(300u128));

    // Check vesting balance
    check_token_balance(&mut app, &cw20_token_instance, &vesting_instance, 0u128);

    // Check user balance
    check_token_balance(&mut app, &cw20_token_instance, &user1, 300u128);

    // Owner balance mustn't change after claim
    check_token_balance(
        &mut app,
        &cw20_token_instance,
        &owner.clone(),
        TOKEN_INITIAL_AMOUNT - 300u128,
    );

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    // Check user balance after claim
    let user1_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    assert_eq!(user1_vesting_amount.clone(), Uint128::new(0u128));
}

#[test]
fn claim_native() {
    let user1 = Addr::unchecked(USER1);
    let owner = Addr::unchecked(OWNER1);

    let mut app = mock_app(&owner);

    let token_code_id = store_token_code(&mut app);

    let random_token_instance =
        instantiate_token(&mut app, token_code_id, "RND", Some(1_000_000_000));

    mint_tokens(&mut app, &random_token_instance, &owner, 1_000_000_000);

    let vesting_instance = instantiate_vesting_remote_chain(&mut app);

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(101).seconds(),
                        amount: Uint128::new(200),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(300u128),
    };

    let err = app
        .execute_contract(owner.clone(), random_token_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user1.to_string(),
            schedules: vec![
                VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(101).seconds(),
                        amount: Uint128::new(100),
                    }),
                },
                VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(110).seconds(),
                        amount: Uint128::new(100),
                    }),
                },
                VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(200).seconds(),
                        amount: Uint128::new(100),
                    }),
                },
            ],
        }],
    };

    app.execute_contract(
        owner.clone(),
        vesting_instance.clone(),
        &msg,
        &coins(300, VESTING_TOKEN),
    )
    .unwrap();

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let user1_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(user1_vesting_amount.clone(), Uint128::new(300u128));

    // Check owner balance
    let bal = query_balance(&app.wrap(), &owner, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 300u128);

    // Check vesting balance
    let bal = query_balance(&app.wrap(), &vesting_instance, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, 300u128);

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    app.execute_contract(user1.clone(), vesting_instance.clone(), &msg, &[])
        .unwrap();

    let vesting_res: VestingAccountResponse = app
        .wrap()
        .query_wasm_smart(
            vesting_instance.clone(),
            &QueryMsg::VestingAccount {
                address: user1.to_string(),
            },
        )
        .unwrap();
    assert_eq!(vesting_res.info.released_amount, Uint128::from(300u128));

    // Check vesting balance
    let bal = query_balance(&app.wrap(), &vesting_instance, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, 0);

    // Check user balance
    let bal = query_balance(&app.wrap(), &user1, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, 300);

    // Owner balance mustn't change after claim
    let bal = query_balance(&app.wrap(), &owner, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 300u128);

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    // Check user balance after claim
    let user1_vesting_amount: Uint128 =
        app.wrap().query_wasm_smart(vesting_instance, &msg).unwrap();

    assert_eq!(user1_vesting_amount.clone(), Uint128::new(0u128));
}

#[test]
fn register_vesting_accounts() {
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);
    let owner = Addr::unchecked(OWNER1);

    let mut app = mock_app(&owner);

    let token_code_id = store_token_code(&mut app);

    let cw20_token_instance =
        instantiate_token(&mut app, token_code_id, "NTRN", Some(1_000_000_000_000_000));

    let noname_token_instance = instantiate_token(
        &mut app,
        token_code_id,
        "NONAME",
        Some(1_000_000_000_000_000),
    );

    mint_tokens(
        &mut app,
        &noname_token_instance,
        &owner,
        TOKEN_INITIAL_AMOUNT,
    );

    let vesting_instance = instantiate_vesting(&mut app, &cw20_token_instance);

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(150).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::new(100),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(100u128),
    };

    let res = app
        .execute_contract(owner.clone(), cw20_token_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(res.root_cause().to_string(), "Vesting schedule error on addr: user1. Should satisfy: (start < end and at_start < total) or (start = end and at_start = total)");

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(150).seconds(),
                        amount: Uint128::new(100),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(100u128),
    };

    let res = app
        .execute_contract(
            user1.clone(),
            cw20_token_instance.clone(),
            &msg.clone(),
            &[],
        )
        .unwrap_err();
    assert_eq!(res.root_cause().to_string(), "Cannot Sub with 0 and 100");

    let res = app
        .execute_contract(owner.clone(), noname_token_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(res.root_cause().to_string(), "Unauthorized");

    // Checking that execute endpoint with native coin is unreachable if the asset is a cw20 token
    let native_msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user1.to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: Timestamp::from_seconds(100).seconds(),
                    amount: Uint128::zero(),
                },
                end_point: Some(VestingSchedulePoint {
                    time: Timestamp::from_seconds(150).seconds(),
                    amount: Uint128::new(100),
                }),
            }],
        }],
    };

    let err = app
        .execute_contract(
            owner.clone(),
            vesting_instance.clone(),
            &native_msg,
            &coins(100u128, "random_coin"),
        )
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let _res = app
        .execute_contract(owner.clone(), cw20_token_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let user1_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    assert_eq!(user1_vesting_amount.clone(), Uint128::new(100u128));
    check_token_balance(
        &mut app,
        &cw20_token_instance,
        &owner.clone(),
        TOKEN_INITIAL_AMOUNT - 100u128,
    );
    check_token_balance(&mut app, &cw20_token_instance, &vesting_instance, 100u128);

    // Let's check user1's final vesting amount after add schedule for a new one
    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user2.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(150).seconds(),
                        amount: Uint128::new(200),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(200u128),
    };

    let _res = app
        .execute_contract(owner.clone(), cw20_token_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::AvailableAmount {
        address: user2.to_string(),
    };

    let user2_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    check_token_balance(
        &mut app,
        &cw20_token_instance,
        &owner.clone(),
        TOKEN_INITIAL_AMOUNT - 300u128,
    );
    check_token_balance(&mut app, &cw20_token_instance, &vesting_instance, 300u128);
    // A new schedule has been added successfully and an old one hasn't changed.
    // The new schedule doesn't have the same value as the old one.
    assert_eq!(user2_vesting_amount, Uint128::new(200u128));
    assert_eq!(user1_vesting_amount, Uint128::from(100u128));

    // Add one more vesting schedule; final amount to vest must increase
    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(200).seconds(),
                        amount: Uint128::new(10),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(10u128),
    };

    let _res = app
        .execute_contract(owner.clone(), cw20_token_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let vesting_res: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    assert_eq!(vesting_res, Uint128::new(110u128));
    check_token_balance(
        &mut app,
        &cw20_token_instance,
        &owner.clone(),
        TOKEN_INITIAL_AMOUNT - 310u128,
    );
    check_token_balance(&mut app, &cw20_token_instance, &vesting_instance, 310u128);

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    let _res = app
        .execute_contract(user1.clone(), vesting_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::VestingAccount {
        address: user1.to_string(),
    };

    let vesting_res: VestingAccountResponse = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(vesting_res.info.released_amount, Uint128::from(110u128));
    check_token_balance(&mut app, &cw20_token_instance, &vesting_instance, 200u128);
    check_token_balance(&mut app, &cw20_token_instance, &user1, 110u128);

    // Owner balance mustn't change after claim
    check_token_balance(
        &mut app,
        &cw20_token_instance,
        &owner.clone(),
        TOKEN_INITIAL_AMOUNT - 310u128,
    );
}

#[test]
fn register_vesting_accounts_native() {
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);
    let owner = Addr::unchecked(OWNER1);

    let mut app = mock_app(&owner);

    let token_code_id = store_token_code(&mut app);

    let random_token_instance =
        instantiate_token(&mut app, token_code_id, "RND", Some(1_000_000_000_000_000));

    mint_tokens(
        &mut app,
        &random_token_instance,
        &owner,
        TOKEN_INITIAL_AMOUNT,
    );

    let vesting_instance = instantiate_vesting_remote_chain(&mut app);

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![VestingAccount {
                address: user1.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: Timestamp::from_seconds(100).seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: Timestamp::from_seconds(150).seconds(),
                        amount: Uint128::new(100),
                    }),
                }],
            }],
        })
        .unwrap(),
        amount: Uint128::from(100u128),
    };

    let err = app
        .execute_contract(owner.clone(), random_token_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    // Checking that execute endpoint with random native coin is unreachable
    let native_msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user1.to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: Timestamp::from_seconds(100).seconds(),
                    amount: Uint128::zero(),
                },
                end_point: Some(VestingSchedulePoint {
                    time: Timestamp::from_seconds(150).seconds(),
                    amount: Uint128::new(100),
                }),
            }],
        }],
    };

    let err = app
        .execute_contract(
            owner.clone(),
            vesting_instance.clone(),
            &native_msg,
            &coins(100u128, "random_coin"),
        )
        .unwrap_err();
    assert_eq!(
        ContractError::PaymentError(PaymentError::MissingDenom("vesting_token".to_string())),
        err.downcast().unwrap()
    );

    app.execute_contract(
        owner.clone(),
        vesting_instance.clone(),
        &native_msg,
        &coins(100u128, VESTING_TOKEN),
    )
    .unwrap();

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let user1_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(&vesting_instance, &msg)
        .unwrap();
    assert_eq!(user1_vesting_amount.u128(), 100u128);

    let bal = query_balance(&app.wrap(), &owner, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 100u128);

    let bal = query_balance(&app.wrap(), &vesting_instance, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, 100);

    // Let's check user1's final vesting amount after add schedule for a new one
    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user2.to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: Timestamp::from_seconds(100).seconds(),
                    amount: Uint128::zero(),
                },
                end_point: Some(VestingSchedulePoint {
                    time: Timestamp::from_seconds(150).seconds(),
                    amount: Uint128::new(200),
                }),
            }],
        }],
    };

    app.execute_contract(
        owner.clone(),
        vesting_instance.clone(),
        &msg,
        &coins(200, VESTING_TOKEN),
    )
    .unwrap();

    let msg = QueryMsg::AvailableAmount {
        address: user2.to_string(),
    };

    let user2_vesting_amount: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    let bal = query_balance(&app.wrap(), &owner, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 300u128);
    let bal = query_balance(&app.wrap(), &vesting_instance, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, 300u128);

    // A new schedule has been added successfully and an old one hasn't changed.
    // The new schedule doesn't have the same value as the old one.
    assert_eq!(user2_vesting_amount, Uint128::new(200u128));
    assert_eq!(user1_vesting_amount, Uint128::from(100u128));

    // Add one more vesting schedule; final amount to vest must increase
    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user1.to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: Timestamp::from_seconds(100).seconds(),
                    amount: Uint128::zero(),
                },
                end_point: Some(VestingSchedulePoint {
                    time: Timestamp::from_seconds(200).seconds(),
                    amount: Uint128::new(10),
                }),
            }],
        }],
    };

    app.execute_contract(
        owner.clone(),
        vesting_instance.clone(),
        &msg,
        &coins(10, VESTING_TOKEN),
    )
    .unwrap();

    let msg = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    let vesting_res: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(vesting_res, Uint128::new(110u128));

    let bal = query_balance(&app.wrap(), &owner, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 310u128);
    let bal = query_balance(&app.wrap(), &vesting_instance, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, 310u128);

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    let _res = app
        .execute_contract(user1.clone(), vesting_instance.clone(), &msg, &[])
        .unwrap();

    let msg = QueryMsg::VestingAccount {
        address: user1.to_string(),
    };

    let vesting_res: VestingAccountResponse = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(vesting_res.info.released_amount, Uint128::from(110u128));

    let bal = query_balance(&app.wrap(), &vesting_instance, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, 200);
    let bal = query_balance(&app.wrap(), &user1, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, 110u128);

    let bal = query_balance(&app.wrap(), &owner, VESTING_TOKEN)
        .unwrap()
        .u128();
    assert_eq!(bal, TOKEN_INITIAL_AMOUNT - 310u128);
}

#[test]
fn query_at_height() {
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);
    let owner = Addr::unchecked(OWNER1);

    let mut app = mock_app(&owner);
    let start_block_height = app.block_info().height;

    let vesting_instance = instantiate_vesting_remote_chain(&mut app);

    let native_msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![
            VestingAccount {
                address: user1.to_string(),
                schedules: vec![
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: app.block_info().time.seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: app
                                .block_info()
                                .time
                                .plus_seconds(100 * BLOCK_TIME)
                                .seconds(),
                            amount: Uint128::new(50),
                        }),
                    },
                    VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: app.block_info().time.seconds(),
                            amount: Uint128::zero(),
                        },
                        end_point: Some(VestingSchedulePoint {
                            time: app
                                .block_info()
                                .time
                                .plus_seconds(100 * BLOCK_TIME)
                                .seconds(),
                            amount: Uint128::new(150),
                        }),
                    },
                ],
            },
            VestingAccount {
                address: user2.to_string(),
                schedules: vec![VestingSchedule {
                    start_point: VestingSchedulePoint {
                        time: app.block_info().time.seconds(),
                        amount: Uint128::zero(),
                    },
                    end_point: Some(VestingSchedulePoint {
                        time: app
                            .block_info()
                            .time
                            .plus_seconds(100 * BLOCK_TIME)
                            .seconds(),
                        amount: Uint128::new(1000),
                    }),
                }],
            },
        ],
    };

    app.execute_contract(
        owner,
        vesting_instance.clone(),
        &native_msg,
        &coins(1200, VESTING_TOKEN),
    )
    .unwrap();

    let query = QueryMsg::AvailableAmount {
        address: user1.to_string(),
    };

    for _ in 1..=10 {
        let vesting_res: Uint128 = app
            .wrap()
            .query_wasm_smart(vesting_instance.clone(), &query)
            .unwrap();
        assert_eq!(vesting_res, Uint128::new(0u128));

        app.update_block(|b| {
            b.height += 10;
            b.time = b.time.plus_seconds(10 * BLOCK_TIME)
        });

        let vesting_res: Uint128 = app
            .wrap()
            .query_wasm_smart(vesting_instance.clone(), &query)
            .unwrap();
        assert_eq!(vesting_res, Uint128::new(20u128));

        let msg = ExecuteMsg::Claim {
            recipient: None,
            amount: None,
        };
        let _res = app
            .execute_contract(user1.clone(), vesting_instance.clone(), &msg, &[])
            .unwrap();

        let vesting_res: Uint128 = app
            .wrap()
            .query_wasm_smart(vesting_instance.clone(), &query)
            .unwrap();
        assert_eq!(vesting_res, Uint128::new(0u128));
    }
    app.update_block(|b| {
        b.height += 100;
        b.time = b.time.plus_seconds(100 * BLOCK_TIME)
    });
    let vesting_res: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &query)
        .unwrap();
    assert_eq!(vesting_res, Uint128::new(0u128));

    let query_user_unclamed = UnclaimedAmountAtHeight {
        address: user1.to_string(),
        height: start_block_height - 1,
    };
    let vesting_res: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &query_user_unclamed)
        .unwrap();
    assert_eq!(vesting_res, Uint128::new(0u128));

    let query_total_unclamed = UnclaimedTotalAmountAtHeight {
        height: start_block_height - 1,
    };
    let vesting_res: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &query_total_unclamed)
        .unwrap();
    assert_eq!(vesting_res, Uint128::new(0u128));
    let max_unclaimed_user1: u128 = 200;
    let max_unclaimed_total: u128 = 1200;
    for i in 0..=10 {
        let query = UnclaimedAmountAtHeight {
            address: user1.to_string(),
            height: start_block_height + 1 + i * 10,
        };
        let vesting_res: Uint128 = app
            .wrap()
            .query_wasm_smart(vesting_instance.clone(), &query)
            .unwrap();
        assert_eq!(
            vesting_res,
            Uint128::new(max_unclaimed_user1 - (i as u128) * 20)
        );

        let query_total_unclamed = UnclaimedTotalAmountAtHeight {
            height: start_block_height + 1 + i * 10,
        };
        let vesting_res: Uint128 = app
            .wrap()
            .query_wasm_smart(vesting_instance.clone(), &query_total_unclamed)
            .unwrap();
        assert_eq!(
            vesting_res,
            Uint128::new(max_unclaimed_total - (i as u128) * 20)
        );
    }
}

#[test]
fn vesting_managers() {
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);
    let owner = Addr::unchecked(OWNER1);

    let mut app = mock_app(&owner);
    let vesting_instance = instantiate_vesting_remote_chain(&mut app);

    let query = QueryMsg::VestingManagers {};
    let vesting_res: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &query)
        .unwrap();
    assert_eq!(vesting_res.len(), 0,);

    let native_msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user1.to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: app.block_info().time.seconds(),
                    amount: Uint128::zero(),
                },
                end_point: Some(VestingSchedulePoint {
                    time: app
                        .block_info()
                        .time
                        .plus_seconds(100 * BLOCK_TIME)
                        .seconds(),
                    amount: Uint128::new(50),
                }),
            }],
        }],
    };
    let err = app
        .execute_contract(user1.clone(), vesting_instance.clone(), &native_msg, &[])
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let add_manager_msg = ExecuteMsg::AddVestingManagers {
        managers: vec![user1.to_string()],
    };

    let err = app
        .execute_contract(
            user1.clone(),
            vesting_instance.clone(),
            &add_manager_msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let _res = app
        .execute_contract(
            owner.clone(),
            vesting_instance.clone(),
            &add_manager_msg,
            &[],
        )
        .unwrap();

    let vesting_res: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &query)
        .unwrap();
    assert_eq!(vesting_res, vec![Addr::unchecked(user1.clone())]);

    app.send_tokens(owner.clone(), user1.clone(), &coins(50, VESTING_TOKEN))
        .unwrap();

    let _res = app
        .execute_contract(
            user1.clone(),
            vesting_instance.clone(),
            &native_msg,
            &coins(50, VESTING_TOKEN),
        )
        .unwrap();
    let err = app
        .execute_contract(user2, vesting_instance.clone(), &native_msg, &[])
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let remove_manager_msg = ExecuteMsg::RemoveVestingManagers {
        managers: vec![user1.to_string()],
    };
    let err = app
        .execute_contract(user1, vesting_instance.clone(), &remove_manager_msg, &[])
        .unwrap_err();
    assert_eq!(ContractError::Unauthorized {}, err.downcast().unwrap());

    let _res = app
        .execute_contract(owner, vesting_instance.clone(), &remove_manager_msg, &[])
        .unwrap();

    let vesting_res: Vec<Addr> = app
        .wrap()
        .query_wasm_smart(vesting_instance, &query)
        .unwrap();
    assert_eq!(vesting_res.len(), 0);
}

fn mock_app(owner: &Addr) -> App {
    App::new(|app, _, storage| {
        app.bank
            .init_balance(
                storage,
                owner,
                vec![
                    coin(TOKEN_INITIAL_AMOUNT, VESTING_TOKEN),
                    coin(10_000_000_000u128, "random_coin"),
                ],
            )
            .unwrap()
    })
}

fn store_token_code(app: &mut App) -> u64 {
    let cw20_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    app.store_code(cw20_token_contract)
}

fn instantiate_token(app: &mut App, token_code_id: u64, name: &str, cap: Option<u128>) -> Addr {
    let name = String::from(name);

    let msg = TokenInstantiateMsg {
        name: name.clone(),
        symbol: name.clone(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: String::from(OWNER1),
            cap: cap.map(Uint128::from),
        }),
        marketing: None,
    };

    app.instantiate_contract(
        token_code_id,
        Addr::unchecked(OWNER1),
        &msg,
        &[],
        name,
        None,
    )
    .unwrap()
}

fn instantiate_vesting(app: &mut App, cw20_token_instance: &Addr) -> Addr {
    let vesting_contract = Box::new(ContractWrapper::new_with_empty(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    ));
    let owner = Addr::unchecked(OWNER1);
    let token_manager = Addr::unchecked(TOKEN_MANAGER);
    let vesting_code_id = app.store_code(vesting_contract);

    let init_msg = InstantiateMsg {
        owner: OWNER1.to_string(),
        token_info_manager: TOKEN_MANAGER.to_string(),
        vesting_managers: vec![],
    };

    let vesting_instance = app
        .instantiate_contract(
            vesting_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "Vesting",
            None,
        )
        .unwrap();
    let set_vesting_token_msg = ExecuteMsg::SetVestingToken {
        vesting_token: token_asset_info(cw20_token_instance.clone()),
    };
    app.execute_contract(
        token_manager,
        vesting_instance.clone(),
        &set_vesting_token_msg,
        &[],
    )
    .unwrap();

    let res: Config = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &QueryMsg::Config {})
        .unwrap();
    assert_eq!(
        cw20_token_instance.to_string(),
        res.vesting_token.unwrap().to_string()
    );

    mint_tokens(app, cw20_token_instance, &owner, TOKEN_INITIAL_AMOUNT);

    check_token_balance(app, cw20_token_instance, &owner, TOKEN_INITIAL_AMOUNT);

    vesting_instance
}

fn instantiate_vesting_remote_chain(app: &mut App) -> Addr {
    let vesting_contract = Box::new(ContractWrapper::new_with_empty(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    ));
    let owner = Addr::unchecked(OWNER1);
    let token_manager = Addr::unchecked(TOKEN_MANAGER);
    let vesting_code_id = app.store_code(vesting_contract);

    let init_msg = InstantiateMsg {
        owner: OWNER1.to_string(),
        token_info_manager: TOKEN_MANAGER.to_string(),
        vesting_managers: vec![],
    };

    let res = app
        .instantiate_contract(vesting_code_id, owner, &init_msg, &[], "Vesting", None)
        .unwrap();
    let msg = ExecuteMsg::SetVestingToken {
        vesting_token: native_asset_info(VESTING_TOKEN.to_string()),
    };
    app.execute_contract(token_manager, res.clone(), &msg, &[])
        .unwrap();
    res
}

fn mint_tokens(app: &mut App, token: &Addr, recipient: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::Mint {
        recipient: recipient.to_string(),
        amount: Uint128::from(amount),
    };

    app.execute_contract(Addr::unchecked(OWNER1), token.to_owned(), &msg, &[])
        .unwrap();
}

fn check_token_balance(app: &mut App, token: &Addr, address: &Addr, expected: u128) {
    let msg = Cw20QueryMsg::Balance {
        address: address.to_string(),
    };
    let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
    assert_eq!(res.unwrap().balance, Uint128::from(expected));
}
