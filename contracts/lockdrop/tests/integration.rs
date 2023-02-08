use astroport::asset::AssetInfo;
use astroport::restricted_vector::RestrictedVector;
use astroport_governance::utils::EPOCH_START;
use astroport_periphery::{
    auction::{ExecuteMsg as AuctionExecuteMsg, UpdateConfigMsg as AuctionUpdateConfigMsg},
    lockdrop::{
        self, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrationInfo, QueryMsg, StateResponse,
        UpdateConfigMsg, UserInfoResponse,
    },
};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{
    attr, to_binary, Addr, Coin, Decimal, Timestamp, Uint128, Uint256 as CUint256, Uint64,
};

use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use astroport_periphery::lockdrop::{Config, PoolInfo};
use cw20::{Cw20ExecuteMsg, Cw20QueryMsg};
use cw_multi_test::{App, AppBuilder, BankKeeper, ContractWrapper, Executor};

fn mock_app() -> App {
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(EPOCH_START);
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();

    AppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .build(|_, _, _| {})
}

// Instantiate ASTRO Token Contract
fn instantiate_astro_token(app: &mut App, owner: Addr) -> Addr {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let astro_token_code_id = app.store_code(astro_token_contract);

    let msg = TokenInstantiateMsg {
        name: String::from("Astro token"),
        symbol: String::from("ASTRO"),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(cw20::MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
        marketing: None,
    };

    let astro_token_instance = app
        .instantiate_contract(
            astro_token_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ASTRO"),
            None,
        )
        .unwrap();
    astro_token_instance
}

// Instantiate Terraswap
fn instantiate_terraswap(app: &mut App, owner: Addr) -> Addr {
    // Terraswap Pair
    let terraswap_pair_contract = Box::new(ContractWrapper::new_with_empty(
        terraswap_pair::contract::execute,
        terraswap_pair::contract::instantiate,
        terraswap_pair::contract::query,
    ));
    let terraswap_pair_code_id = app.store_code(terraswap_pair_contract);

    // Terraswap LP Token
    let terraswap_token_contract = Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ));
    let terraswap_token_code_id = app.store_code(terraswap_token_contract);

    // Terraswap Factory Contract
    let terraswap_factory_contract = Box::new(ContractWrapper::new_with_empty(
        terraswap_factory::contract::execute,
        terraswap_factory::contract::instantiate,
        terraswap_factory::contract::query,
    ));

    let terraswap_factory_code_id = app.store_code(terraswap_factory_contract);

    let msg = terraswap::factory::InstantiateMsg {
        pair_code_id: terraswap_pair_code_id,
        token_code_id: terraswap_token_code_id,
    };

    let terraswap_factory_instance = app
        .instantiate_contract(
            terraswap_factory_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("Terraswap_Factory"),
            None,
        )
        .unwrap();
    terraswap_factory_instance
}

// Instantiate Astroport
fn instantiate_astroport(app: &mut App, owner: Addr) -> Addr {
    let mut pair_configs = vec![];
    // Astroport Pair
    let astroport_pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair::contract::execute,
            astroport_pair::contract::instantiate,
            astroport_pair::contract::query,
        )
        .with_reply_empty(astroport_pair::contract::reply),
    );
    let astroport_pair_code_id = app.store_code(astroport_pair_contract);
    pair_configs.push(astroport::factory::PairConfig {
        code_id: astroport_pair_code_id,
        pair_type: astroport::factory::PairType::Xyk {},
        total_fee_bps: 5u16,
        maker_fee_bps: 3u16,
        is_disabled: false,
        is_generator_disabled: false,
    });

    // Astroport Pair :: Stable
    let astroport_pair_stable_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair_stable::contract::execute,
            astroport_pair_stable::contract::instantiate,
            astroport_pair_stable::contract::query,
        )
        .with_reply_empty(astroport_pair_stable::contract::reply),
    );
    let astroport_pair_stable_code_id = app.store_code(astroport_pair_stable_contract);
    pair_configs.push(astroport::factory::PairConfig {
        code_id: astroport_pair_stable_code_id,
        pair_type: astroport::factory::PairType::Stable {},
        total_fee_bps: 5u16,
        maker_fee_bps: 3u16,
        is_disabled: false,
        is_generator_disabled: false,
    });

    // Astroport LP Token
    let astroport_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));
    let astroport_token_code_id = app.store_code(astroport_token_contract);

    // Astroport Factory Contract
    let astroport_factory_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_factory::contract::execute,
            astroport_factory::contract::instantiate,
            astroport_factory::contract::query,
        )
        .with_reply_empty(astroport_factory::contract::reply),
    );

    let astroport_factory_code_id = app.store_code(astroport_factory_contract);

    let whitelist_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_whitelist::contract::execute,
        astroport_whitelist::contract::instantiate,
        astroport_whitelist::contract::query,
    ));

    let whitelist_code_id = app.store_code(whitelist_contract);

    let msg = astroport::factory::InstantiateMsg {
        /// Pair contract code IDs which are allowed to create pairs
        pair_configs,
        token_code_id: astroport_token_code_id,
        fee_address: Some("fee_address".to_string()),
        generator_address: Some("generator_address".to_string()),
        owner: owner.clone().to_string(),
        whitelist_code_id,
    };

    let astroport_factory_instance = app
        .instantiate_contract(
            astroport_factory_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("Astroport_Factory"),
            None,
        )
        .unwrap();
    astroport_factory_instance
}

// Instantiate Astroport's generator and vesting contracts
fn instantiate_generator_and_vesting(
    app: &mut App,
    owner: Addr,
    astro_token_instance: Addr,
    astro_factory_instance: Addr,
) -> (Addr, Addr) {
    // Vesting
    let vesting_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_vesting::contract::execute,
        astroport_vesting::contract::instantiate,
        astroport_vesting::contract::query,
    ));
    let vesting_code_id = app.store_code(vesting_contract);

    let init_msg = astroport::vesting::InstantiateMsg {
        owner: owner.to_string(),
        token_addr: astro_token_instance.clone().to_string(),
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

    mint_some_tokens(
        app,
        owner.clone(),
        astro_token_instance.clone(),
        Uint128::new(900_000_000_000),
        owner.to_string(),
    );
    app.execute_contract(
        owner.clone(),
        astro_token_instance.clone(),
        &Cw20ExecuteMsg::IncreaseAllowance {
            spender: vesting_instance.clone().to_string(),
            amount: Uint128::new(900_000_000_000),
            expires: None,
        },
        &[],
    )
    .unwrap();

    let whitelist_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_whitelist::contract::execute,
        astroport_whitelist::contract::instantiate,
        astroport_whitelist::contract::query,
    ));

    let whitelist_code_id = app.store_code(whitelist_contract);

    // Generator
    let generator_contract = Box::new(
        ContractWrapper::new_with_empty(
            astroport_generator::contract::execute,
            astroport_generator::contract::instantiate,
            astroport_generator::contract::query,
        )
        .with_reply_empty(astroport_generator::contract::reply),
    );

    let generator_code_id = app.store_code(generator_contract);

    let init_msg = astroport::generator::InstantiateMsg {
        allowed_reward_proxies: vec![],
        start_block: Uint64::from(app.block_info().height),
        astro_token: astro_token_instance.to_string(),
        tokens_per_block: Uint128::from(0u128),
        vesting_contract: vesting_instance.clone().to_string(),
        owner: owner.to_string(),
        factory: astro_factory_instance.to_string(),
        generator_controller: None,
        voting_escrow: None,
        guardian: None,
        whitelist_code_id,
    };

    let generator_instance = app
        .instantiate_contract(
            generator_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "Generator",
            None,
        )
        .unwrap();

    let tokens_per_block = Uint128::new(10_000000);

    let msg = astroport::generator::ExecuteMsg::SetTokensPerBlock {
        amount: tokens_per_block,
    };
    app.execute_contract(owner.clone(), generator_instance.clone(), &msg, &[])
        .unwrap();

    let msg = astroport::generator::QueryMsg::Config {};
    let res: astroport::generator::Config = app
        .wrap()
        .query_wasm_smart(&generator_instance, &msg)
        .unwrap();
    assert_eq!(res.tokens_per_block, tokens_per_block);

    // vesting to generator:

    let current_block = app.block_info();

    let amount = Uint128::new(630720000000);

    let msg = Cw20ExecuteMsg::Send {
        contract: vesting_instance.to_string(),
        amount,
        msg: to_binary(&astroport::vesting::Cw20HookMsg::RegisterVestingAccounts {
            vesting_accounts: vec![astroport::vesting::VestingAccount {
                address: generator_instance.to_string(),
                schedules: vec![astroport::vesting::VestingSchedule {
                    start_point: astroport::vesting::VestingSchedulePoint {
                        time: current_block.time.seconds(),
                        amount,
                    },
                    end_point: None,
                }],
            }],
        })
        .unwrap(),
    };

    app.execute_contract(owner.clone(), astro_token_instance.clone(), &msg, &[])
        .unwrap();

    (generator_instance, vesting_instance)
}

// Mints some Tokens to "to" recipient
fn mint_some_tokens(app: &mut App, owner: Addr, token_instance: Addr, amount: Uint128, to: String) {
    let msg = cw20::Cw20ExecuteMsg::Mint {
        recipient: to.clone(),
        amount: amount,
    };
    let res = app
        .execute_contract(owner.clone(), token_instance.clone(), &msg, &[])
        .unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[1].attributes[2], attr("to", to));
    assert_eq!(res.events[1].attributes[3], attr("amount", amount));
}

// Instantiate AUCTION Contract
fn instantiate_auction_contract(
    app: &mut App,
    owner: Addr,
    astro_token_instance: Addr,
    airdrop_instance: Addr,
    lockdrop_instance: Addr,
    pair_instance: Addr,
    generator_instance: Addr,
) -> (Addr, astroport_periphery::auction::InstantiateMsg) {
    let auction_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_auction::contract::execute,
        astroport_auction::contract::instantiate,
        astroport_auction::contract::query,
    ));

    let auction_code_id = app.store_code(auction_contract);

    let auction_instantiate_msg = astroport_periphery::auction::InstantiateMsg {
        owner: Some(owner.to_string()),
        astro_token_address: astro_token_instance.clone().into_string(),
        airdrop_contract_address: airdrop_instance.to_string(),
        lockdrop_contract_address: lockdrop_instance.to_string(),
        lp_tokens_vesting_duration: 7776000u64,
        init_timestamp: EPOCH_START + 10_600_000,
        deposit_window: 100_00_0,
        withdrawal_window: 5_00_00,
    };

    // Init contract
    let auction_instance = app
        .instantiate_contract(
            auction_code_id,
            owner.clone(),
            &auction_instantiate_msg,
            &[],
            "auction",
            None,
        )
        .unwrap();

    app.execute_contract(
        owner.clone(),
        auction_instance.clone(),
        &AuctionExecuteMsg::UpdateConfig {
            new_config: AuctionUpdateConfigMsg {
                astro_ust_pair_address: Some(pair_instance.to_string()),
                owner: None,
                generator_contract: Some(generator_instance.to_string()),
            },
        },
        &[],
    )
    .unwrap();
    (auction_instance, auction_instantiate_msg)
}

// Instantiate LOCKDROP Contract
fn instantiate_lockdrop_contract(app: &mut App, owner: Addr) -> (Addr, InstantiateMsg) {
    let lockdrop_contract = Box::new(ContractWrapper::new_with_empty(
        neutron_lockdrop::contract::execute,
        neutron_lockdrop::contract::instantiate,
        neutron_lockdrop::contract::query,
    ));

    let lockdrop_code_id = app.store_code(lockdrop_contract);

    let lockdrop_instantiate_msg = InstantiateMsg {
        owner: Some(owner.clone().to_string()),
        init_timestamp: EPOCH_START + 100_000,
        deposit_window: 10_000_000,
        withdrawal_window: 500_000,
        min_lock_duration: 1u64,
        max_lock_duration: 52u64,
        weekly_multiplier: 1u64,
        weekly_divider: 12u64,
        max_positions_per_user: 14,
        credit_contract: "credit_contract".to_string(),
    };

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 900_00)
    });

    // Init contract
    let lockdrop_instance = app
        .instantiate_contract(
            lockdrop_code_id,
            owner.clone(),
            &lockdrop_instantiate_msg,
            &[],
            "lockdrop",
            None,
        )
        .unwrap();
    (lockdrop_instance, lockdrop_instantiate_msg)
}

// Instantiate
fn instantiate_all_contracts(
    app: &mut App,
    owner: Addr,
) -> (Addr, Addr, Addr, Addr, UpdateConfigMsg) {
    let (lockdrop_instance, _lockdrop_instantiate_msg) =
        instantiate_lockdrop_contract(app, owner.clone());

    let astro_token = instantiate_astro_token(app, owner.clone());

    // Initiate Terraswap
    let terraswap_factory_instance = instantiate_terraswap(app, owner.clone());

    // Initiate ASTRO-UST Pair on Astroport
    let astroport_factory_instance = instantiate_astroport(app, owner.clone());
    let pair_info = [
        astroport::asset::AssetInfo::Token {
            contract_addr: astro_token.clone(),
        },
        astroport::asset::AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        },
    ];
    app.execute_contract(
        Addr::unchecked("user"),
        astroport_factory_instance.clone(),
        &astroport::factory::ExecuteMsg::CreatePair {
            asset_infos: pair_info.clone(),
            init_params: None,
            pair_type: astroport::factory::PairType::Xyk {},
        },
        &[],
    )
    .unwrap();
    let pair_resp: astroport::asset::PairInfo = app
        .wrap()
        .query_wasm_smart(
            &astroport_factory_instance,
            &astroport::factory::QueryMsg::Pair {
                asset_infos: pair_info.clone(),
            },
        )
        .unwrap();
    let pool_address = pair_resp.contract_addr;

    let (generator_address, _) = instantiate_generator_and_vesting(
        app,
        owner.clone(),
        astro_token.clone(),
        astroport_factory_instance.clone(),
    );

    // Airdrop Contract
    let airdrop_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_airdrop::contract::execute,
        astroport_airdrop::contract::instantiate,
        astroport_airdrop::contract::query,
    ));

    let airdrop_code_id = app.store_code(airdrop_contract);

    let airdrop_msg = astroport_periphery::airdrop::InstantiateMsg {
        owner: Some(owner.clone().to_string()),
        astro_token_address: astro_token.clone().into_string(),
        merkle_roots: Some(vec!["merkle_roots".to_string()]),
        from_timestamp: Some(1_000_00),
        to_timestamp: 10000_000_00,
    };

    let airdrop_instance = app
        .instantiate_contract(
            airdrop_code_id,
            owner.clone(),
            &airdrop_msg,
            &[],
            String::from("airdrop_instance"),
            None,
        )
        .unwrap();

    // Initiate Auction contract
    let (auction_contract, _) = instantiate_auction_contract(
        app,
        owner.clone(),
        astro_token.clone(),
        airdrop_instance.clone(),
        lockdrop_instance.clone(),
        pool_address,
        generator_address.clone(),
    );

    // Set auction contract in airdrop contract
    app.execute_contract(
        owner.clone(),
        airdrop_instance.clone(),
        &astroport_periphery::airdrop::ExecuteMsg::UpdateConfig {
            owner: None,
            auction_contract_address: Some(auction_contract.clone().to_string()),
            merkle_roots: None,
            from_timestamp: None,
            to_timestamp: None,
        },
        &[],
    )
    .unwrap();

    let update_msg = UpdateConfigMsg {
        astro_token_address: Some(astro_token.to_string()),
        auction_contract_address: Some(auction_contract.to_string()),
        generator_address: Some(generator_address.to_string()),
    };
    app.execute_contract(
        owner.clone(),
        astro_token.clone(),
        &Cw20ExecuteMsg::IncreaseAllowance {
            spender: lockdrop_instance.clone().to_string(),
            amount: Uint128::new(1000000000u128),
            expires: None,
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        owner.clone(),
        lockdrop_instance.clone(),
        &ExecuteMsg::UpdateConfig {
            new_config: update_msg.clone(),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        owner.clone(),
        astro_token.clone(),
        &Cw20ExecuteMsg::Send {
            amount: Uint128::from(1000000000u64),
            contract: lockdrop_instance.to_string(),
            msg: to_binary(&Cw20HookMsg::IncreaseNTRNIncentives {}).unwrap(),
        },
        &[],
    )
    .unwrap();

    return (
        astro_token,
        lockdrop_instance,
        astroport_factory_instance,
        terraswap_factory_instance,
        update_msg,
    );
}

// Instantiate Pools and Migrate Liquidity to Astroport
fn initialize_and_migrate_liquidity_for_pool(
    app: &mut App,
    owner: Addr,
    token_instance: Addr,
    lockdrop_instance: Addr,
    astroport_factory_instance: Addr,
) -> (String, Addr, Addr) {
    // Terraswap LP Token
    let terraswap_token_contract = Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ));
    let terraswap_token_code_id = app.store_code(terraswap_token_contract);

    // Terraswap Pair
    let terraswap_pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            terraswap_pair::contract::execute,
            terraswap_pair::contract::instantiate,
            terraswap_pair::contract::query,
        )
        .with_reply_empty(terraswap_pair::contract::reply),
    );
    let terraswap_pair_code_id = app.store_code(terraswap_pair_contract);

    // LP POOL INSTANCE
    let terraswap_pool_instance = app
        .instantiate_contract(
            terraswap_pair_code_id,
            Addr::unchecked("user".to_string()),
            &terraswap::pair::InstantiateMsg {
                asset_infos: [
                    terraswap::asset::AssetInfo::Token {
                        contract_addr: token_instance.clone().to_string(),
                    },
                    terraswap::asset::AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
                token_code_id: terraswap_token_code_id,
                asset_decimals: [6, 6],
            },
            &[],
            String::from("terraswap_pool"),
            None,
        )
        .unwrap();

    // Query LP Token
    let pair_response: terraswap::asset::PairInfo = app
        .wrap()
        .query_wasm_smart(
            &terraswap_pool_instance,
            &terraswap::pair::QueryMsg::Pair {},
        )
        .unwrap();
    let terraswap_token_instance = pair_response.liquidity_token;

    // SUCCESSFULLY INITIALIZES POOL
    app.execute_contract(
        owner.clone(),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::InitializePool {
            terraswap_lp_token: terraswap_token_instance.to_string(),
            incentives_share: 10000000u64,
        },
        &[],
    )
    .unwrap();

    let user_address = "user".to_string();
    let user2_address = "user2".to_string();

    // Mint ANC to users
    app.execute_contract(
        owner.clone(),
        token_instance.clone(),
        &cw20::Cw20ExecuteMsg::Mint {
            recipient: user_address.clone(),
            amount: Uint128::from(10000_000000u64),
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        owner.clone(),
        token_instance.clone(),
        &cw20::Cw20ExecuteMsg::Mint {
            recipient: user2_address.clone(),
            amount: Uint128::from(10000_000000u64),
        },
        &[],
    )
    .unwrap();

    // Set UST user balances
    app.init_modules(|router, _, storage| {
        router
            .bank
            .init_balance(
                storage,
                &Addr::unchecked(user_address.clone()),
                vec![Coin::new(1000000_000000, "uusd")],
            )
            .unwrap();
        router
            .bank
            .init_balance(
                storage,
                &Addr::unchecked(user2_address.clone()),
                vec![Coin::new(1000000_000000, "uusd")],
            )
            .unwrap();
    });

    // user#1 adds liquidity to Terraswap Pool and locks that in Lockdrop contract
    // increase allowance
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        token_instance.clone(),
        &Cw20ExecuteMsg::IncreaseAllowance {
            spender: terraswap_pool_instance.clone().to_string(),
            amount: Uint128::new(1000_000000),
            expires: None,
        },
        &[],
    )
    .unwrap();

    // add Liquidity to Terraswap pool
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        terraswap_pool_instance.clone(),
        &terraswap::pair::ExecuteMsg::ProvideLiquidity {
            assets: [
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::Token {
                        contract_addr: token_instance.clone().to_string(),
                    },
                    amount: Uint128::from(1000_000000u64),
                },
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                    amount: Uint128::from(1000_000000u64),
                },
            ],
            slippage_tolerance: None,
            receiver: None,
        },
        &[Coin::new(1000_000000, "uusd")],
    )
    .unwrap();

    // Query LP balance
    let lp_balance_res: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &terraswap_token_instance.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: user_address.clone(),
            },
        )
        .unwrap();
    let user_lp_balance = lp_balance_res.balance;

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 1_000_00)
    });

    // Lock LP Tokens into Lockup Position
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        Addr::unchecked(terraswap_token_instance.clone()),
        &cw20::Cw20ExecuteMsg::Send {
            contract: lockdrop_instance.clone().to_string(),
            amount: user_lp_balance,
            msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 10u64 }).unwrap(),
        },
        &[],
    )
    .unwrap();

    // user#2 adds liquidity to Terraswap Pool and locks that in Lockdrop contract
    // increase allowance
    app.execute_contract(
        Addr::unchecked(user2_address.clone()),
        token_instance.clone(),
        &Cw20ExecuteMsg::IncreaseAllowance {
            spender: terraswap_pool_instance.clone().to_string(),
            amount: Uint128::new(1000_000000),
            expires: None,
        },
        &[],
    )
    .unwrap();

    // add Liquidity to Terraswap pool
    app.execute_contract(
        Addr::unchecked(user2_address.clone()),
        terraswap_pool_instance.clone(),
        &terraswap::pair::ExecuteMsg::ProvideLiquidity {
            assets: [
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::Token {
                        contract_addr: token_instance.clone().to_string(),
                    },
                    amount: Uint128::from(1000_000000u64),
                },
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                    amount: Uint128::from(1000_000000u64),
                },
            ],
            slippage_tolerance: None,
            receiver: None,
        },
        &[Coin::new(1000_000000, "uusd")],
    )
    .unwrap();

    // Query LP balance
    let lp_balance_res: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &terraswap_token_instance.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: user2_address.clone(),
            },
        )
        .unwrap();
    let user_lp_balance = lp_balance_res.balance;

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 1_000_00)
    });

    // Lock LP Tokens into Lockup Position
    app.execute_contract(
        Addr::unchecked(user2_address.clone()),
        Addr::unchecked(terraswap_token_instance.clone()),
        &cw20::Cw20ExecuteMsg::Send {
            contract: lockdrop_instance.clone().to_string(),
            amount: user_lp_balance,
            msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 10u64 }).unwrap(),
        },
        &[],
    )
    .unwrap();

    // Increase timestamp for window closure
    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 10600001)
    });

    // Create Astroport Pair
    app.execute_contract(
        Addr::unchecked("user"),
        astroport_factory_instance.clone(),
        &astroport::factory::ExecuteMsg::CreatePair {
            asset_infos: [
                astroport::asset::AssetInfo::Token {
                    contract_addr: token_instance.clone(),
                },
                astroport::asset::AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            init_params: None,
            pair_type: astroport::factory::PairType::Xyk {},
        },
        &[],
    )
    .unwrap();

    // Query Astroport addresses
    let pair_resp: astroport::asset::PairInfo = app
        .wrap()
        .query_wasm_smart(
            &astroport_factory_instance,
            &astroport::factory::QueryMsg::Pair {
                asset_infos: [
                    astroport::asset::AssetInfo::Token {
                        contract_addr: token_instance.clone(),
                    },
                    astroport::asset::AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
            },
        )
        .unwrap();
    let astro_pool_address = pair_resp.contract_addr;
    let astro_lp_address = pair_resp.liquidity_token;

    // Migrate Liquidity
    app.execute_contract(
        owner.clone(),
        lockdrop_instance.clone(),
        &ExecuteMsg::MigrateLiquidity {
            terraswap_lp_token: terraswap_token_instance.clone(),
            astroport_pool_addr: astro_pool_address.to_string(),
            slippage_tolerance: None,
        },
        &[],
    )
    .unwrap();

    return (
        terraswap_token_instance,
        astro_lp_address,
        astro_pool_address,
    );
}

#[test]
fn proper_initialization_lockdrop() {
    let owner = Addr::unchecked("contract_owner");
    let mut app = mock_app();

    let (lockdrop_instance, lockdrop_instantiate_msg) =
        instantiate_lockdrop_contract(&mut app, owner);

    let resp: Config = app
        .wrap()
        .query_wasm_smart(&lockdrop_instance, &QueryMsg::Config {})
        .unwrap();

    // Check config
    assert_eq!(
        lockdrop_instantiate_msg.owner.unwrap().to_string(),
        resp.owner
    );
    assert_eq!(None, resp.ntrn_token);
    assert_eq!(None, resp.auction_contract);
    assert_eq!(None, resp.generator);
    assert_eq!(lockdrop_instantiate_msg.init_timestamp, resp.init_timestamp);
    assert_eq!(lockdrop_instantiate_msg.deposit_window, resp.deposit_window);
    assert_eq!(
        lockdrop_instantiate_msg.withdrawal_window,
        resp.withdrawal_window
    );
    assert_eq!(
        lockdrop_instantiate_msg.min_lock_duration,
        resp.min_lock_duration
    );
    assert_eq!(
        lockdrop_instantiate_msg.max_lock_duration,
        resp.max_lock_duration
    );
    assert_eq!(
        lockdrop_instantiate_msg.weekly_multiplier,
        resp.weekly_multiplier
    );
    assert_eq!(lockdrop_instantiate_msg.weekly_divider, resp.weekly_divider);
    assert_eq!(Uint128::zero(), resp.lockdrop_incentives);

    // Check state
    let resp: StateResponse = app
        .wrap()
        .query_wasm_smart(&lockdrop_instance, &QueryMsg::State {})
        .unwrap();

    assert_eq!(0u64, resp.total_incentives_share);
    assert_eq!(false, resp.are_claims_allowed);
}

#[test]
fn test_update_config() {
    let mut app = mock_app();
    let owner = Addr::unchecked("contract_owner");

    let (lockdrop_instance, _lockdrop_instantiate_msg) =
        instantiate_lockdrop_contract(&mut app, owner.clone());

    let astro_token = instantiate_astro_token(&mut app, owner.clone());

    // Initiate ASTRO-UST Pair on Astroport
    let astroport_factory_instance = instantiate_astroport(&mut app, owner.clone());
    let pair_info = [
        astroport::asset::AssetInfo::Token {
            contract_addr: astro_token.clone(),
        },
        astroport::asset::AssetInfo::NativeToken {
            denom: "uusd".to_string(),
        },
    ];
    app.execute_contract(
        Addr::unchecked("user"),
        astroport_factory_instance.clone(),
        &astroport::factory::ExecuteMsg::CreatePair {
            asset_infos: pair_info.clone(),
            init_params: None,
            pair_type: astroport::factory::PairType::Xyk {},
        },
        &[],
    )
    .unwrap();
    let pair_resp: astroport::asset::PairInfo = app
        .wrap()
        .query_wasm_smart(
            &astroport_factory_instance,
            &astroport::factory::QueryMsg::Pair {
                asset_infos: pair_info.clone(),
            },
        )
        .unwrap();
    let pool_address = pair_resp.contract_addr;

    let (generator_address, _) = instantiate_generator_and_vesting(
        &mut app,
        owner.clone(),
        astro_token.clone(),
        astroport_factory_instance.clone(),
    );

    // Initiate Auction contract
    let (auction_contract, _) = instantiate_auction_contract(
        &mut app,
        owner.clone(),
        astro_token.clone(),
        Addr::unchecked("auction_instance"),
        lockdrop_instance.clone(),
        pool_address,
        generator_address.clone(),
    );

    let update_msg = UpdateConfigMsg {
        astro_token_address: Some(astro_token.to_string()),
        auction_contract_address: Some(auction_contract.to_string()),
        generator_address: Some(generator_address.to_string()),
    };

    // ######    ERROR :: Unauthorized     ######
    let err = app
        .execute_contract(
            Addr::unchecked("wrong_owner"),
            lockdrop_instance.clone(),
            &ExecuteMsg::UpdateConfig {
                new_config: update_msg.clone(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // ######    SUCCESS :: Should have successfully updated   ######

    app.execute_contract(
        owner.clone(),
        lockdrop_instance.clone(),
        &ExecuteMsg::UpdateConfig {
            new_config: update_msg.clone(),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        owner.clone(),
        astro_token.clone(),
        &Cw20ExecuteMsg::Send {
            amount: Uint128::from(1000000000u64),
            contract: lockdrop_instance.to_string(),
            msg: to_binary(&Cw20HookMsg::IncreaseNTRNIncentives {}).unwrap(),
        },
        &[],
    )
    .unwrap();

    let resp: Config = app
        .wrap()
        .query_wasm_smart(&lockdrop_instance, &QueryMsg::Config {})
        .unwrap();

    assert_eq!(
        update_msg.clone().astro_token_address.unwrap(),
        resp.ntrn_token.unwrap()
    );
    assert_eq!(
        update_msg.clone().generator_address.unwrap(),
        resp.generator.unwrap()
    );
    assert_eq!(Uint128::from(1000000000u64), resp.lockdrop_incentives);

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 10600001)
    });

    // ######    ERROR :: ASTRO tokens are live. Configuration cannot be updated now     ######
    app.execute_contract(
        Addr::unchecked(auction_contract),
        lockdrop_instance.clone(),
        &ExecuteMsg::EnableClaims {},
        &[],
    )
    .unwrap();

    let err = app
        .execute_contract(
            owner.clone(),
            lockdrop_instance.clone(),
            &ExecuteMsg::UpdateConfig {
                new_config: update_msg.clone(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: ASTRO token already set"
    );
}

#[test]
fn test_initialize_pool() {
    let mut app = mock_app();
    let owner = Addr::unchecked("contract_owner");

    let (_, lockdrop_instance, _, _, _update_msg) =
        instantiate_all_contracts(&mut app, owner.clone());

    // Terraswap LP Token
    let terraswap_token_contract = Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ));
    let terraswap_token_code_id = app.store_code(terraswap_token_contract);

    let terraswap_token_instance = app
        .instantiate_contract(
            terraswap_token_code_id,
            Addr::unchecked("user".to_string()),
            &terraswap::token::InstantiateMsg {
                name: "terraswap liquidity token".to_string(),
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(cw20::MinterResponse {
                    minter: "pair_instance".to_string(),
                    cap: None,
                }),
            },
            &[],
            String::from("terraswap_lp_token"),
            None,
        )
        .unwrap();

    let initialize_pool_msg = astroport_periphery::lockdrop::ExecuteMsg::InitializePool {
        terraswap_lp_token: terraswap_token_instance.to_string(),
        incentives_share: 10000000u64,
    };

    // ######    ERROR :: Unauthorized     ######
    let err = app
        .execute_contract(
            Addr::unchecked("wrong_owner"),
            lockdrop_instance.clone(),
            &initialize_pool_msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // ######    SUCCESS :: SHOULD SUCCESSFULLY INITIALIZE     ######
    app.execute_contract(
        owner.clone(),
        lockdrop_instance.clone(),
        &initialize_pool_msg,
        &[],
    )
    .unwrap();
    // check state
    let state_resp: StateResponse = app
        .wrap()
        .query_wasm_smart(&lockdrop_instance, &QueryMsg::State {})
        .unwrap();
    assert_eq!(10000000u64, state_resp.total_incentives_share);
    assert_eq!(false, state_resp.are_claims_allowed);
    assert_eq!(
        vec![terraswap_token_instance.clone()],
        state_resp.supported_pairs_list
    );
    // check Pool Info
    let pool_resp: PoolInfo = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::Pool {
                terraswap_lp_token: terraswap_token_instance.clone().to_string(),
            },
        )
        .unwrap();
    assert_eq!("pair_instance".to_string(), pool_resp.terraswap_pool);
    assert_eq!(Uint128::zero(), pool_resp.terraswap_amount_in_lockups);
    assert_eq!(None, pool_resp.migration_info);
    assert_eq!(10000000u64, pool_resp.incentives_share);
    assert_eq!(CUint256::zero(), pool_resp.weighted_amount);
    // assert_eq!(Decimal::zero(), pool_resp.generator_astro_per_share);
    // assert_eq!(Decimal::zero(), pool_resp.generator_proxy_per_share);
    assert_eq!(false, pool_resp.is_staked);

    // ######    ERROR :: Already supported     ######
    let err = app
        .execute_contract(
            owner.clone(),
            lockdrop_instance.clone(),
            &initialize_pool_msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Already supported"
    );

    // ######    SUCCESS :: SHOULD SUCCESSFULLY INITIALIZE #2    ######

    let terraswap_token_instance2 = app
        .instantiate_contract(
            terraswap_token_code_id,
            Addr::unchecked("user".to_string()),
            &terraswap::token::InstantiateMsg {
                name: "terraswap liquidity token #2".to_string(),
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(cw20::MinterResponse {
                    minter: "pair_instance#2".to_string(),
                    cap: None,
                }),
            },
            &[],
            String::from("terraswap_lp_token#2"),
            None,
        )
        .unwrap();

    app.execute_contract(
        owner.clone(),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::InitializePool {
            terraswap_lp_token: terraswap_token_instance2.to_string(),
            incentives_share: 10400000u64,
        },
        &[],
    )
    .unwrap();
    // check state
    let state_resp: StateResponse = app
        .wrap()
        .query_wasm_smart(&lockdrop_instance, &QueryMsg::State {})
        .unwrap();
    assert_eq!(20400000u64, state_resp.total_incentives_share);
    assert_eq!(
        vec![
            terraswap_token_instance.clone(),
            terraswap_token_instance2.clone(),
        ],
        state_resp.supported_pairs_list
    );
    // check Pool Info
    let pool_resp: PoolInfo = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::Pool {
                terraswap_lp_token: terraswap_token_instance2.clone().to_string(),
            },
        )
        .unwrap();
    assert_eq!("pair_instance#2".to_string(), pool_resp.terraswap_pool);
    assert_eq!(Uint128::zero(), pool_resp.terraswap_amount_in_lockups);
    assert_eq!(None, pool_resp.migration_info);
    assert_eq!(10400000u64, pool_resp.incentives_share);

    // ######    ERROR :: Pools cannot be added post deposit window closure     ######
    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 900000_00)
    });
    let err = app
        .execute_contract(
            owner.clone(),
            lockdrop_instance.clone(),
            &initialize_pool_msg,
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Pools cannot be added post deposit window closure"
    );
}

#[test]
fn test_increase_lockup() {
    let mut app = mock_app();
    let owner = Addr::unchecked("contract_owner");

    let (_, lockdrop_instance, _, _, _) = instantiate_all_contracts(&mut app, owner.clone());

    // Terraswap LP Token
    let terraswap_token_contract = Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ));
    let terraswap_token_code_id = app.store_code(terraswap_token_contract);

    // LP Token #1
    let terraswap_token_instance = app
        .instantiate_contract(
            terraswap_token_code_id,
            Addr::unchecked("user".to_string()),
            &terraswap::token::InstantiateMsg {
                name: "terraswap liquidity token".to_string(),
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(cw20::MinterResponse {
                    minter: "pair_instance".to_string(),
                    cap: None,
                }),
            },
            &[],
            String::from("terraswap_lp_token"),
            None,
        )
        .unwrap();

    // LP Token #2
    let terraswap_token_instance2 = app
        .instantiate_contract(
            terraswap_token_code_id,
            Addr::unchecked("user".to_string()),
            &terraswap::token::InstantiateMsg {
                name: "terraswap liquidity token".to_string(),
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(cw20::MinterResponse {
                    minter: "pair2_instance".to_string(),
                    cap: None,
                }),
            },
            &[],
            String::from("terraswap_lp_token2"),
            None,
        )
        .unwrap();

    // SUCCESSFULLY INITIALIZES POOL
    app.execute_contract(
        owner.clone(),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::InitializePool {
            terraswap_lp_token: terraswap_token_instance.to_string(),
            incentives_share: 10000000u64,
        },
        &[],
    )
    .unwrap();

    let user_address = "user".to_string();
    let user2_address = "user2".to_string();

    // Mint some LP tokens to user#1
    app.execute_contract(
        Addr::unchecked("pair_instance".to_string()),
        terraswap_token_instance.clone(),
        &cw20::Cw20ExecuteMsg::Mint {
            recipient: user_address.clone(),
            amount: Uint128::from(124231343u128),
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        Addr::unchecked("pair2_instance".to_string()),
        terraswap_token_instance2.clone(),
        &cw20::Cw20ExecuteMsg::Mint {
            recipient: user_address.clone(),
            amount: Uint128::from(100000000u128),
        },
        &[],
    )
    .unwrap();

    // Mint some LP tokens to user#2
    app.execute_contract(
        Addr::unchecked("pair_instance".to_string()),
        terraswap_token_instance.clone(),
        &cw20::Cw20ExecuteMsg::Mint {
            recipient: user2_address.clone(),
            amount: Uint128::from(124231343u128),
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        Addr::unchecked("pair2_instance".to_string()),
        terraswap_token_instance2.clone(),
        &cw20::Cw20ExecuteMsg::Mint {
            recipient: user2_address.clone(),
            amount: Uint128::from(100000000u128),
        },
        &[],
    )
    .unwrap();

    // ######    ERROR :: LP Pool not supported    ######
    let err = app
        .execute_contract(
            Addr::unchecked(user_address.clone()),
            terraswap_token_instance2.clone(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: lockdrop_instance.clone().to_string(),
                amount: Uint128::from(10000u128),
                msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 4u64 }).unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "astroport_periphery::lockdrop::PoolInfo not found"
    );

    // ######    ERROR :: Deposit window closed (havent opened)   ######

    let err = app
        .execute_contract(
            Addr::unchecked(user_address.clone()),
            terraswap_token_instance.clone(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: lockdrop_instance.clone().to_string(),
                amount: Uint128::from(10000u128),
                msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 5u64 }).unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Deposit window closed"
    );

    // ######    ERROR :: Lockup duration needs to be between 1 and 52   ######

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 1_000_00)
    });

    let err = app
        .execute_contract(
            Addr::unchecked(user_address.clone()),
            terraswap_token_instance.clone(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: lockdrop_instance.clone().to_string(),
                amount: Uint128::from(10000u128),
                msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 0u64 }).unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Lockup duration needs to be between 1 and 52"
    );

    let err = app
        .execute_contract(
            Addr::unchecked(user_address.clone()),
            terraswap_token_instance.clone(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: lockdrop_instance.clone().to_string(),
                amount: Uint128::from(10000u128),
                msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 53u64 }).unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Lockup duration needs to be between 1 and 52"
    );

    // ######    SUCCESS :: SHOULD SUCCESSFULLY DEPOSIT LP TOKENS INTO POOL     ######
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        terraswap_token_instance.clone(),
        &cw20::Cw20ExecuteMsg::Send {
            contract: lockdrop_instance.clone().to_string(),
            amount: Uint128::from(10000u128),
            msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 5u64 }).unwrap(),
        },
        &[],
    )
    .unwrap();

    // check Pool Info
    let pool_resp: PoolInfo = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::Pool {
                terraswap_lp_token: terraswap_token_instance.clone().to_string(),
            },
        )
        .unwrap();
    assert_eq!(
        Uint128::from(10000u128),
        pool_resp.terraswap_amount_in_lockups
    );
    assert_eq!(CUint256::from(13333u64), pool_resp.weighted_amount);
    assert_eq!(10000000u64, pool_resp.incentives_share);

    // check User Info
    let user_resp: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(1000000000u64), user_resp.total_astro_rewards);
    assert_eq!(false, user_resp.astro_transferred);

    assert_eq!(
        Uint128::from(10000u128),
        user_resp.lockup_infos[0].lp_units_locked
    );
    assert_eq!(false, user_resp.lockup_infos[0].withdrawal_flag);
    assert_eq!(
        user_resp.total_astro_rewards,
        user_resp.lockup_infos[0].astro_rewards
    );
    assert_eq!(5u64, user_resp.lockup_infos[0].duration);
    assert_eq!(
        Uint128::zero(),
        user_resp.lockup_infos[0].generator_astro_debt
    );
    assert_eq!(
        RestrictedVector::<AssetInfo, Uint128>::default(),
        user_resp.lockup_infos[0].generator_proxy_debt
    );
    assert_eq!(
        EPOCH_START + 13624000u64,
        user_resp.lockup_infos[0].unlock_timestamp
    );
    assert_eq!(None, user_resp.lockup_infos[0].astroport_lp_units);
    assert_eq!(None, user_resp.lockup_infos[0].astroport_lp_token);

    // ######    SUCCESS :: SHOULD SUCCESSFULLY DEPOSIT LP TOKENS INTO POOL (2nd USER)     ######
    app.execute_contract(
        Addr::unchecked(user2_address.clone()),
        terraswap_token_instance.clone(),
        &cw20::Cw20ExecuteMsg::Send {
            contract: lockdrop_instance.clone().to_string(),
            amount: Uint128::from(10000u128),
            msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 10u64 }).unwrap(),
        },
        &[],
    )
    .unwrap();

    // check Pool Info
    let pool_resp: PoolInfo = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::Pool {
                terraswap_lp_token: terraswap_token_instance.clone().to_string(),
            },
        )
        .unwrap();
    assert_eq!(
        Uint128::from(20000u128),
        pool_resp.terraswap_amount_in_lockups
    );
    assert_eq!(CUint256::from(30833u64), pool_resp.weighted_amount);

    // check User Info
    let user_resp: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user2_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(567573703u64), user_resp.total_astro_rewards);
    assert_eq!(
        Uint128::from(10000u128),
        user_resp.lockup_infos[0].lp_units_locked
    );
    assert_eq!(
        user_resp.total_astro_rewards,
        user_resp.lockup_infos[0].astro_rewards
    );
    assert_eq!(
        EPOCH_START + 16648000u64,
        user_resp.lockup_infos[0].unlock_timestamp
    );

    // check User#1 Info (ASTRO rewards should be the latest one)
    let user_resp: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(432426296u128), user_resp.total_astro_rewards);
    assert_eq!(
        Uint128::from(10000u128),
        user_resp.lockup_infos[0].lp_units_locked
    );

    // ######    SUCCESS :: SHOULD SUCCESSFULLY AGAIN DEPOSIT LP TOKENS INTO POOL     ######
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        terraswap_token_instance.clone(),
        &cw20::Cw20ExecuteMsg::Send {
            contract: lockdrop_instance.clone().to_string(),
            amount: Uint128::from(10u128),
            msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 51u64 }).unwrap(),
        },
        &[],
    )
    .unwrap();

    // check Pool Info
    let pool_resp: PoolInfo = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::Pool {
                terraswap_lp_token: terraswap_token_instance.clone().to_string(),
            },
        )
        .unwrap();
    assert_eq!(
        Uint128::from(20010u128),
        pool_resp.terraswap_amount_in_lockups
    );

    // check User Info
    let user_resp: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(
        Uint128::from(10u128),
        user_resp.lockup_infos[1].lp_units_locked
    );
    assert_eq!(51u64, user_resp.lockup_infos[1].duration);
    assert_eq!(Uint128::from(433363553u128), user_resp.total_astro_rewards);

    // ######    ERROR :: Deposit window closed   ######
    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 900_000000)
    });

    let err = app
        .execute_contract(
            Addr::unchecked(user_address.clone()),
            terraswap_token_instance.clone(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: lockdrop_instance.clone().to_string(),
                amount: Uint128::from(100u128),
                msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 5u64 }).unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Deposit window closed"
    );
}

#[test]
fn test_migrate_liquidity() {
    let owner = Addr::unchecked("contract_owner");
    let mut app = mock_app();

    let (_, lockdrop_instance, astroport_factory_instance, _, _) =
        instantiate_all_contracts(&mut app, owner.clone());

    // CW20 TOKEN :: Dummy token
    let cw20_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let cw20_code_id = app.store_code(cw20_contract);

    let anc_instance = app
        .instantiate_contract(
            cw20_code_id,
            owner.clone(),
            &TokenInstantiateMsg {
                name: String::from("ANC"),
                symbol: String::from("ANC"),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(cw20::MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            String::from("ANC"),
            None,
        )
        .unwrap();

    // Terraswap LP Token
    let terraswap_token_contract = Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ));
    let terraswap_token_code_id = app.store_code(terraswap_token_contract);

    // Terraswap Pair
    let terraswap_pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            terraswap_pair::contract::execute,
            terraswap_pair::contract::instantiate,
            terraswap_pair::contract::query,
        )
        .with_reply_empty(terraswap_pair::contract::reply),
    );
    let terraswap_pair_code_id = app.store_code(terraswap_pair_contract);

    // LP POOL INSTANCE
    let terraswap_pool_instance = app
        .instantiate_contract(
            terraswap_pair_code_id,
            Addr::unchecked("user".to_string()),
            &terraswap::pair::InstantiateMsg {
                asset_infos: [
                    terraswap::asset::AssetInfo::Token {
                        contract_addr: anc_instance.clone().to_string(),
                    },
                    terraswap::asset::AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
                token_code_id: terraswap_token_code_id,
                asset_decimals: [6, 6],
            },
            &[],
            String::from("terraswap_pool"),
            None,
        )
        .unwrap();

    // Query LP Token
    let pair_response: terraswap::asset::PairInfo = app
        .wrap()
        .query_wasm_smart(
            &terraswap_pool_instance,
            &terraswap::pair::QueryMsg::Pair {},
        )
        .unwrap();
    let terraswap_token_instance = pair_response.liquidity_token;

    // SUCCESSFULLY INITIALIZES POOL
    app.execute_contract(
        owner.clone(),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::InitializePool {
            terraswap_lp_token: terraswap_token_instance.to_string(),
            incentives_share: 10000000u64,
        },
        &[],
    )
    .unwrap();

    let user_address = "user".to_string();
    let user2_address = "user2".to_string();

    // Mint ANC to users
    app.execute_contract(
        owner.clone(),
        anc_instance.clone(),
        &cw20::Cw20ExecuteMsg::Mint {
            recipient: user_address.clone(),
            amount: Uint128::from(10000_000000u64),
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        owner.clone(),
        anc_instance.clone(),
        &cw20::Cw20ExecuteMsg::Mint {
            recipient: user2_address.clone(),
            amount: Uint128::from(10000_000000u64),
        },
        &[],
    )
    .unwrap();

    // Set UST user balances
    app.init_modules(|router, _, storage| {
        router
            .bank
            .init_balance(
                storage,
                &Addr::unchecked(user_address.clone()),
                vec![Coin::new(1000000_000000, "uusd")],
            )
            .unwrap();
        router
            .bank
            .init_balance(
                storage,
                &Addr::unchecked(user2_address.clone()),
                vec![Coin::new(1000000_000000, "uusd")],
            )
            .unwrap();
    });

    // user#1 adds liquidity to Terraswap Pool and locks that in Lockdrop contract
    // increase allowance
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        anc_instance.clone(),
        &Cw20ExecuteMsg::IncreaseAllowance {
            spender: terraswap_pool_instance.clone().to_string(),
            amount: Uint128::new(1000_000000),
            expires: None,
        },
        &[],
    )
    .unwrap();

    // add Liquidity to Terraswap pool
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        terraswap_pool_instance.clone(),
        &terraswap::pair::ExecuteMsg::ProvideLiquidity {
            assets: [
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::Token {
                        contract_addr: anc_instance.clone().to_string(),
                    },
                    amount: Uint128::from(1000_000000u64),
                },
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                    amount: Uint128::from(1000_000000u64),
                },
            ],
            slippage_tolerance: None,
            receiver: None,
        },
        &[Coin::new(1000_000000, "uusd")],
    )
    .unwrap();

    // Query LP balance
    let lp_balance_res: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &terraswap_token_instance.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: user_address.clone(),
            },
        )
        .unwrap();
    let user_lp_balance = lp_balance_res.balance;

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 1_000_00)
    });

    // Lock LP Tokens into Lockup Position
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        Addr::unchecked(terraswap_token_instance.clone()),
        &cw20::Cw20ExecuteMsg::Send {
            contract: lockdrop_instance.clone().to_string(),
            amount: user_lp_balance,
            msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 10u64 }).unwrap(),
        },
        &[],
    )
    .unwrap();

    // Increase timestamp for window closure
    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 10600001)
    });

    // Create Astroport Pair
    app.execute_contract(
        Addr::unchecked("user"),
        astroport_factory_instance.clone(),
        &astroport::factory::ExecuteMsg::CreatePair {
            asset_infos: [
                astroport::asset::AssetInfo::Token {
                    contract_addr: anc_instance.clone(),
                },
                astroport::asset::AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
            ],
            init_params: None,
            pair_type: astroport::factory::PairType::Xyk {},
        },
        &[],
    )
    .unwrap();

    // Query Astroport addresses
    let pair_resp: astroport::asset::PairInfo = app
        .wrap()
        .query_wasm_smart(
            &astroport_factory_instance,
            &astroport::factory::QueryMsg::Pair {
                asset_infos: [
                    astroport::asset::AssetInfo::Token {
                        contract_addr: anc_instance.clone(),
                    },
                    astroport::asset::AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
            },
        )
        .unwrap();
    let astro_pool_address = pair_resp.contract_addr;
    let astro_lp_address = pair_resp.liquidity_token;

    // astro LP token balance (Lockdrop)
    let terraswap_balance_resp: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &terraswap_token_instance,
            &Cw20QueryMsg::Balance {
                address: lockdrop_instance.clone().to_string(),
            },
        )
        .unwrap();

    // Query pool before migration
    let pool_resp_before_migration: PoolInfo = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::Pool {
                terraswap_lp_token: terraswap_token_instance.clone().to_string(),
            },
        )
        .unwrap();

    assert_eq!(
        terraswap_balance_resp.balance,
        pool_resp_before_migration.terraswap_amount_in_lockups
    );
    assert_eq!(
        CUint256::from(1750000000u128),
        pool_resp_before_migration.weighted_amount
    );
    assert_eq!(false, pool_resp_before_migration.is_staked);
    assert_eq!(None, pool_resp_before_migration.migration_info);

    // Migrate Liquidity
    app.execute_contract(
        owner.clone(),
        lockdrop_instance.clone(),
        &ExecuteMsg::MigrateLiquidity {
            terraswap_lp_token: terraswap_token_instance.clone(),
            astroport_pool_addr: astro_pool_address.to_string(),
            slippage_tolerance: None,
        },
        &[],
    )
    .unwrap();

    // astro LP token balance (Lockdrop)
    let astro_balance_resp: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &astro_lp_address,
            &Cw20QueryMsg::Balance {
                address: lockdrop_instance.clone().to_string(),
            },
        )
        .unwrap();

    // Query pool after migration
    let pool_resp_after_migration: PoolInfo = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::Pool {
                terraswap_lp_token: terraswap_token_instance.clone().to_string(),
            },
        )
        .unwrap();

    assert_eq!(
        pool_resp_before_migration.terraswap_pool,
        pool_resp_after_migration.terraswap_pool
    );
    assert_eq!(
        pool_resp_before_migration.terraswap_amount_in_lockups,
        pool_resp_after_migration.terraswap_amount_in_lockups
    );
    assert_eq!(
        pool_resp_before_migration.incentives_share,
        pool_resp_after_migration.incentives_share
    );

    assert_eq!(
        pool_resp_before_migration.incentives_share,
        pool_resp_after_migration.incentives_share
    );
    assert_eq!(
        Decimal::zero(),
        pool_resp_after_migration.generator_ntrn_per_share
    );
    assert_eq!(
        RestrictedVector::default(),
        pool_resp_after_migration.generator_proxy_per_share
    );
    assert_eq!(
        MigrationInfo {
            terraswap_migrated_amount: astro_balance_resp.balance,
            astroport_lp_token: astro_lp_address
        },
        pool_resp_after_migration.migration_info.unwrap()
    );
}

#[test]
fn test_migrate_liquidity_uusd_uluna_pool() {
    let mut app = mock_app();
    let owner = Addr::unchecked("contract_owner");

    let (_, lockdrop_instance, astroport_factory_instance, _, _) =
        instantiate_all_contracts(&mut app, owner.clone());

    // Terraswap LP Token
    let terraswap_token_contract = Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ));
    let terraswap_token_code_id = app.store_code(terraswap_token_contract);

    // Terraswap Pair
    let terraswap_pair_contract = Box::new(
        ContractWrapper::new_with_empty(
            terraswap_pair::contract::execute,
            terraswap_pair::contract::instantiate,
            terraswap_pair::contract::query,
        )
        .with_reply_empty(terraswap_pair::contract::reply),
    );
    let terraswap_pair_code_id = app.store_code(terraswap_pair_contract);

    // LP POOL INSTANCE
    let terraswap_pool_instance = app
        .instantiate_contract(
            terraswap_pair_code_id,
            Addr::unchecked("user".to_string()),
            &terraswap::pair::InstantiateMsg {
                asset_infos: [
                    terraswap::asset::AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    terraswap::asset::AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                ],
                token_code_id: terraswap_token_code_id,
                asset_decimals: [6, 6],
            },
            &[],
            String::from("terraswap_pool"),
            None,
        )
        .unwrap();

    // Query LP Token
    let pair_response: terraswap::asset::PairInfo = app
        .wrap()
        .query_wasm_smart(
            &terraswap_pool_instance,
            &terraswap::pair::QueryMsg::Pair {},
        )
        .unwrap();
    let terraswap_token_instance = pair_response.liquidity_token;

    // SUCCESSFULLY INITIALIZES POOL
    app.execute_contract(
        owner.clone(),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::InitializePool {
            terraswap_lp_token: terraswap_token_instance.to_string(),
            incentives_share: 10000000u64,
        },
        &[],
    )
    .unwrap();

    let user_address = "user".to_string();
    let user2_address = "user2".to_string();

    // Set UST user balances
    app.init_modules(|router, _, storage| {
        router
            .bank
            .init_balance(
                storage,
                &Addr::unchecked(user_address.clone()),
                vec![
                    Coin::new(1000000_000000, "uusd"),
                    Coin::new(1000000_000000, "uluna"),
                ],
            )
            .unwrap();
        router
            .bank
            .init_balance(
                storage,
                &Addr::unchecked(user2_address.clone()),
                vec![
                    Coin::new(1000000_000000, "uusd"),
                    Coin::new(1000000_000000, "uluna"),
                ],
            )
            .unwrap();
    });

    // user#1 adds liquidity to Terraswap Pool and locks that in Lockdrop contract

    // add Liquidity to Terraswap pool
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        terraswap_pool_instance.clone(),
        &terraswap::pair::ExecuteMsg::ProvideLiquidity {
            assets: [
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                    amount: Uint128::from(1000_000000u64),
                },
                terraswap::asset::Asset {
                    info: terraswap::asset::AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    amount: Uint128::from(1000_000000u64),
                },
            ],
            slippage_tolerance: None,
            receiver: None,
        },
        &[
            Coin::new(1000_000000, "uluna"),
            Coin::new(1000_000000, "uusd"),
        ],
    )
    .unwrap();

    // Query LP balance
    let lp_balance_res: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &terraswap_token_instance.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: user_address.clone(),
            },
        )
        .unwrap();
    let user_lp_balance = lp_balance_res.balance;

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 1_000_00)
    });

    // Lock LP Tokens into Lockup Position
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        Addr::unchecked(terraswap_token_instance.clone()),
        &cw20::Cw20ExecuteMsg::Send {
            contract: lockdrop_instance.clone().to_string(),
            amount: user_lp_balance,
            msg: to_binary(&lockdrop::Cw20HookMsg::IncreaseLockup { duration: 10u64 }).unwrap(),
        },
        &[],
    )
    .unwrap();

    // Increase timestamp for window closure
    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 10600001)
    });

    // Create Astroport Pair
    app.execute_contract(
        Addr::unchecked("user"),
        astroport_factory_instance.clone(),
        &astroport::factory::ExecuteMsg::CreatePair {
            asset_infos: [
                astroport::asset::AssetInfo::NativeToken {
                    denom: "uusd".to_string(),
                },
                astroport::asset::AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
            ],
            init_params: None,
            pair_type: astroport::factory::PairType::Xyk {},
        },
        &[],
    )
    .unwrap();

    // Query Astroport addresses
    let pair_resp: astroport::asset::PairInfo = app
        .wrap()
        .query_wasm_smart(
            &astroport_factory_instance,
            &astroport::factory::QueryMsg::Pair {
                asset_infos: [
                    astroport::asset::AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                    astroport::asset::AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                ],
            },
        )
        .unwrap();

    let astro_pool_address = pair_resp.contract_addr;
    let astro_lp_address = pair_resp.liquidity_token;

    // astro LP token balance (Lockdrop)
    let terraswap_balance_resp: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &terraswap_token_instance,
            &Cw20QueryMsg::Balance {
                address: lockdrop_instance.clone().to_string(),
            },
        )
        .unwrap();

    // Query pool before migration
    let pool_resp_before_migration: PoolInfo = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::Pool {
                terraswap_lp_token: terraswap_token_instance.clone().to_string(),
            },
        )
        .unwrap();

    assert_eq!(
        terraswap_balance_resp.balance,
        pool_resp_before_migration.terraswap_amount_in_lockups
    );
    assert_eq!(
        CUint256::from(1750000000u128),
        pool_resp_before_migration.weighted_amount
    );
    assert_eq!(false, pool_resp_before_migration.is_staked);
    assert_eq!(None, pool_resp_before_migration.migration_info);

    // Migrate Liquidity
    app.execute_contract(
        owner.clone(),
        lockdrop_instance.clone(),
        &ExecuteMsg::MigrateLiquidity {
            terraswap_lp_token: terraswap_token_instance.clone(),
            astroport_pool_addr: astro_pool_address.to_string(),
            slippage_tolerance: None,
        },
        &[],
    )
    .unwrap();

    // astro LP token balance (Lockdrop)
    let astro_balance_resp: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &astro_lp_address,
            &Cw20QueryMsg::Balance {
                address: lockdrop_instance.clone().to_string(),
            },
        )
        .unwrap();

    // Query pool after migration
    let pool_resp_after_migration: PoolInfo = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::Pool {
                terraswap_lp_token: terraswap_token_instance.clone().to_string(),
            },
        )
        .unwrap();

    assert_eq!(
        pool_resp_before_migration.terraswap_pool,
        pool_resp_after_migration.terraswap_pool
    );
    assert_eq!(
        pool_resp_before_migration.terraswap_amount_in_lockups,
        pool_resp_after_migration.terraswap_amount_in_lockups
    );
    assert_eq!(
        pool_resp_before_migration.incentives_share,
        pool_resp_after_migration.incentives_share
    );

    assert_eq!(
        pool_resp_before_migration.incentives_share,
        pool_resp_after_migration.incentives_share
    );
    assert_eq!(
        Decimal::zero(),
        pool_resp_after_migration.generator_ntrn_per_share
    );
    assert_eq!(
        RestrictedVector::default(),
        pool_resp_after_migration.generator_proxy_per_share
    );
    assert_eq!(
        MigrationInfo {
            terraswap_migrated_amount: astro_balance_resp.balance,
            astroport_lp_token: astro_lp_address
        },
        pool_resp_after_migration.migration_info.unwrap()
    );
}

#[test]
fn test_stake_lp_tokens() {
    let mut app = mock_app();
    let owner = Addr::unchecked("contract_owner");

    let (_, lockdrop_instance, astroport_factory_instance, _, update_msg) =
        instantiate_all_contracts(&mut app, owner.clone());

    let cw20_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let cw20_code_id = app.store_code(cw20_contract);

    let token_instance = app
        .instantiate_contract(
            cw20_code_id,
            owner.clone(),
            &TokenInstantiateMsg {
                name: String::from("ANC"),
                symbol: String::from("ANC"),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(cw20::MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            String::from("ANC"),
            None,
        )
        .unwrap();

    // Initialize and migrate liquidity for a pool
    let (terraswap_token_instance, astro_lp_address, _) = initialize_and_migrate_liquidity_for_pool(
        &mut app,
        owner.clone(),
        token_instance,
        lockdrop_instance.clone(),
        astroport_factory_instance,
    );

    // Add pool to ASTRO Generator
    app.execute_contract(
        Addr::unchecked(owner.clone()),
        Addr::unchecked(update_msg.clone().generator_address.unwrap()),
        &astroport::generator::ExecuteMsg::SetupPools {
            pools: vec![(astro_lp_address.to_string(), Uint128::from(10u128))],
        },
        &[],
    )
    .unwrap();

    // ######    ERROR :: Unauthorized    ######

    let err = app
        .execute_contract(
            Addr::unchecked("not_owner".to_string()),
            lockdrop_instance.clone(),
            &astroport_periphery::lockdrop::ExecuteMsg::StakeLpTokens {
                terraswap_lp_token: terraswap_token_instance.clone(),
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(err.root_cause().to_string(), "Generic error: Unauthorized");

    // ######    SHOULD SUCCESSFULLY STAKE LP TOKENS WITH GENERATOR   ######

    // astro LP token balance (Lockdrop)
    let lockdrop_astro_balance_before_migration: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &astro_lp_address,
            &Cw20QueryMsg::Balance {
                address: lockdrop_instance.clone().to_string(),
            },
        )
        .unwrap();

    app.execute_contract(
        Addr::unchecked(owner.clone()),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::StakeLpTokens {
            terraswap_lp_token: terraswap_token_instance.clone(),
        },
        &[],
    )
    .unwrap();

    // astro LP token balance (Generator)
    let generator_astro_balance_after_migration: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &astro_lp_address,
            &Cw20QueryMsg::Balance {
                address: update_msg.clone().generator_address.unwrap().to_string(),
            },
        )
        .unwrap();

    assert_eq!(
        lockdrop_astro_balance_before_migration.balance,
        generator_astro_balance_after_migration.balance
    );

    // Query pool after migration
    let pool_resp_after_migration: PoolInfo = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::Pool {
                terraswap_lp_token: terraswap_token_instance.clone().to_string(),
            },
        )
        .unwrap();

    assert_eq!(true, pool_resp_after_migration.is_staked);
}

// TODO: enable when make a deal with pool
// #[test]
fn test_claim_rewards() {
    let mut app = mock_app();
    let owner = Addr::unchecked("contract_owner");

    let (_, lockdrop_instance, astroport_factory_instance, _, update_msg) =
        instantiate_all_contracts(&mut app, owner.clone());

    let cw20_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let cw20_code_id = app.store_code(cw20_contract);

    let token_instance = app
        .instantiate_contract(
            cw20_code_id,
            owner.clone(),
            &TokenInstantiateMsg {
                name: String::from("ANC"),
                symbol: String::from("ANC"),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(cw20::MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            String::from("ANC"),
            None,
        )
        .unwrap();

    // Initialize and migrate liquidity for a pool
    let (terraswap_token_instance, astro_lp_address, _) = initialize_and_migrate_liquidity_for_pool(
        &mut app,
        owner.clone(),
        token_instance,
        lockdrop_instance.clone(),
        astroport_factory_instance,
    );

    // Add pool to ASTRO Generator
    app.execute_contract(
        Addr::unchecked(owner.clone()),
        Addr::unchecked(update_msg.clone().generator_address.unwrap()),
        &astroport::generator::ExecuteMsg::SetupPools {
            pools: vec![(astro_lp_address.to_string(), Uint128::from(10u128))],
        },
        &[],
    )
    .unwrap();

    // Stake LP Tokens with Generator
    app.execute_contract(
        Addr::unchecked(owner.clone()),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::StakeLpTokens {
            terraswap_lp_token: terraswap_token_instance.clone(),
        },
        &[],
    )
    .unwrap();

    let user_address = "user".to_string();
    let user2_address = "user2".to_string();

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);
    assert_eq!(false, user_info.astro_transferred);
    let lockup_response = astroport_periphery::lockdrop::LockUpInfoResponse {
        lp_units_locked: Uint128::from(1000000000u64),
        withdrawal_flag: false,
        astro_rewards: Uint128::from(500000000u64),
        duration: 10u64,
        generator_astro_debt: Uint128::from(0u64),
        claimable_generator_astro_debt: Uint128::from(0u64),
        generator_proxy_debt: RestrictedVector::default(),
        claimable_generator_proxy_debt: RestrictedVector::default(),
        unlock_timestamp: EPOCH_START + 16648000u64,
        astroport_lp_units: Some(Uint128::from(1000000000u64)),
        astroport_lp_token: Some(astro_lp_address.clone()),
        terraswap_lp_token: Addr::unchecked(terraswap_token_instance.clone()),
        astroport_lp_transferred: None,
    };
    assert_eq!(lockup_response, user_info.lockup_infos[0]);

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 10600001)
    });

    // Query state
    let state: StateResponse = app
        .wrap()
        .query_wasm_smart(&lockdrop_instance, &QueryMsg::State {})
        .unwrap();

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);

    // DEPOSIT UST INTO AUCTION
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        Addr::unchecked("auction_contract".to_string()),
        &astroport_periphery::auction::ExecuteMsg::DepositUst {},
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(432423u128),
        }],
    )
    .unwrap();

    // ######    ERROR :: Reward claim not allowed    ######

    let err = app
        .execute_contract(
            Addr::unchecked(user_address.clone()),
            lockdrop_instance.clone(),
            &astroport_periphery::lockdrop::ExecuteMsg::ClaimRewardsAndOptionallyUnlock {
                terraswap_lp_token: terraswap_token_instance.clone(),
                duration: 10u64,
                withdraw_lp_stake: false,
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "Generic error: Reward claim not allowed"
    );

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 10750001)
    });

    // INITIALIZE ASTRO-UST POOL TO ENABLE CLAIMS
    app.execute_contract(
        Addr::unchecked(owner.to_string()),
        Addr::unchecked("auction_contract".to_string()),
        &astroport_periphery::auction::ExecuteMsg::InitPool { slippage: None },
        &[],
    )
    .unwrap();

    // ######    ERROR :: Invalid Lockup    ######

    let err = app
        .execute_contract(
            Addr::unchecked(user_address.clone()),
            lockdrop_instance.clone(),
            &astroport_periphery::lockdrop::ExecuteMsg::ClaimRewardsAndOptionallyUnlock {
                terraswap_lp_token: terraswap_token_instance.clone(),
                duration: 9u64,
                withdraw_lp_stake: false,
            },
            &[],
        )
        .unwrap_err();

    assert_eq!(
        err.root_cause().to_string(),
        "astroport_periphery::lockdrop::LockupInfoV1 not found"
    );

    // ######    SHOULD SUCCESSFULLY CLAIM REWARDS   ######

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);
    assert_eq!(false, user_info.astro_transferred);
    let lockup_response = astroport_periphery::lockdrop::LockUpInfoResponse {
        lp_units_locked: Uint128::from(1000000000u64),
        withdrawal_flag: false,
        astro_rewards: Uint128::from(500000000u64),
        duration: 10u64,
        generator_astro_debt: Uint128::from(0u64),
        claimable_generator_astro_debt: Uint128::from(172800000000u64),
        generator_proxy_debt: RestrictedVector::default(),
        claimable_generator_proxy_debt: RestrictedVector::default(),
        unlock_timestamp: EPOCH_START + 16648000u64,
        astroport_lp_units: Some(Uint128::from(1000000000u64)),
        astroport_lp_token: Some(astro_lp_address.clone()),
        terraswap_lp_token: Addr::unchecked(terraswap_token_instance.clone()),
        astroport_lp_transferred: None,
    };
    assert_eq!(lockup_response, user_info.lockup_infos[0]);

    let user1_astro_balance_before: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &update_msg.astro_token_address.clone().unwrap(),
            &Cw20QueryMsg::Balance {
                address: user_address.clone(),
            },
        )
        .unwrap();

    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::ClaimRewardsAndOptionallyUnlock {
            terraswap_lp_token: terraswap_token_instance.clone(),
            duration: 10u64,
            withdraw_lp_stake: false,
        },
        &[],
    )
    .unwrap();

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);
    assert_eq!(true, user_info.astro_transferred);
    let lockup_response = astroport_periphery::lockdrop::LockUpInfoResponse {
        lp_units_locked: Uint128::from(1000000000u64),
        withdrawal_flag: false,
        astro_rewards: Uint128::from(500000000u64),
        duration: 10u64,
        generator_astro_debt: Uint128::from(172800000000u64),
        claimable_generator_astro_debt: Uint128::from(0u64),
        generator_proxy_debt: RestrictedVector::default(),
        claimable_generator_proxy_debt: RestrictedVector::default(),
        unlock_timestamp: EPOCH_START + 16648000u64,
        astroport_lp_units: Some(Uint128::from(1000000000u64)),
        astroport_lp_token: Some(astro_lp_address.clone()),
        terraswap_lp_token: Addr::unchecked(terraswap_token_instance.clone()),
        astroport_lp_transferred: None,
    };
    assert_eq!(lockup_response, user_info.lockup_infos[0]);

    let user1_astro_balance_after: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &update_msg.astro_token_address.clone().unwrap(),
            &Cw20QueryMsg::Balance {
                address: user_address.clone(),
            },
        )
        .unwrap();

    let astro_reward_claimed =
        user1_astro_balance_after.balance - user1_astro_balance_before.balance;

    // ######    SHOULD SUCCESSFULLY CLAIM REWARDS :: user-2  ######

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user2_address.clone(),
            },
        )
        .unwrap();

    let user2_astro_balance_before: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &update_msg.astro_token_address.clone().unwrap(),
            &Cw20QueryMsg::Balance {
                address: user2_address.clone(),
            },
        )
        .unwrap();

    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);
    assert_eq!(false, user_info.astro_transferred);
    let lockup_response = astroport_periphery::lockdrop::LockUpInfoResponse {
        lp_units_locked: Uint128::from(1000000000u64),
        withdrawal_flag: false,
        astro_rewards: Uint128::from(500000000u64),
        duration: 10u64,
        generator_astro_debt: Uint128::from(0u64),
        claimable_generator_astro_debt: Uint128::from(172800000000u64),
        generator_proxy_debt: RestrictedVector::default(),
        claimable_generator_proxy_debt: RestrictedVector::default(),
        unlock_timestamp: EPOCH_START + 16648000u64,
        astroport_lp_units: Some(Uint128::from(1000000000u64)),
        astroport_lp_token: Some(astro_lp_address.clone()),
        terraswap_lp_token: Addr::unchecked(terraswap_token_instance.clone()),
        astroport_lp_transferred: None,
    };
    assert_eq!(lockup_response, user_info.lockup_infos[0]);

    app.execute_contract(
        Addr::unchecked(user2_address.clone()),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::ClaimRewardsAndOptionallyUnlock {
            terraswap_lp_token: terraswap_token_instance.clone(),
            duration: 10u64,
            withdraw_lp_stake: false,
        },
        &[],
    )
    .unwrap();

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user2_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);
    assert_eq!(true, user_info.astro_transferred);
    let lockup_response = astroport_periphery::lockdrop::LockUpInfoResponse {
        lp_units_locked: Uint128::from(1000000000u64),
        withdrawal_flag: false,
        astro_rewards: Uint128::from(500000000u64),
        duration: 10u64,
        generator_astro_debt: Uint128::from(172800000000u64),
        claimable_generator_astro_debt: Uint128::from(0u64),
        generator_proxy_debt: RestrictedVector::default(),
        claimable_generator_proxy_debt: RestrictedVector::default(),
        unlock_timestamp: EPOCH_START + 16648000u64,
        astroport_lp_units: Some(Uint128::from(1000000000u64)),
        astroport_lp_token: Some(astro_lp_address.clone()),
        terraswap_lp_token: Addr::unchecked(terraswap_token_instance.clone()),
        astroport_lp_transferred: None,
    };
    assert_eq!(lockup_response, user_info.lockup_infos[0]);

    let user2_astro_balance_after: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &update_msg.astro_token_address.clone().unwrap(),
            &Cw20QueryMsg::Balance {
                address: user2_address.clone(),
            },
        )
        .unwrap();

    let astro_reward_claimed =
        user2_astro_balance_after.balance - user2_astro_balance_before.balance;
}

// TODO: enable when make a deal with pool
// #[test]
fn test_claim_rewards_and_unlock() {
    let mut app = mock_app();
    let owner = Addr::unchecked("contract_owner");

    let (_, lockdrop_instance, astroport_factory_instance, _, update_msg) =
        instantiate_all_contracts(&mut app, owner.clone());

    let cw20_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let cw20_code_id = app.store_code(cw20_contract);

    let token_instance = app
        .instantiate_contract(
            cw20_code_id,
            owner.clone(),
            &TokenInstantiateMsg {
                name: String::from("ANC"),
                symbol: String::from("ANC"),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(cw20::MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            String::from("ANC"),
            None,
        )
        .unwrap();

    // Initialize and migrate liquidity for a pool
    let (terraswap_token_instance, astro_lp_address, _) = initialize_and_migrate_liquidity_for_pool(
        &mut app,
        owner.clone(),
        token_instance,
        lockdrop_instance.clone(),
        astroport_factory_instance,
    );

    // Add pool to ASTRO Generator
    app.execute_contract(
        Addr::unchecked(owner.clone()),
        Addr::unchecked(update_msg.clone().generator_address.unwrap()),
        &astroport::generator::ExecuteMsg::SetupPools {
            pools: vec![(astro_lp_address.to_string(), Uint128::from(10u128))],
        },
        &[],
    )
    .unwrap();

    // Stake LP Tokens with Generator
    app.execute_contract(
        Addr::unchecked(owner.clone()),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::StakeLpTokens {
            terraswap_lp_token: terraswap_token_instance.clone(),
        },
        &[],
    )
    .unwrap();

    let user_address = "user".to_string();
    let user2_address = "user2".to_string();

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);
    assert_eq!(false, user_info.astro_transferred);
    let lockup_response = astroport_periphery::lockdrop::LockUpInfoResponse {
        lp_units_locked: Uint128::from(1000000000u64),
        withdrawal_flag: false,
        astro_rewards: Uint128::from(500000000u64),
        duration: 10u64,
        generator_astro_debt: Uint128::from(0u64),
        claimable_generator_astro_debt: Uint128::from(0u64),
        generator_proxy_debt: RestrictedVector::default(),
        claimable_generator_proxy_debt: RestrictedVector::default(),
        unlock_timestamp: EPOCH_START + 16648000u64,
        astroport_lp_units: Some(Uint128::from(1000000000u64)),
        astroport_lp_token: Some(astro_lp_address.clone()),
        terraswap_lp_token: Addr::unchecked(terraswap_token_instance.clone()),
        astroport_lp_transferred: None,
    };
    assert_eq!(lockup_response, user_info.lockup_infos[0]);

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 10600001)
    });

    // Query state
    let state: StateResponse = app
        .wrap()
        .query_wasm_smart(&lockdrop_instance, &QueryMsg::State {})
        .unwrap();

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);

    // DEPOSIT UST INTO AUCTION
    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        Addr::unchecked("auction_contract".to_string()),
        &astroport_periphery::auction::ExecuteMsg::DepositUst {},
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128::from(432423u128),
        }],
    )
    .unwrap();

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 10750001)
    });

    // INITIALIZE ASTRO-UST POOL TO ENABLE CLAIMS
    app.execute_contract(
        Addr::unchecked(owner.to_string()),
        Addr::unchecked("auction_contract".to_string()),
        &astroport_periphery::auction::ExecuteMsg::InitPool { slippage: None },
        &[],
    )
    .unwrap();

    // ######    SHOULD SUCCESSFULLY CLAIM REWARDS AND UNLOCK   ######

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 16648001)
    });

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);

    assert_eq!(false, user_info.astro_transferred);
    let lockup_response = astroport_periphery::lockdrop::LockUpInfoResponse {
        lp_units_locked: Uint128::from(1000000000u64),
        withdrawal_flag: false,
        astro_rewards: Uint128::from(500000000u64),
        duration: 10u64,
        generator_astro_debt: Uint128::from(0u64),
        claimable_generator_astro_debt: Uint128::from(259200000000u64),
        generator_proxy_debt: RestrictedVector::default(),
        claimable_generator_proxy_debt: RestrictedVector::default(),
        unlock_timestamp: EPOCH_START + 16648000u64,
        astroport_lp_units: Some(Uint128::from(1000000000u64)),
        astroport_lp_token: Some(astro_lp_address.clone()),
        terraswap_lp_token: Addr::unchecked(terraswap_token_instance.clone()),
        astroport_lp_transferred: None,
    };
    assert_eq!(lockup_response, user_info.lockup_infos[0]);

    let user1_astro_balance_before: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &update_msg.astro_token_address.clone().unwrap(),
            &Cw20QueryMsg::Balance {
                address: user_address.clone(),
            },
        )
        .unwrap();

    let user1_astro_lp_balance_before: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &astro_lp_address.clone(),
            &Cw20QueryMsg::Balance {
                address: user_address.clone(),
            },
        )
        .unwrap();

    app.execute_contract(
        Addr::unchecked(user_address.clone()),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::ClaimRewardsAndOptionallyUnlock {
            terraswap_lp_token: terraswap_token_instance.clone(),
            duration: 10u64,
            withdraw_lp_stake: true,
        },
        &[],
    )
    .unwrap();

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);

    assert_eq!(true, user_info.astro_transferred);

    let user1_astro_balance_after: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &update_msg.astro_token_address.clone().unwrap(),
            &Cw20QueryMsg::Balance {
                address: user_address.clone(),
            },
        )
        .unwrap();

    let user1_astro_lp_balance_after: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &astro_lp_address.clone(),
            &Cw20QueryMsg::Balance {
                address: user_address.clone(),
            },
        )
        .unwrap();

    let astro_reward_claimed =
        user1_astro_balance_after.balance - user1_astro_balance_before.balance;
    let lp_tokens_withdrawn =
        user1_astro_lp_balance_after.balance - user1_astro_lp_balance_before.balance;

    assert_eq!(lp_tokens_withdrawn, Uint128::from(1000000000u64));

    // ######    SHOULD SUCCESSFULLY CLAIM REWARDS :: user-2  ######

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user2_address.clone(),
            },
        )
        .unwrap();

    let user2_astro_balance_before: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &update_msg.astro_token_address.clone().unwrap(),
            &Cw20QueryMsg::Balance {
                address: user2_address.clone(),
            },
        )
        .unwrap();

    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);
    assert_eq!(false, user_info.astro_transferred);
    let lockup_response = astroport_periphery::lockdrop::LockUpInfoResponse {
        lp_units_locked: Uint128::from(1000000000u64),
        withdrawal_flag: false,
        astro_rewards: Uint128::from(500000000u64),
        duration: 10u64,
        generator_astro_debt: Uint128::from(0u64),
        claimable_generator_astro_debt: Uint128::from(259200000000u64),
        generator_proxy_debt: RestrictedVector::default(),
        claimable_generator_proxy_debt: RestrictedVector::default(),
        unlock_timestamp: EPOCH_START + 16648000u64,
        astroport_lp_units: Some(Uint128::from(1000000000u64)),
        astroport_lp_token: Some(astro_lp_address.clone()),
        terraswap_lp_token: Addr::unchecked(terraswap_token_instance.clone()),
        astroport_lp_transferred: None,
    };
    assert_eq!(lockup_response, user_info.lockup_infos[0]);

    let user2_astro_lp_balance_before: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &astro_lp_address.clone(),
            &Cw20QueryMsg::Balance {
                address: user2_address.clone(),
            },
        )
        .unwrap();

    app.execute_contract(
        Addr::unchecked(user2_address.clone()),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::ClaimRewardsAndOptionallyUnlock {
            terraswap_lp_token: terraswap_token_instance.clone(),
            duration: 10u64,
            withdraw_lp_stake: true,
        },
        &[],
    )
    .unwrap();

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user2_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);
    assert_eq!(true, user_info.astro_transferred);

    let user2_astro_balance_after: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &update_msg.astro_token_address.clone().unwrap(),
            &Cw20QueryMsg::Balance {
                address: user2_address.clone(),
            },
        )
        .unwrap();

    let user2_astro_lp_balance_after: cw20::BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &astro_lp_address.clone(),
            &Cw20QueryMsg::Balance {
                address: user2_address.clone(),
            },
        )
        .unwrap();

    let astro_reward_claimed =
        user2_astro_balance_after.balance - user2_astro_balance_before.balance;

    let lp_tokens_withdrawn =
        user2_astro_lp_balance_after.balance - user2_astro_lp_balance_before.balance;
    assert_eq!(lp_tokens_withdrawn, Uint128::from(1000000000u64));
}

#[test]
fn test_delegate_astro_to_auction() {
    let mut app = mock_app();
    let owner = Addr::unchecked("contract_owner");

    let (_, lockdrop_instance, astroport_factory_instance, _, update_msg) =
        instantiate_all_contracts(&mut app, owner.clone());

    let cw20_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    let cw20_code_id = app.store_code(cw20_contract);

    let token_instance = app
        .instantiate_contract(
            cw20_code_id,
            owner.clone(),
            &TokenInstantiateMsg {
                name: String::from("ANC"),
                symbol: String::from("ANC"),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(cw20::MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            String::from("ANC"),
            None,
        )
        .unwrap();

    // Initialize and migrate liquidity for a pool
    let (terraswap_token_instance, astro_lp_address, _) = initialize_and_migrate_liquidity_for_pool(
        &mut app,
        owner.clone(),
        token_instance,
        lockdrop_instance.clone(),
        astroport_factory_instance,
    );

    // Add pool to ASTRO Generator
    app.execute_contract(
        Addr::unchecked(owner.clone()),
        Addr::unchecked(update_msg.clone().generator_address.unwrap()),
        &astroport::generator::ExecuteMsg::SetupPools {
            pools: vec![(astro_lp_address.to_string(), Uint128::from(10u128))],
        },
        &[],
    )
    .unwrap();

    // Stake LP Tokens with Generator
    app.execute_contract(
        Addr::unchecked(owner.clone()),
        lockdrop_instance.clone(),
        &astroport_periphery::lockdrop::ExecuteMsg::StakeLpTokens {
            terraswap_lp_token: terraswap_token_instance.clone(),
        },
        &[],
    )
    .unwrap();

    let user_address = "user".to_string();

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);
    assert_eq!(false, user_info.astro_transferred);
    let lockup_response = astroport_periphery::lockdrop::LockUpInfoResponse {
        lp_units_locked: Uint128::from(1000000000u64),
        withdrawal_flag: false,
        astro_rewards: Uint128::from(500000000u64),
        duration: 10u64,
        generator_astro_debt: Uint128::from(0u64),
        claimable_generator_astro_debt: Uint128::from(0u64),
        generator_proxy_debt: RestrictedVector::default(),
        claimable_generator_proxy_debt: RestrictedVector::default(),
        unlock_timestamp: EPOCH_START + 16648000u64,
        astroport_lp_units: Some(Uint128::from(1000000000u64)),
        astroport_lp_token: Some(astro_lp_address.clone()),
        terraswap_lp_token: Addr::unchecked(terraswap_token_instance.clone()),
        astroport_lp_transferred: None,
    };
    assert_eq!(lockup_response, user_info.lockup_infos[0]);

    app.update_block(|b| {
        b.height += 17280;
        b.time = Timestamp::from_seconds(EPOCH_START + 10600001)
    });

    // Query state
    let state: StateResponse = app
        .wrap()
        .query_wasm_smart(&lockdrop_instance, &QueryMsg::State {})
        .unwrap();

    // Query user
    let user_info: UserInfoResponse = app
        .wrap()
        .query_wasm_smart(
            &lockdrop_instance,
            &QueryMsg::UserInfo {
                address: user_address.clone(),
            },
        )
        .unwrap();
    assert_eq!(Uint128::from(500000000u64), user_info.total_astro_rewards);
}
