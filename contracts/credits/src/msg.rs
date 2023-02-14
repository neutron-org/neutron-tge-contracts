use crate::state::Allocation;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};
use cw_utils::Expiration;

#[cw_serde]
pub struct InstantiateMsg {
    /// DAO contract address
    pub dao_address: String,
    /// Airdrop contract address
    pub airdrop_address: Option<String>,
    /// Lockdrop contract address,
    pub lockdrop_address: Option<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// UpdateConfig is a message to initialize all addresses because there are circle deps between contracts
    UpdateConfig {
        airdrop_address: String,
        lockdrop_address: String,
    },
    // AddVesting is a message that allows address to claim particular amount of NTRNs at particular time.
    // Can store multiple vestings with different claimable dates for the same address.
    AddVesting {
        address: String,
        amount: Uint128,
        start_time: u64,
        duration: u64,
    },
    /// Transfer is a base message to move tokens to another account without triggering actions
    Transfer { recipient: String, amount: Uint128 },
    // TODO: rename
    /// Withdraw is a message that burns all vested CNTRN tokens on the sender and sends NTRN tokens in 1:1 proportion
    Withdraw {},
    /// Burn is a message only for `config.lockdrop` account to destroy certain amount of CNTRN's forever and send NTRN tokens in 1:1 proportion
    /// Used for giving lockdrop rewards
    Burn { amount: Uint128 },
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
    /// Returns the current vestings of the given address.
    #[returns(WithdrawableAmountResponse)]
    WithdrawableAmount { address: String },
    /// Returns the current allocation of the given address
    #[returns(AllocationResponse)]
    Allocation { address: String },
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
    // TODO: handler to liquidize some portion tokens for address. Also needs implementation changes in withdraw section.
    // Liquidize { address: String, amount: Uint128 }
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub struct ConfigResponse {
    /// DAO contract address
    pub dao_address: Addr,
    /// Airdrop contract address
    pub airdrop_address: Option<Addr>,
    /// Lockdrop contract address,
    pub lockdrop_address: Option<Addr>,
}

#[cw_serde]
pub struct WithdrawableAmountResponse {
    pub amount: Uint128,
}

#[cw_serde]
pub struct AllocationResponse {
    /// Current allocation for a user
    pub allocation: Allocation,
}
