use crate::ContractError;
use cosmwasm_std::{from_slice, Binary, Timestamp, Uint128};
use cw_utils::Scheduled;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub credits_address: Option<String>,
    pub reserve_address: Option<String>,
    pub neutron_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    UpdateConfig {
        /// NewOwner if non sent, contract gets locked. Recipients can receive airdrops
        /// but owner cannot register new stages.
        new_owner: Option<String>,
        new_credits_address: Option<String>,
        new_reserve_address: Option<String>,
        new_neutron_denom: Option<String>,
    },
    RegisterMerkleRoot {
        /// MerkleRoot is hex-encoded merkle root.
        merkle_root: String,
        expiration: Timestamp,
        start: Option<Scheduled>,
        total_amount: Option<Uint128>,
        // hrp is the bech32 parameter required for building external network address
        // from signature message during claim action. example "cosmos", "terra", "juno"
        hrp: Option<String>,
    },
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
    /// Withdraw all remaining tokens that the contract owns (only owner)
    WithdrawAll {},
    Pause {},
    Resume {
        new_expiration: Option<Timestamp>,
    },
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
    pub credits_address: Option<String>,
    pub reserve_address: Option<String>,
    pub neutron_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MerkleRootResponse {
    /// MerkleRoot is hex-encoded merkle root.
    pub merkle_root: String,
    pub expiration: Timestamp,
    pub start: Option<Scheduled>,
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
