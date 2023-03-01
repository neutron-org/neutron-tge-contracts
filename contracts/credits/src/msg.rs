use cosmwasm_std::{Timestamp, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    /// Airdrop contract address
    pub airdrop_address: String,
    /// Lockdrop contract address,
    pub lockdrop_address: String,
    /// When can start withdrawing NTRN funds
    pub when_withdrawable: Timestamp,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// AddVesting is a message that allows address to claim particular amount of NTRNs at particular time.
    /// Can only store one vesting amount per address.
    /// [Permissioned - Airdrop address]
    AddVesting {
        address: String,
        amount: Uint128,
        start_time: u64,
        duration: u64,
    },
    /// Transfer is a base message to move tokens to another account without triggering actions.
    /// [Permissioned - Airdrop address]
    Transfer { recipient: String, amount: Uint128 },
    /// Withdraw is a message that burns all vested cNTRN tokens
    /// on the sender and sends NTRN tokens in 1:1 proportion.
    /// [Permissionless]
    Withdraw {},
    /// Burns is a message that burns certain amount of cntrn tokens and sends untrn tokens in 1:1 proportion.
    /// [Permissioned - Airdrop address]
    Burn { amount: Uint128 },
    /// BurnFrom burns owner's cNTRN tokens and mints NTRN tokens in 1:1 proportion certain amount for owner.
    /// Used to skip vesting as a reward for participating in the lockdrop.
    /// [Permissioned - Lockdrop address]
    BurnFrom { owner: String, amount: Uint128 },
    /// Locks the untrn tokens and mints ucntrn tokens in 1:1 amount to the airdrop balance.
    /// [Permissioned - DAO] (DAO address set in initialize func as cw20 minter)
    Mint {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns the current vestings of the given address.
    WithdrawableAmount { address: String },
    /// Returns the amount that is left vested of the given address.
    VestedAmount { address: String },
    /// Returns the current allocation of the given address.
    Allocation { address: String },
    /// Returns the current balance of the given address, 0 if unset.
    Balance { address: String },
    /// Returns the total supply at provided height, or current total supply if `height` is unset.
    TotalSupplyAtHeight { height: Option<u64> },
    /// Returns the balance of the given address at a given block height or current balance if `height` is unset.
    /// Returns 0 if no balance found.
    BalanceAtHeight {
        address: String,
        height: Option<u64>,
    },
    /// Returns metadata on the contract - name, decimals, supply, etc.
    TokenInfo {},
    /// Returns who can mint and the hard cap on maximum tokens after minting.
    Minter {},
    /// Returns how much spender can use from owner account, 0 if unset.
    Allowance { owner: String, spender: String },
    /// Returns all allowances this owner has approved. Supports pagination.
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns all accounts that have balances. Supports pagination.
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns current config of Credits contract
    Config {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TotalSupplyResponse {
    // Total supply of ucntrn for specified block height
    pub total_supply: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct WithdrawableAmountResponse {
    /// Amount that the user can withdraw at this block height.
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct VestedAmountResponse {
    /// Amount that is still vested for the user.
    pub amount: Uint128,
}
