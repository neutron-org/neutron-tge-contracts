use crate::state::{Allocation, Config};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use cw20::{
    AllAccountsResponse, AllAllowancesResponse, AllowanceResponse, BalanceResponse, MinterResponse,
    TokenInfoResponse,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub dao_address: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct UpdateConfigMsg {
    /// Airdrop contract address
    pub airdrop_address: Option<String>,
    /// Lockdrop contract address,
    pub lockdrop_address: Option<String>,
    /// When can start withdrawing untrn tokens
    pub when_withdrawable: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// UpdateConfig is a message that allows to update config of the contract.
    /// [Permissioned - DAO]
    UpdateConfig { config: UpdateConfigMsg },
    /// AddVesting is a message that allows address to claim particular amount of untrn tokens at particular time.
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
    /// on the sender and sends untrn tokens in 1:1 proportion.
    /// [Permissionless]
    Withdraw {},
    /// Burns is a message that burns certain amount of cNTRN tokens and sends untrn tokens in 1:1 proportion.
    /// [Permissioned - Airdrop address]
    Burn { amount: Uint128 },
    /// BurnFrom burns owner's cNTRN tokens and mints untrn tokens in 1:1 proportion specified amount for owner.
    /// Used to skip vesting as a reward for participating in the lockdrop.
    /// [Permissioned - Lockdrop address]
    BurnFrom { owner: String, amount: Uint128 },
    /// Locks untrn tokens and mints cNTRN tokens in 1:1 proportion to the airdrop balance.
    /// [Permissioned - DAO] (DAO address set in initialize func as cw20 minter)
    Mint {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the current vestings of the given address.
    #[returns(WithdrawableAmountResponse)]
    WithdrawableAmount { address: String },
    /// Returns the amount that is left vested of the given address.
    #[returns(VestedAmountResponse)]
    VestedAmount { address: String },
    /// Returns the current allocation of the given address.
    #[returns(Allocation)]
    Allocation { address: String },
    /// Returns the current balance of the given address, 0 if unset.
    #[returns(BalanceResponse)]
    Balance { address: String },
    /// Returns the total supply at provided height, or current total supply if `height` is unset.
    #[returns(TotalSupplyResponse)]
    TotalSupplyAtHeight { height: Option<u64> },
    /// Returns the balance of the given address at a given block height or current balance if `height` is unset.
    /// Returns 0 if no balance found.
    #[returns(BalanceResponse)]
    BalanceAtHeight {
        address: String,
        height: Option<u64>,
    },
    /// Returns metadata on the contract - name, decimals, supply, etc.
    #[returns(TokenInfoResponse)]
    TokenInfo {},
    /// Returns who can mint and the hard cap on maximum tokens after minting.
    #[returns(Option<MinterResponse>)]
    Minter {},
    /// Returns how much spender can use from owner account, 0 if unset.
    #[returns(AllowanceResponse)]
    Allowance { owner: String, spender: String },
    /// Returns all allowances this owner has approved. Supports pagination.
    #[returns(AllAllowancesResponse)]
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns all accounts that have balances. Supports pagination.
    #[returns(AllAccountsResponse)]
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns current config of Credits contract
    #[returns(Config)]
    Config {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TotalSupplyResponse {
    // Total supply of cNTRN tokens for specified block height
    pub total_supply: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct WithdrawableAmountResponse {
    /// Amount that the user can withdraw at this block height.
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct VestedAmountResponse {
    /// Amount that is still vested for the user.
    pub amount: Uint128,
}
