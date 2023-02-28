# Airdrop

The Airdrop contract is for ASTRO tokens airdrop claim during the intital protocol launch.

## Contract Design

### Handle Messages

| Message                                       | Description                                                                                                                                                                                                                                            |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `ExecuteMsg::Claim`                           | Executes an airdrop claim for Users.                                                                                                                                                                                                                   |
| `ExecuteMsg::DelegateAstroToBootstrapAuction` | This function facilitates ASTRO tokens delegation to the Bootstrap auction contract after airdrop is claimed by the user during the bootstrap auction phase. Delegated ASTRO tokens are added to the user's position in the bootstrap auction contract |
| `ExecuteMsg::EnableClaims`                    | Executed by the Bootstrap auction contract when liquidity is added to the ASTRO-UST pool. Enables ASTRO withdrawals by the airdrop recipients.                                                                                                         |
| `ExecuteMsg::WithdrawAirdropReward`           | Facilitates ASTRO withdrawal for airdrop recipients once claim withdrawals are allowed                                                                                                                                                                 |
| `ExecuteMsg::TransferUnclaimedTokens`         | Admin function. Transfers unclaimed ASTRO tokens available with the contract to the recipient address once the claim window is over                                                                                                                    |
| `ExecuteMsg::UpdateConfig`                    | Admin function to update any of the configuration parameters.                                                                                                                                                                                          |

- Before the completion of LP bootstrap via auction phase, airdrop claims create user position's within the contract via which users can choose how many ASTRO tokens they want to provide for the LP bootstrap via auction, and withdraw the remaining ASTRO post the completion of LP bootstrap via auction phase

- Post the completion of LP bootstrap via auction phase, any airdrop claim by the user transfers the user's max ASTRO airdrop amount to the user's wallet.

### Query Messages

| Message                    | Description                                                                                                         |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `QueryMsg::Config`         | Returns the config info                                                                                             |
| `QueryMsg::State`          | Returns the contract's state info                                                                                   |
| `QueryMsg::HasUserClaimed` | Returns a boolean value indicating if the corresponding address (terra / evm) have yet claimed their airdrop or not |
| `QueryMsg::UserInfo`       | Returns user's airdrop claim state (total airdrop size and ASTRO delegated balances)                                |

## How to Guide :: Get merkle proofs

### Create distribution lists for terra and evm users

claimees_data.json

```
{[ { address: 'terra1k0jntykt7e4g3y88ltc60czgjuqdy4c9ax8tx2',
    amount: '43454523323'
  },
  { address: 'terra1xzlgeyuuyqje79ma6vllregprkmgwgavjx2h6m',
    amount: '1343252443'
  }
]}
```

### Get proof with user input

```
    import  {Terra_Merkle_Tree}  from "./helpers/terra_merkle_tree.js";

    const terra_merkle_tree = new Terra_Merkle_Tree(terra_claimees_data);
    const terra_tree_root = terra_merkle_tree.getMerkleRoot();

    let merkle_proof_for_terra_user_ = terra_merkle_tree.getMerkleProof({  "address":"terra1k0jntykt7e4g3y88ltc60czgjuqdy4c9ax8tx2",
                                                                            "amount": (43454523323).toString()
                                                                        } );

    console.log("Terra Merkle Root ", terra_tree_root)
    console.log("Terra Merkle Proof ", merkle_proof_for_terra_user_)
    console.log("Verify Terra Merkle Proof ", terra_merkle_tree.verify({  "address":"terra1k0jntykt7e4g3y88ltc60czgjuqdy4c9ax8tx2",
                                                                            "amount": (43454523323).toString()
                                                                        }) )

```

## Build schema and run unit-tests

```
cargo schema
cargo test
```

## License

TBD
