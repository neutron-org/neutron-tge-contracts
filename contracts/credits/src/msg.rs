use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Timestamp, Uint128};
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
pub enum QueryMsg {
    /// Returns the current balance of the given address, 0 if unset.
    #[returns(cw20::BalanceResponse)]
    Balance { address: String },
    /// Returns metadata on the contract - name, decimals, supply, etc.
    #[returns(cw20::TokenInfoResponse)]
    TokenInfo {},
    /// Only with "mintable" extension.
    /// Returns who can mint and the hard cap on maximum tokens after minting.
    #[returns(cw20::MinterResponse)]
    Minter {},
    /// Only with "allowance" extension.
    /// Returns how much spender can use from owner account, 0 if unset.
    #[returns(cw20::AllowanceResponse)]
    Allowance { owner: String, spender: String },
    /// Only with "enumerable" extension (and "allowances")
    /// Returns all allowances this owner has approved. Supports pagination.
    #[returns(cw20::AllAllowancesResponse)]
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Only with "enumerable" extension (and "allowances")
    /// Returns all allowances this spender has been granted. Supports pagination.
    #[returns(cw20::AllSpenderAllowancesResponse)]
    AllSpenderAllowances {
        spender: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Only with "enumerable" extension
    /// Returns all accounts that have balances. Supports pagination.
    #[returns(cw20::AllAccountsResponse)]
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns current config of Credits contract
    #[returns(ConfigResponse)]
    Config {},
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct ConfigResponse {
    /// Date when you can execute `burn` method to burn CNTRN and get NTRN tokens
    pub when_claimable: Timestamp,
    /// DAO contract address
    pub dao_address: Addr,
    /// Airdrop contract address
    pub airdrop_address: Addr,
    /// Sale contract address
    pub sale_address: Addr,
    /// Lockdrop contract address,
    pub lockdrop_address: Addr,
}
