use cosmwasm_std::{Addr, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub owner: Option<String>,
    pub astro_token_address: String,
    pub merkle_roots: Option<Vec<String>>,
    pub from_timestamp: Option<u64>,
    pub to_timestamp: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    /// Admin function to update the configuration parameters
    UpdateConfig {
        owner: Option<String>,
        auction_contract_address: Option<String>,
        merkle_roots: Option<Vec<String>>,
        from_timestamp: Option<u64>,
        to_timestamp: Option<u64>,
    },
    /// Called by the bootstrap auction contract when liquidity is added to the
    /// ASTRO-UST Pool to enable ASTRO withdrawals by users
    EnableClaims {},
    /// Allows Terra users to claim their ASTRO Airdrop
    Claim {
        claim_amount: Uint128,
        merkle_proof: Vec<String>,
        root_index: u32,
    },
    /// Allows users to delegate their ASTRO tokens to the LP Bootstrap auction contract
    DelegateAstroToBootstrapAuction {
        amount_to_delegate: Uint128,
    },
    /// Allows users to withdraw their ASTRO tokens
    WithdrawAirdropReward {},
    /// Admin function to facilitate transfer of the unclaimed ASTRO Tokens
    TransferUnclaimedTokens {
        recipient: String,
        amount: Uint128,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    IncreaseAstroIncentives {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
    UserInfo { address: String },
    HasUserClaimed { address: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct Config {
    /// Account who can update config
    pub owner: Addr,
    ///  ASTRO token address
    pub astro_token_address: Addr,
    /// Merkle roots used to verify is a terra user is eligible for the airdrop
    pub merkle_roots: Vec<String>,
    /// Timestamp since which ASTRO airdrops can be delegated to bootstrap auction contract
    pub from_timestamp: u64,
    /// Timestamp to which ASTRO airdrops can be claimed
    pub to_timestamp: u64,
    /// Bootstrap auction contract address
    pub auction_contract_address: Option<Addr>,
    /// Boolean value indicating if the users can withdraw their ASTRO airdrop tokens or not
    /// This value is updated in the same Tx in which Liquidity is added to the LP Pool
    pub are_claims_enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct State {
    /// Total ASTRO issuance used as airdrop incentives
    pub total_airdrop_size: Uint128,
    /// Total ASTRO tokens that have been delegated to the bootstrap auction pool
    pub total_delegated_amount: Uint128,
    /// Total ASTRO tokens that are yet to be claimed by the users
    pub unclaimed_tokens: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub struct UserInfo {
    /// Total ASTRO airdrop tokens claimable by the user
    pub claimed_amount: Uint128,
    /// ASTRO tokens delegated to the bootstrap auction contract to add to the user's position
    pub delegated_amount: Uint128,
    /// Boolean value indicating if the user has withdrawn the remaining ASTRO tokens
    pub tokens_withdrawn: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ClaimResponse {
    pub is_claimed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
