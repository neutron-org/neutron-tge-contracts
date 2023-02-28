use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Timestamp, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    /// Airdrop contract address
    pub airdrop_address: Option<String>,
    /// Lockdrop contract address,
    pub lockdrop_address: Option<String>,
    /// When can start withdrawing NTRN funds
    pub when_withdrawable: Timestamp,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// UpdateConfig is a message to initialize all addresses.
    /// Needed because there are circle deps between contracts.
    /// [Permissioned - DAO]
    UpdateConfig {
        airdrop_address: String,
        lockdrop_address: String,
    },
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
    /// Burn is a message only for airdrop account to burn
    /// certain amount of cntrn tokens and send untrn tokens in 1:1 proportion.
    /// [Permissioned - Airdrop address]
    Burn { amount: Uint128 },
    /// BurnFrom is a message only for lockdrop contract
    /// to burn owner's cNTRN tokens and mint NTRN tokens in 1:1 proportion certain amount for owner.
    /// Used to skip vesting as a reward for participating in the lockdrop.
    /// [Permissioned - Lockdrop address]
    BurnFrom { owner: String, amount: Uint128 },
    /// Locks the untrn tokens and mints ucntrn tokens in 1:1 amount to the airdrop balance.
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
    #[returns(crate::state::Allocation)]
    Allocation { address: String },
    /// Returns the current balance of the given address, 0 if unset.
    #[returns(cw20::BalanceResponse)]
    Balance { address: String },
    /// Returns the total supply at provided height, or current total supply if `height` is unset.
    #[returns(TotalSupplyResponse)]
    TotalSupplyAtHeight { height: Option<u64> },
    /// Returns the balance of the given address at a given block height or current balance if `height` is unset.
    /// Returns 0 if no balance found.
    #[returns(cw20::BalanceResponse)]
    BalanceAtHeight {
        address: String,
        height: Option<u64>,
    },
    /// Returns metadata on the contract - name, decimals, supply, etc.
    #[returns(cw20::TokenInfoResponse)]
    TokenInfo {},
    /// Returns who can mint and the hard cap on maximum tokens after minting.
    #[returns(cw20::MinterResponse)]
    Minter {},
    /// Returns how much spender can use from owner account, 0 if unset.
    #[returns(cw20::AllowanceResponse)]
    Allowance { owner: String, spender: String },
    /// Returns all allowances this owner has approved. Supports pagination.
    #[returns(cw20::AllAllowancesResponse)]
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns all allowances this spender has been granted. Supports pagination.
    #[returns(cw20::AllSpenderAllowancesResponse)]
    AllSpenderAllowances {
        spender: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns all accounts that have balances. Supports pagination.
    #[returns(cw20::AllAccountsResponse)]
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Returns current config of Credits contract
    #[returns(crate::state::Config)]
    Config {},
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
    /// When can start withdrawing NTRN funds
    pub when_withdrawable: Timestamp,
}

#[cw_serde]
pub struct TotalSupplyResponse {
    // Total supply of ucntrn for specified block height
    pub total_supply: Uint128,
}

#[cw_serde]
pub struct WithdrawableAmountResponse {
    /// Amount that the user can withdraw at this block height.
    pub amount: Uint128,
}

#[cw_serde]
pub struct VestedAmountResponse {
    /// Amount that is still vested for the user.
    pub amount: Uint128,
}
