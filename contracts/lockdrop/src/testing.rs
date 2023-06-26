use crate::contract::{execute, instantiate, query, UNTRN_DENOM};
use crate::state::MIGRATION_STATUS;
use astroport_periphery::lockdrop::{
    Config, ExecuteMsg, InstantiateMsg, LockupRewardsInfo, MigrationState, QueryMsg,
};
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{coin, from_binary, Addr, Decimal256, StdError, Uint128};

#[test]
fn update_owner() {
    let mut deps = mock_dependencies();
    let info = mock_info("addr0000", &[]);
    MIGRATION_STATUS
        .save(&mut deps.storage, &MigrationState::Completed)
        .unwrap();
    let owner = Addr::unchecked("owner");
    let token_info_manager = Addr::unchecked("token_info_manager");

    let env = mock_env();

    let msg = InstantiateMsg {
        owner: Some(owner.to_string()),
        token_info_manager: token_info_manager.to_string(),
        init_timestamp: env.block.time.seconds(),
        lock_window: 10_000_000,
        withdrawal_window: 500_000,
        min_lock_duration: 1u64,
        max_lock_duration: 52u64,
        max_positions_per_user: 14,
        credits_contract: "credit_contract".to_string(),
        lockup_rewards_info: vec![LockupRewardsInfo {
            duration: 1,
            coefficient: Decimal256::zero(),
        }],
        auction_contract: "auction_contract".to_string(),
    };

    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    let new_owner = String::from("new_owner");

    // BNew owner
    let env = mock_env();
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    let info = mock_info(new_owner.as_str(), &[]);

    // Unauthorized check
    let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Claim before a proposal
    let info = mock_info(new_owner.as_str(), &[]);
    execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap_err();

    // Propose new owner
    let info = mock_info(owner.as_str(), &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // Unauthorized ownership claim
    let info = mock_info("invalid_addr", &[]);
    let err = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Claim ownership
    let info = mock_info(new_owner.as_str(), &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap();
    assert_eq!(0, res.messages.len());

    // Let's query the state
    let config: Config =
        from_binary(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!(new_owner, config.owner);
}

#[test]
fn increase_ntrn_incentives() {
    let mut deps = mock_dependencies();
    let info = mock_info("addr0000", &[]);

    let owner = Addr::unchecked("owner");
    let token_info_manager = Addr::unchecked("token_info_manager");
    MIGRATION_STATUS
        .save(&mut deps.storage, &MigrationState::Completed)
        .unwrap();
    let env = mock_env();

    let msg = InstantiateMsg {
        owner: Some(owner.to_string()),
        token_info_manager: token_info_manager.to_string(),
        init_timestamp: env.block.time.seconds(),
        lock_window: 10_000_000,
        withdrawal_window: 500_000,
        min_lock_duration: 1u64,
        max_lock_duration: 52u64,
        max_positions_per_user: 14,
        credits_contract: "credit_contract".to_string(),
        auction_contract: "auction_contract".to_string(),
        lockup_rewards_info: vec![LockupRewardsInfo {
            duration: 1,
            coefficient: Decimal256::zero(),
        }],
    };

    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    let env = mock_env();
    let msg = ExecuteMsg::IncreaseNTRNIncentives {};

    let info = mock_info(owner.as_str(), &[coin(100u128, UNTRN_DENOM)]);

    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let config: Config =
        from_binary(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!(Uint128::new(100u128), config.lockdrop_incentives);

    // invalid coin
    let env = mock_env();
    let msg = ExecuteMsg::IncreaseNTRNIncentives {};

    let info = mock_info(owner.as_str(), &[coin(100u128, "DENOM")]);

    let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
    assert_eq!(
        err,
        StdError::generic_err(format!("{} is not found", UNTRN_DENOM))
    );
}
