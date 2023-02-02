# LP Bootstrap via Auction Contract

The LP Bootstrap via auction contract facilitates ASTRO-UST Astroport pool initialization during the protocol launch.

**Phase 1 :: Bootstrapping ASTRO and UST Side of the LP Pool**

- Airdrop recipients and lockdrop participants can delegate part / all of their ASTRO rewards to the auction contract.
- Any user can deposit UST directly to the auction contract to participate in the LP bootstrap auction.
- Both UST deposited & ASTRO delegated (if any) balances are used to calculate user's LP token shares and additional ASTRO incentives that he will receive for participating in the auction.

**Phase 2 :: Post ASTRO-UST Pool initialization**

- ASTRO reward withdrawals from lockdrop & airdrop contracts are enabled during the ASTRO-UST Pool initializaiton.
- ASTRO-UST LP tokens are staked with the generator contract, with LP Staking rewards allocated equally among the users based on their % LP share
- ASTRO incentives are directly claimable
- Users ASTRO-UST LP shares are vested linearly on a 90 day period

## Contract Design

### Handle Messages

| Message                     | Description                                                                                                                                                                                                                                                                                    |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ExecuteMsg::Receive`       | ReceiveCW20 Hook which facilitates ASTRO tokens delegation by lockdrop participants / airdrop recipients                                                                                                                                                                                       |
| `ExecuteMsg::UpdateConfig`  | Admin function to update any of the configuration parameters.                                                                                                                                                                                                                                  |
| `ExecuteMsg::DepositUst`    | Facilitates UST deposits by users                                                                                                                                                                                                                                                              |
| `ExecuteMsg::WithdrawUst`   | Facilitates UST withdrawals by users. 100% amount can be withdrawn during deposit window, which is then limited to 50% during 1st half of deposit window which then decreases linearly during 2nd half of deposit window. Only 1 withdrawal can be made by a user during the withdrawal window |
| `ExecuteMsg::InitPool`      | Admin function which facilitates Liquidity addtion to the Astroport ASTRO-UST Pool. Uses CallbackMsg to update state post liquidity addition to the pool                                                                                                                                       |
| `ExecuteMsg::StakeLpTokens` | Admin function to stake ASTRO-UST LP tokens with the generator contract                                                                                                                                                                                                                        |
| `ExecuteMsg::ClaimRewards`  | Facilitates ASTRO rewards claim (staking incentives from generator) for users and the withdrawal of LP shares which have been unlocked for the user.                                                                                                                                           |

### Handle Messages :: Callback

| Message                                             | Description                                                                                          |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `CallbackMsg::UpdateStateOnLiquidityAdditionToPool` | Callback function to update state after liquidity is added to the ASTRO-UST Pool                     |
| `CallbackMsg::UpdateStateOnRewardClaim`             | Callback function to update state after ASTRO rewards are claimed from the generator                 |
| `CallbackMsg::WithdrawUserRewardsCallback`          | Callback function to facilitate ASTRO reward claiming and unlocked LP tokens withdrawal for the user |

### Query Messages

| Message              | Description                   |
| -------------------- | ----------------------------- |
| `QueryMsg::Config`   | Returns the config info       |
| `QueryMsg::State`    | Returns state of the contract |
| `QueryMsg::UserInfo` | Returns user position details |

## Build schema and run unit-tests

```
cargo schema
cargo test
```

## License

TBD
