// Copyright 2022 Neutron Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::contract::{
    execute_add_vesting, execute_burn_from, execute_mint, execute_transfer, instantiate,
    DEPOSITED_SYMBOL,
};
use crate::error::ContractError;
use crate::msg::InstantiateMsg;
use crate::state::ALLOCATIONS;
use cosmwasm_std::testing::{
    mock_dependencies, mock_env, mock_info, MockApi, MockQuerier, MockStorage,
};
use cosmwasm_std::{
    coins, Addr, BankMsg, Coin, DepsMut, Empty, Env, MessageInfo, OwnedDeps, Response, Timestamp,
    Uint128,
};
use cw20_base::state::{BALANCES, TOKEN_INFO};

// instantiates the contracts, mints the money, transfers `amount` to `somebody` address
fn _instantiate_vest_to_somebody(
    total_to_mint: u128,
    amount: u128,
    vesting_start_time: Option<u64>,
    vesting_duration: u64,
) -> (OwnedDeps<MockStorage, MockApi, MockQuerier, Empty>, Env) {
    // instantiate
    let mut deps = mock_dependencies();
    let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

    // mint
    let dao_info = mock_info("dao_address", &coins(total_to_mint, DEPOSITED_SYMBOL));
    let res = execute_mint(deps.as_mut(), env.clone(), dao_info);
    assert!(res.is_ok());

    // transfer to `somebody` with vesting
    let airdrop_info = mock_info("airdrop_address", &[]);
    let res = execute_transfer(
        deps.as_mut(),
        env.clone(),
        airdrop_info,
        "somebody".to_string(),
        Uint128::new(amount),
    );
    assert!(res.is_ok());

    // vest
    _do_add_vesting(
        deps.as_mut(),
        env.clone(),
        "somebody".to_string(),
        Uint128::new(amount),
        vesting_start_time.unwrap_or_else(|| env.block.time.seconds()),
        vesting_duration,
    );

    (deps, env)
}

fn _do_simple_instantiate(deps: DepsMut, funds: Option<Vec<Coin>>) -> (MessageInfo, Env) {
    _do_instantiate(
        deps,
        "airdrop_address".to_string(),
        "lockdrop_address".to_string(),
        funds,
        Timestamp::from_seconds(0),
    )
}

fn _do_instantiate(
    mut deps: DepsMut,
    airdrop_address: String,
    lockdrop_address: String,
    funds: Option<Vec<Coin>>,
    when_withdrawable: Timestamp,
) -> (MessageInfo, Env) {
    let instantiate_msg = InstantiateMsg {
        airdrop_address,
        lockdrop_address,
        when_withdrawable,
    };
    let info = mock_info("dao_address", &funds.unwrap_or_default());
    let env = mock_env();
    let res = instantiate(deps.branch(), env.clone(), info.clone(), instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());

    (info, env)
}

fn _do_add_vesting(
    deps: DepsMut,
    env: Env,
    address: String,
    amount: Uint128,
    start_time: u64,
    duration: u64,
) {
    let airdrop_info = mock_info("airdrop_address", &[]);

    let res = execute_add_vesting(
        deps,
        env,
        airdrop_info,
        address,
        amount,
        start_time,
        duration,
    );
    assert!(res.is_ok());
}

fn _withdraw_rewards(deps: DepsMut, env: Env, amount: u128) {
    let res = execute_burn_from(
        deps,
        env,
        mock_info("lockdrop_address", &[]),
        "somebody".to_string(),
        Uint128::new(amount),
    );
    assert!(res.is_ok());
}

fn _assert_withdrawn(
    deps: DepsMut,
    res: Result<Response, ContractError>,
    previous_total_supply: u128,
    allocated_amount: u128,
    withdrawn_total_amount: u128,
    withdrawn_now_amount: u128,
    rewarded_amount_without_vesting: u128,
) {
    assert!(res.is_ok());
    let msgs = res.unwrap().messages;
    assert_eq!(msgs.len(), 1);
    assert_eq!(
        msgs.first().unwrap().msg,
        BankMsg::Send {
            to_address: "somebody".to_string(),
            amount: coins(withdrawn_now_amount, DEPOSITED_SYMBOL)
        }
        .into()
    );
    let allocation = ALLOCATIONS
        .load(deps.storage, &Addr::unchecked("somebody"))
        .unwrap();
    assert_eq!(allocation.allocated_amount, Uint128::new(allocated_amount));
    assert_eq!(
        allocation.withdrawn_amount,
        Uint128::new(withdrawn_total_amount)
    );

    let balance = BALANCES
        .load(deps.storage, &Addr::unchecked("somebody"))
        .unwrap();
    assert_eq!(
        balance,
        Uint128::new(allocated_amount - withdrawn_total_amount - rewarded_amount_without_vesting)
    );

    let token_info = TOKEN_INFO.load(deps.storage).unwrap();
    assert_eq!(
        token_info.total_supply,
        Uint128::new(previous_total_supply - withdrawn_now_amount)
    );
}

mod instantiate {
    use super::*;
    use crate::contract::{query_config, TOKEN_DECIMALS, TOKEN_NAME, TOKEN_SYMBOL};
    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::Uint128;
    use cw20_base::contract::{query_minter, query_token_info};
    use cw20_base::enumerable::query_all_accounts;

    #[test]
    fn basic() {
        let mut deps = mock_dependencies();
        let (_info, _env) = _do_instantiate(
            deps.as_mut(),
            "airdrop_address".to_string(),
            "lockdrop_address".to_string(),
            None,
            Timestamp::from_seconds(0),
        );
        let config = query_config(deps.as_ref()).unwrap();
        assert_eq!(config.dao_address, "dao_address".to_string());
        assert_eq!(config.lockdrop_address, "lockdrop_address".to_string());
        assert_eq!(config.airdrop_address, "airdrop_address".to_string());

        // no accounts since we don't mint anything
        assert_eq!(
            query_all_accounts(deps.as_ref(), None, None)
                .unwrap()
                .accounts
                .len(),
            0
        );
        // minter is dao account
        assert_eq!(
            query_minter(deps.as_ref()).unwrap().unwrap().minter,
            "dao_address".to_string()
        );

        // Write TOKEN_INFO
        let token_info = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(token_info.decimals, TOKEN_DECIMALS);
        assert_eq!(token_info.name, TOKEN_NAME);
        assert_eq!(token_info.symbol, TOKEN_SYMBOL);
        assert_eq!(token_info.total_supply, Uint128::zero());
    }
}

mod add_vesting {
    use crate::contract::{execute_add_vesting, VESTING_CLIFF};
    use crate::error::ContractError;
    use crate::error::ContractError::Unauthorized;
    use crate::state::{Schedule, ALLOCATIONS};
    use crate::testing::tests::_do_simple_instantiate;
    use cosmwasm_std::testing::{mock_dependencies, mock_info};
    use cosmwasm_std::{Addr, Uint128};

    #[test]
    fn adds_vesting_for_account_with_correct_settings() {
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);
        let airdrop_info = mock_info("airdrop_address", &[]);

        let res = execute_add_vesting(
            deps.as_mut(),
            env,
            airdrop_info,
            "address".to_string(),
            Uint128::new(100),
            15,
            1000,
        );
        assert!(res.is_ok());

        let allocation = ALLOCATIONS
            .load(&deps.storage, &Addr::unchecked("address"))
            .unwrap();
        assert_eq!(allocation.allocated_amount, Uint128::new(100));
        assert_eq!(allocation.withdrawn_amount, Uint128::new(0));
        assert_eq!(
            allocation.schedule,
            Schedule {
                start_time: 15,
                cliff: VESTING_CLIFF,
                duration: 1000
            }
        );
    }

    #[test]
    fn non_airdrop_addresses_cannot_set_vesting() {
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);
        let non_airdrop_info = mock_info("non_airdrop_address", &[]);

        let res = execute_add_vesting(
            deps.as_mut(),
            env,
            non_airdrop_info,
            "address".to_string(),
            Uint128::new(100),
            15,
            1000,
        );
        assert_eq!(res, Err(Unauthorized));
    }

    #[test]
    fn cannot_add_vesting_twice_to_same_address() {
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);
        let airdrop_info = mock_info("airdrop_address", &[]);

        let res = execute_add_vesting(
            deps.as_mut(),
            env.clone(),
            airdrop_info.clone(),
            "address".to_string(),
            Uint128::new(100),
            15,
            1000,
        );
        assert!(res.is_ok());

        let res = execute_add_vesting(
            deps.as_mut(),
            env,
            airdrop_info,
            "address".to_string(),
            Uint128::new(100),
            15,
            1000,
        );
        assert_eq!(
            res,
            Err(ContractError::AlreadyVested {
                address: "address".to_string()
            })
        );
    }
}

mod transfer {
    use crate::contract::{execute_mint, execute_transfer, DEPOSITED_SYMBOL};
    use crate::error::ContractError::{Cw20Error, Unauthorized};
    use crate::testing::tests::_do_simple_instantiate;
    use cosmwasm_std::testing::{mock_dependencies, mock_info};
    use cosmwasm_std::OverflowOperation::Sub;
    use cosmwasm_std::{coins, Addr, OverflowError, StdError, Uint128};
    use cw20_base::state::BALANCES;
    use cw20_base::ContractError::Std;

    #[test]
    fn works_from_airdrop_and_lockdrop() {
        // instantiate
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

        // mint
        let dao_info = mock_info("dao_address", &coins(1_000_000_000, DEPOSITED_SYMBOL));
        let res = execute_mint(deps.as_mut(), env.clone(), dao_info);
        assert!(res.is_ok());

        let airdrop_info = mock_info("airdrop_address", &[]);
        let res = execute_transfer(
            deps.as_mut(),
            env,
            airdrop_info,
            "somebody".to_string(),
            Uint128::new(1_000),
        );
        assert!(res.is_ok());
        let balance = BALANCES
            .load(&deps.storage, &Addr::unchecked("somebody"))
            .unwrap();
        assert_eq!(balance, Uint128::new(1_000));

        let airdrop_balance = BALANCES
            .load(&deps.storage, &Addr::unchecked("airdrop_address"))
            .unwrap();
        assert_eq!(airdrop_balance, Uint128::new(1_000_000_000 - 1_000));
        // TODO: add test that lockdrop address has access to transfer (if we need this permission really)
    }

    #[test]
    fn fails_when_try_non_existent_funds() {
        // instantiate
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

        let airdrop_info = mock_info("airdrop_address", &[]);
        let res = execute_transfer(
            deps.as_mut(),
            env,
            airdrop_info,
            "somebody".to_string(),
            Uint128::new(1_000),
        );
        assert_eq!(
            res,
            Err(Cw20Error(Std(StdError::overflow(OverflowError {
                operation: Sub,
                operand1: "0".to_string(),
                operand2: "1000".to_string()
            }))))
        );
    }

    #[test]
    fn not_authorized_from_others() {
        // instantiate
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

        let airdrop_info = mock_info("somebody", &[]);
        let res = execute_transfer(
            deps.as_mut(),
            env,
            airdrop_info,
            "somebody".to_string(),
            Uint128::new(1_000),
        );
        assert_eq!(res, Err(Unauthorized));
    }
}

mod withdraw {
    use crate::contract::execute_withdraw;
    use crate::error::ContractError::{NoFundsToClaim, Std, TooEarlyToClaim};
    use crate::testing::tests::{
        _assert_withdrawn, _do_instantiate, _do_simple_instantiate, _instantiate_vest_to_somebody,
        _withdraw_rewards,
    };
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::StdError;

    // withdrawing rewards (burn_from) should not fail withdrawing all funds later
    #[test]
    fn full() {
        // instantiate
        let (mut deps, mut env) = _instantiate_vest_to_somebody(10_000_000, 100, None, 1000);

        // at this point `somebody` has vested 100 NTRNs on 1000 seconds

        // pass 3/4 vesting duration (750 seconds)
        env.block.time = env.block.time.plus_seconds(750);

        // withdraw 75 (100 * 75%)
        let res = execute_withdraw(deps.as_mut(), env.clone(), mock_info("somebody", &[]));
        _assert_withdrawn(deps.as_mut(), res, 10_000_000, 100, 75, 75, 0);

        // now let's check that if we distribute rewards and skip all vesting, we still successfully withdraw what's left

        // pass all vesting duration (> 1000 seconds)
        env.block.time = env.block.time.plus_seconds(1250);

        // withdraw rewards for that account
        _withdraw_rewards(deps.as_mut(), env.clone(), 10);

        // after sending 10 to account, we only have 25-10=15 left

        // withdraw what's left
        let res = execute_withdraw(deps.as_mut(), env, mock_info("somebody", &[]));
        _assert_withdrawn(deps.as_mut(), res, 10_000_000 - 75 - 10, 100, 90, 15, 10);
    }

    // withdrawing rewards (burn_from) should not change amount of vested tokens
    #[test]
    fn vesting_schedule_is_immutable() {
        // instantiate
        let (mut deps, mut env) = _instantiate_vest_to_somebody(10_000_000, 100, None, 1000);

        // at this point `somebody` has vested 100 NTRNs on 1000 seconds

        // pass 50% time
        env.block.time = env.block.time.plus_seconds(500);

        // withdraw rewards for that account
        _withdraw_rewards(deps.as_mut(), env.clone(), 25);

        // because vesting schedule is immutable we still have 50 to withdraw,
        // even though we withdrew 25 as rewards
        let res = execute_withdraw(deps.as_mut(), env, mock_info("somebody", &[]));
        _assert_withdrawn(deps.as_mut(), res, 10_000_000 - 25, 100, 50, 50, 25);
    }

    #[test]
    fn does_not_withdraw_if_vesting_empty() {
        // instantiate
        let (mut deps, env) = _instantiate_vest_to_somebody(10_000_000, 100, None, 1000);

        // at this point `somebody` has vested 100 NTRNs on 1000 seconds

        // call at 0% progress of vesting returns error
        let res = execute_withdraw(deps.as_mut(), env, mock_info("somebody", &[]));
        assert_eq!(res, Err(NoFundsToClaim));
    }

    #[test]
    fn does_not_withdraw_if_vesting_does_not_started() {
        // instantiate
        let (mut deps, env) = _instantiate_vest_to_somebody(
            10_000_000,
            100,
            Some(mock_env().block.time.plus_seconds(100).seconds()),
            1000,
        );

        // at this point `somebody` has vested 100 NTRNs on 1000 seconds

        // call at 0% progress of vesting returns error
        let res = execute_withdraw(deps.as_mut(), env, mock_info("somebody", &[]));
        assert_eq!(res, Err(NoFundsToClaim));
    }

    #[test]
    fn does_not_withdraw_until_ready() {
        // instantiate
        let mut deps = mock_dependencies();
        let (_info, env) = _do_instantiate(
            deps.as_mut(),
            "airdrop_address".to_string(),
            "lockdrop_address".to_string(),
            None,
            mock_env().block.time.plus_seconds(1_000_000),
        );
        let somebody_info = mock_info("somebody", &[]);
        let res = execute_withdraw(deps.as_mut(), env, somebody_info);
        assert_eq!(res, Err(TooEarlyToClaim));
    }

    #[test]
    fn does_not_withdraw_if_no_tokens_vested_yet() {
        // instantiate
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

        // check
        let somebody_info = mock_info("somebody", &[]);
        let res = execute_withdraw(deps.as_mut(), env, somebody_info);
        assert_eq!(
            res,
            Err(Std(StdError::not_found("credits::state::Allocation")))
        );
    }
}

mod burn {
    use crate::contract::{execute_burn, execute_mint, DEPOSITED_SYMBOL};
    use crate::error::ContractError::Unauthorized;
    use crate::testing::tests::_do_simple_instantiate;
    use cosmwasm_std::testing::{mock_dependencies, mock_info};
    use cosmwasm_std::{coins, Addr, BankMsg, Uint128};
    use cw20_base::state::{BALANCES, TOKEN_INFO};

    #[test]
    fn works_with_correct_params_for_airdrop() {
        // instantiate
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

        // mint
        let minted_balance = 1_000_000_000;
        let dao_info = mock_info("dao_address", &coins(minted_balance, DEPOSITED_SYMBOL));
        let res = execute_mint(deps.as_mut(), env.clone(), dao_info);
        assert!(res.is_ok());

        // burn amount
        let airdrop_info = mock_info("airdrop_address", &[]);
        let res = execute_burn(deps.as_mut(), env, airdrop_info, Uint128::new(10000));
        assert!(res.is_ok());

        let msgs = res.unwrap().messages;
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs.first().unwrap().msg,
            BankMsg::Send {
                to_address: "airdrop_address".to_string(),
                amount: coins(10_000, DEPOSITED_SYMBOL)
            }
            .into()
        );

        let balance = BALANCES
            .load(&deps.storage, &Addr::unchecked("airdrop_address"))
            .unwrap();
        assert_eq!(balance, Uint128::new(minted_balance - 10_000));

        let token_info = TOKEN_INFO.load(&deps.storage).unwrap();
        assert_eq!(
            token_info.total_supply,
            Uint128::new(minted_balance - 10_000)
        );
    }

    #[test]
    fn unauthorized_for_non_airdrop_addresses() {
        // instantiate
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

        // burn amount
        let airdrop_info = mock_info("non_airdrop_address", &[]);
        let res = execute_burn(deps.as_mut(), env, airdrop_info, Uint128::new(10000));
        assert_eq!(res, Err(Unauthorized))
    }
}

mod burn_from {
    use crate::contract::{execute_burn_from, execute_mint, execute_transfer, DEPOSITED_SYMBOL};
    use crate::error::ContractError::Cw20Error;
    use crate::testing::tests::_do_simple_instantiate;
    use cosmwasm_std::testing::{mock_dependencies, mock_info};
    use cosmwasm_std::OverflowOperation::Sub;
    use cosmwasm_std::{coins, Addr, BankMsg, OverflowError, StdError, Uint128};
    use cw20_base::state::{BALANCES, TOKEN_INFO};
    use cw20_base::ContractError::Std;

    #[test]
    fn works_properly_with_airdrop_account() {
        // instantiate
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

        // mint
        let minted_balance = 1_000_000_000;
        let dao_info = mock_info("dao_address", &coins(minted_balance, DEPOSITED_SYMBOL));
        let res = execute_mint(deps.as_mut(), env.clone(), dao_info);
        assert!(res.is_ok());

        // burn_from
        let res = execute_burn_from(
            deps.as_mut(),
            env,
            mock_info("lockdrop_address", &[]),
            "airdrop_address".to_string(),
            Uint128::new(20_000),
        );
        assert!(res.is_ok());

        let msgs = res.unwrap().messages;
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            msgs.first().unwrap().msg,
            BankMsg::Send {
                to_address: "airdrop_address".to_string(),
                amount: coins(20_000, DEPOSITED_SYMBOL)
            }
            .into()
        );

        let balance = BALANCES
            .load(&deps.storage, &Addr::unchecked("airdrop_address"))
            .unwrap();
        assert_eq!(balance, Uint128::new(minted_balance - 20_000));

        let token_info = TOKEN_INFO.load(&deps.storage).unwrap();
        assert_eq!(
            token_info.total_supply,
            Uint128::new(minted_balance - 20_000)
        );
    }

    #[test]
    fn returns_error_if_not_enough() {
        // instantiate
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

        // mint
        let minted_balance = 1_000_000_000;
        let dao_info = mock_info("dao_address", &coins(minted_balance, DEPOSITED_SYMBOL));
        let res = execute_mint(deps.as_mut(), env.clone(), dao_info);
        assert!(res.is_ok());

        // transfer to somebody
        let res = execute_transfer(
            deps.as_mut(),
            env.clone(),
            mock_info("airdrop_address", &[]),
            "somebody".to_string(),
            Uint128::new(100),
        );
        assert!(res.is_ok());

        // burn_from somebody
        let res = execute_burn_from(
            deps.as_mut(),
            env,
            mock_info("lockdrop_address", &[]),
            "somebody".to_string(),
            Uint128::new(20_000),
        );
        assert_eq!(
            res,
            Err(Cw20Error(Std(StdError::overflow(OverflowError {
                operation: Sub,
                operand1: "100".to_string(),
                operand2: "20000".to_string()
            }))))
        );
    }
}

mod mint {
    use crate::contract::{execute_mint, DEPOSITED_SYMBOL};
    use crate::error::ContractError::{Cw20Error, NoFundsSupplied};
    use crate::testing::tests::_do_simple_instantiate;
    use cosmwasm_std::testing::{mock_dependencies, mock_info};
    use cosmwasm_std::{Addr, Coin, Uint128};
    use cw20_base::state::{BALANCES, TOKEN_INFO};

    #[test]
    fn does_not_work_without_funds_sent() {
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);
        let dao_info = mock_info("dao_address", &[]);

        let res = execute_mint(deps.as_mut(), env, dao_info);
        assert_eq!(res, Err(NoFundsSupplied()));
    }

    #[test]
    fn non_dao_cannot_mint() {
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

        let funds = vec![Coin::new(500, DEPOSITED_SYMBOL)];
        let non_dao_info = mock_info("non dao", &funds);
        let res = execute_mint(deps.as_mut(), env, non_dao_info);
        assert_eq!(
            res,
            Err(Cw20Error(cw20_base::ContractError::Unauthorized {}))
        );
    }

    #[test]
    fn works_with_ntrn_funds() {
        let mut deps = mock_dependencies();
        let (_info, env) = _do_simple_instantiate(deps.as_mut(), None);

        let funds = vec![Coin::new(500, DEPOSITED_SYMBOL)];
        let dao_info = mock_info("dao_address", &funds);
        let res = execute_mint(deps.as_mut(), env, dao_info);
        assert!(res.is_ok());

        let config = TOKEN_INFO.load(&deps.storage).unwrap();
        assert_eq!(config.total_supply, Uint128::new(500));

        // sends on balance to airdrop_address
        let balance = BALANCES
            .load(&deps.storage, &Addr::unchecked("airdrop_address"))
            .unwrap();
        assert_eq!(balance, Uint128::new(500));
    }
}

mod compute_withdrawable_amount {
    use crate::contract::{compute_withdrawable_amount, VESTING_CLIFF};
    use crate::state::Schedule;
    use cosmwasm_std::Uint128;

    #[test]
    fn works_before_start_time() {
        let now: u64 = 0;
        let schedule = Schedule {
            start_time: 10,
            cliff: VESTING_CLIFF,
            duration: 2592000, // 30 days
        };
        let result =
            compute_withdrawable_amount(Uint128::new(100), Uint128::new(0), &schedule, now);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Uint128::zero());
    }

    #[test]
    fn works_after_start_time() {
        // 0.5 time passed
        let now: u64 = 1296000;
        let schedule = Schedule {
            start_time: 0,
            cliff: VESTING_CLIFF,
            duration: 2592000, // 30 days
        };
        let result =
            compute_withdrawable_amount(Uint128::new(100), Uint128::new(0), &schedule, now);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Uint128::new(50));

        // 0.75 time passed
        let now_2: u64 = 1944000;
        let result =
            compute_withdrawable_amount(Uint128::new(100), Uint128::new(0), &schedule, now_2);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Uint128::new(75));

        // all time passed
        let now_3: u64 = 3000000;
        let result =
            compute_withdrawable_amount(Uint128::new(100), Uint128::new(0), &schedule, now_3);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Uint128::new(100));

        // 0.5 passed and has already withdrawn funds
        let now_3: u64 = 1296000;
        let result =
            compute_withdrawable_amount(Uint128::new(100), Uint128::new(25), &schedule, now_3);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Uint128::new(25));
    }
}

mod query_vested_amount {
    use crate::contract::query_vested_amount;
    use crate::testing::tests::{_instantiate_vest_to_somebody, _withdraw_rewards};
    use cosmwasm_std::{StdError, Uint128};

    #[test]
    fn returns_error_when_non_existent_address_passed() {
        // instantiate
        let (deps, env) = _instantiate_vest_to_somebody(10_000_000, 100, None, 1000);

        let query_res = query_vested_amount(deps.as_ref(), env, "noname".to_string());
        assert_eq!(
            query_res,
            Err(StdError::not_found("credits::state::Allocation"))
        );
    }

    #[test]
    fn works_when_no_time_passed() {}

    #[test]
    fn works_when_time_passed() {
        // instantiate
        let (deps, mut env) = _instantiate_vest_to_somebody(10_000_000, 100, None, 1000);

        // at this point `somebody` has vested 100 NTRNs on 1000 seconds

        // pass 3/4 vesting duration (750 seconds)
        env.block.time = env.block.time.plus_seconds(750);

        let query_res =
            query_vested_amount(deps.as_ref(), env.clone(), "somebody".to_string()).unwrap();
        assert_eq!(query_res.amount, Uint128::new(25));

        // pass full vesting duration
        env.block.time = env.block.time.plus_seconds(250);
        let query_res =
            query_vested_amount(deps.as_ref(), env.clone(), "somebody".to_string()).unwrap();
        assert_eq!(query_res.amount, Uint128::new(0));

        // pass more than vesting duration
        env.block.time = env.block.time.plus_seconds(100);
        let query_res = query_vested_amount(deps.as_ref(), env, "somebody".to_string()).unwrap();
        assert_eq!(query_res.amount, Uint128::new(0));
    }

    #[test]
    fn works_when_burn_from_used_previously() {
        // instantiate
        let (mut deps, mut env) = _instantiate_vest_to_somebody(10_000_000, 100, None, 1000);

        // withdraw rewards for that account
        _withdraw_rewards(deps.as_mut(), env.clone(), 10);

        // pass 1/4 vesting duration (750 seconds)
        env.block.time = env.block.time.plus_seconds(250);

        // vested amount = 100 - 10 (withdrawn rewards) - 25 (possible to withdraw) = 65
        let query_res =
            query_vested_amount(deps.as_ref(), env.clone(), "somebody".to_string()).unwrap();
        assert_eq!(query_res.amount, Uint128::new(65));

        // pass 95% vesting duration
        env.block.time = env.block.time.plus_seconds(700);
        // vested amount = 0 because we have already withdrawn 10% of funds without vesting
        let query_res =
            query_vested_amount(deps.as_ref(), env.clone(), "somebody".to_string()).unwrap();
        assert_eq!(query_res.amount, Uint128::new(0));

        // pass full duration (1000 seconds)
        env.block.time = env.block.time.plus_seconds(50);
        // vested amount = 0
        let query_res = query_vested_amount(deps.as_ref(), env, "somebody".to_string()).unwrap();
        assert_eq!(query_res.amount, Uint128::new(0));
    }
}
