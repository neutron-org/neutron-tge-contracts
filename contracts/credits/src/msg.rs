use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{/*Addr, Binary,*/ Timestamp, Uint128};
use cw_utils::Expiration;

#[cw_serde]
pub struct InstantiateMsg {
    /// Date when you can execute `burn` method to burn CNTRN and get NTRN tokens
    pub when_claimable: Timestamp,
    /// DAO contract address
    pub dao_address: String,
    /// Airdrop contract address
    pub airdrop_address: String,
    /// Sale contract address
    pub sale_contract_address: String,
    /// Lockdrop contract address,
    pub lockdrop_address: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Transfer is a base message to move tokens to another account without triggering actions
    Transfer { recipient: String, amount: Uint128 },
    /// Burn is a base message to destroy CNTRN's forever and send NTRN tokens in 1:1 proportion
    Burn {},
    /// Only with "approval" extension. Allows spender to access an additional amount tokens
    /// from the owner's (env.sender) account. If expires is Some(), overwrites current allowance
    /// expiration with this one.
    IncreaseAllowance {
        spender: String,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    /// Only with "approval" extension. Lowers the spender's access of tokens
    /// from the owner's (env.sender) account by amount. If expires is Some(), overwrites current
    /// allowance expiration with this one.
    DecreaseAllowance {
        spender: String,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    /// Only with "approval" extension. Transfers amount tokens from owner -> recipient
    /// if `env.sender` has sufficient pre-approval.
    TransferFrom {
        owner: String,
        recipient: String,
        amount: Uint128,
    },
    /// If authorized (only dao can call),
    /// locks the NTRN tokens and mints CNTRN tokens in 1:1 amount
    /// and adds to the dao balance.
    Mint {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}

#[cw_serde]
pub struct MigrateMsg {}
