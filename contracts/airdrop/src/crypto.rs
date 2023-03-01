use cosmwasm_std::{Addr, StdError, StdResult, Uint128};
use sha3::{Digest, Keccak256};
use std::cmp::Ordering;
use std::convert::TryInto;

/// Verify whether a claim is valid.
/// ## Params
/// * **account** is an object of type [`Addr`]. Account on behalf of which the airdrop is to be claimed (etherum addresses without `0x` prefix)
///
/// * **amount** is an object of type [`Uint128`]. Airdrop amount to be claimed by the user
///
/// * **merkle_proof** is a vector of type [`String`]. Array of hashes to prove the input is a leaf of the Merkle Tree
///
/// * **merkle_root** is an object of type [`str`]. Hash of Merkle tree's root
pub fn verify_claim(
    account: &Addr,
    amount: Uint128,
    merkle_proof: Vec<String>,
    merkle_root: &str,
) -> StdResult<bool> {
    let leaf = account.to_string() + &amount.to_string();
    let mut hash_buf = Keccak256::digest(leaf.as_bytes())
        .as_slice()
        .try_into()
        .map_err(|_| StdError::generic_err("Wrong length"))?;
    let mut hash_str: String;

    for p in merkle_proof {
        let mut proof_buf: [u8; 32] = [0; 32];
        hex::decode_to_slice(p, &mut proof_buf).unwrap();
        let proof_buf_str = hex::encode(proof_buf);
        hash_str = hex::encode(hash_buf);

        if proof_buf_str.cmp(&hash_str.clone()) == Ordering::Greater {
            hash_buf = Keccak256::digest(&[hash_buf, proof_buf].concat())
                .as_slice()
                .try_into()
                .map_err(|_| StdError::generic_err("Wrong length"))?
        } else {
            hash_buf = Keccak256::digest(&[proof_buf, hash_buf].concat())
                .as_slice()
                .try_into()
                .map_err(|_| StdError::generic_err("Wrong length"))?
        }
    }

    hash_str = hex::encode(hash_buf);
    Ok(merkle_root == hash_str)
}