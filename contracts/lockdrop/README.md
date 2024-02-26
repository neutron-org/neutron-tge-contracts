# Lockdrop

The lockdrop contract allows users to lock any of the supported Terraswap LP tokens locked for a selected duration against which they will receive ASTRO tokens pro-rata to their weighted share of the LP tokens to the total deposited LP tokens for that particular pool in the contract.

- Upon lockup expiration, users will receive Astroport LP tokens on an equivalent weight basis as per their initial Terraswap LP token deposits.

Note - Users can open muliple lockup positions with different lockup duration for each LP Token pool

## Contract Design

### Handle Messages

| Message                                       | Description                                                                                                                                                                                                                                                                                                               |
|-----------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `ExecuteMsg::UpdateConfig`                    | Can only be called by the admin. Facilitates updating configuration parameters                                                                                                                                                                                                                                            |
| `ExecuteMsg::EnableClaims`                    | Executed by the Bootstrap auction contract when liquidity is added to the ASTRO-UST pool. Enables ASTRO withdrawals by the lockdrop recipients.                                                                                                                                                                           |
| `ExecuteMsg::InitializePool`                  | Admin function. Facilitates addition of new Pool (Terraswap Pools) whose LP tokens can then be locked in the lockdrop contract                                                                                                                                                                                            |
| `ExecuteMsg::UpdatePool`                      | Admin function to update any configuraton parameter for a terraswap pool whose LP tokens are currently accepted for the lockdrop                                                                                                                                                                                          |
| `ExecuteMsg::IncreaseLockup`                  | Facilitates opening a new user position or adding to an existing position                                                                                                                                                                                                                                                 |
| `ExecuteMsg::IncreaseAstroIncentives`         | Admin function to increase the ASTRO incentives that are to be distributed                                                                                                                                                                                                                                                |
| `ExecuteMsg::WithdrawFromLockup`              | Facilitates LP token withdrawals from lockup positions by users. 100% amount can be withdrawn during deposit window, which is then limited to 50% during 1st half of deposit window which then decreases linearly during 2nd half of deposit window. Only 1 withdrawal can be made by a user during the withdrawal windows |
| `ExecuteMsg::MigrateLiquidity`                | Admin function. Facilitates migration of liquidity (locked terraswap LP tokens) from Terraswap to Astroport (Astroport LP tokens)                                                                                                                                                                                         |
| `ExecuteMsg::StakeLpTokens`                   | Admin function. Facilitates staking of Astroport LP tokens for a particular LP pool with the generator contract                                                                                                                                                                                                           |
| `ExecuteMsg::DelegateAstroToAuction`          | This function facilitates ASTRO tokens delegation to the Bootstrap auction contract during the bootstrap auction phase. Delegated ASTRO tokens are added to the user's position in the bootstrap auction contract                                                                                                         |
| `ExecuteMsg::ClaimRewardsAndOptionallyUnlock` | Facilitates rewards claim by users for a particular lockup position along with unlock when possible                                                                                                                                                                                                                       |
| `ExecuteMsg::ClaimAssetReward`                | Collects assets reward from LP and distribute reward to user if all requirements are met                                                                                                                                                                                                                                  |
| `ExecuteMsg::TogglePoolRewards`               | Admin function. Enables assets reward for specified LP                                                                                                                                                                                                                                                                    |
| `ExecuteMsg::ProposeNewOwner`                 | Admin function. Creates an offer to change the contract ownership. The validity period of the offer is set in the `expires_in` variable. After `expires_in` seconds pass, the proposal expires and cannot be accepted anymore.                                                                                            |
| `ExecuteMsg::DropOwnershipProposal`           | Admin function. Removes an existing offer to change the contract owner.                                                                                                                                                                                                                                                   |
| `ExecuteMsg::ClaimOwnership`                  | Admin function. Used to claim contract ownership.                                                                                                                                                                                                                                                                         |

### Handle Messages :: Callback

| Message                                               | Description                                                                                                                                             |
|-------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------|
| `CallbackMsg::UpdatePoolOnDualRewardsClaim`           | Callback function to update contract state after pending dual staking rewards are claimed from the generator contract                                   |
| `CallbackMsg::WithdrawUserLockupRewardsCallback`      | Callback function to withdraw user rewards for a particular lockcup position along with optional LP tokens withdrawal (upon lockup duration expiration) |
| `CallbackMsg::WithdrawLiquidityFromTerraswapCallback` | Callback function used during liquidity migration to update state after liquidity is removed from terraswap                                             |
| `CallbackMsg::DistributeAssetReward`                  | Callback function used for assets reward distribution after rewards claiming from LP                                                                    |

### Query Messages

| Message                         | Description                                                                                                      |
|---------------------------------|------------------------------------------------------------------------------------------------------------------|
| `QueryMsg::Config`              | Returns the config info                                                                                          |
| `QueryMsg::State`               | Returns the contract's global state                                                                              |
| `QueryMsg::Pool`                | Returns info regarding a certain supported LP token pool                                                         |
| `QueryMsg::UserInfo`            | Returns info regarding a user (total ASTRO rewards, list of lockup positions)                                    |
| `QueryMsg::LockUpInfo`          | Returns info regarding a particular lockup position with a given duration and identifer for the LP tokens locked |
| `QueryMsg::PendingAssetReward`  | Returns the amount of pending asset rewards for the specified recipient and for a specific lockup position       |

## Build schema and run unit-tests

```
cargo schema
cargo test
```

## License

TBD
