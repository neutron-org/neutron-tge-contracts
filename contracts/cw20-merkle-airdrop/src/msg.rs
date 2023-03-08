use crate::ContractError;
use cosmwasm_std::{from_slice, Binary, Timestamp, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub credits_address: String,
    pub reserve_address: String,
    /// MerkleRoot is hex-encoded merkle root.
    pub merkle_root: String,
    /// A point in time from which it is possible to claim airdrops
    pub airdrop_start: Timestamp,
    /// A point in time from which a vesting is configured for cNTRNs. At this point, it is still
    /// possible for users to claim their airdrops.
    pub vesting_start: Timestamp,
    /// Total duration of vesting. At `vesting_start.seconds() + vesting_duration_seconds`
    /// point of time it is no longer possible to claim airdrops. At the very same point of time,
    /// it is possible to withdraw all remaining cNTRNs, exchange them for NTRNs and send to
    /// reserve, using `[ExecuteMsg::WithdrawAll]` message
    pub vesting_duration_seconds: u64,
    pub total_amount: Option<Uint128>,
    /// hrp is the bech32 parameter required for building external network address
    /// from signature message during claim action. example "cosmos", "terra", "juno"
    pub hrp: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Claim does not check if contract has enough funds, owner must ensure it.
    Claim {
        amount: Uint128,
        /// Proof is hex-encoded merkle proof.
        proof: Vec<String>,
        /// Enables cross chain airdrops.
        /// Target wallet proves identity by sending a signed [SignedClaimMsg](SignedClaimMsg)
        /// containing the recipient address.
        sig_info: Option<SignatureInfo>,
    },
    /// Permissionless, activated after vesting is over (consult to `[InstantiateMsg]`
    /// documentation for more info). Withdraws all remaining cNTRN tokens, burns them,
    /// receiving NTRN in exchange, and sends all received NTRN's to reserve.
    WithdrawAll {},
    Pause {},
    Resume {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    MerkleRoot {},
    IsClaimed {
        address: String,
    },
    TotalClaimed {},
    // for cross chain airdrops, maps target account to host account
    AccountMap {
        external_address: String,
    },
    AllAccountMaps {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    IsPaused {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ConfigResponse {
    pub owner: String,
    pub credits_address: String,
    pub reserve_address: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MerkleRootResponse {
    /// MerkleRoot is hex-encoded merkle root.
    pub merkle_root: String,
    pub airdrop_start: Timestamp,
    pub vesting_start: Timestamp,
    pub vesting_duration_seconds: u64,
    pub total_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct IsClaimedResponse {
    pub is_claimed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct IsPausedResponse {
    pub is_paused: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct TotalClaimedResponse {
    pub total_claimed: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AccountMapResponse {
    pub host_address: String,
    pub external_address: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct AllAccountMapResponse {
    pub address_maps: Vec<AccountMapResponse>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

// Signature verification is done on external airdrop claims.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct SignatureInfo {
    pub claim_msg: Binary,
    pub signature: Binary,
}
impl SignatureInfo {
    pub fn extract_addr(&self) -> Result<String, ContractError> {
        let claim_msg = from_slice::<ClaimMsg>(&self.claim_msg)?;
        Ok(claim_msg.address)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ClaimMsg {
    // To provide claiming via ledger, the address is passed in the memo field of a cosmos msg.
    #[serde(rename = "memo")]
    address: String,
}
