use cosmwasm_std::{
    to_binary, Addr, Binary, CosmosMsg, QuerierWrapper, QueryRequest, StdResult, Uint128, WasmMsg,
    WasmQuery,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};

/// @dev Helper function which returns a cosmos wasm msg to transfer cw20 tokens to a recipient address
/// @param recipient : Address to be transferred cw20 tokens to
/// @param token_contract_address : Contract address of the cw20 token to transfer
/// @param amount : Number of tokens to transfer
pub fn build_transfer_cw20_token_msg(
    recipient: Addr,
    token_contract_address: String,
    amount: Uint128,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_contract_address,
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: recipient.into(),
            amount,
        })?,
        funds: vec![],
    }))
}

/// @dev Helper function which returns a cosmos wasm msg to send cw20 tokens to another contract which implements the ReceiveCW20 Hook
/// @param recipient_contract_addr : Contract Address to be transferred cw20 tokens to
/// @param token_contract_address : Contract address of the cw20 token to transfer
/// @param amount : Number of tokens to transfer
/// @param msg_ : ExecuteMsg coded into binary which needs to be handled by the recipient contract
pub fn build_send_cw20_token_msg(
    recipient_contract_addr: String,
    token_contract_address: String,
    amount: Uint128,
    msg_: Binary,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_contract_address,
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: recipient_contract_addr,
            amount,
            msg: msg_,
        })?,
        funds: vec![],
    }))
}

/// Helper function to get CW20 token balance of the user
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **token_address** is an object of type [`Addr`].
///
/// * **account_addr** is an object of type [`Addr`].
pub fn cw20_get_balance(
    querier: &QuerierWrapper,
    token_address: Addr,
    account_addr: Addr,
) -> StdResult<Uint128> {
    let query: BalanceResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: token_address.into(),
        msg: to_binary(&Cw20QueryMsg::Balance {
            address: account_addr.into(),
        })?,
    }))?;

    Ok(query.balance)
}

/// Helper function which returns a cosmos wasm msg to approve held cw20 tokens to be transferrable by beneficiary address
/// ## Params
/// * **token_contract_address** is an object of type [`String`]. Token contract address
///
/// * **spender_address** is an object of type [`String`]. Address to which allowance is being provided to, to allow it to transfer the tokens held by the contract
///
/// * **allowance_amount** is an object of type [`Uint128`]. Allowance amount
pub fn build_approve_cw20_msg(
    token_contract_address: String,
    spender_address: String,
    allowance_amount: Uint128,
    expiration_block: u64,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_contract_address,
        msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
            spender: spender_address,
            amount: allowance_amount,
            expires: Some(cw20::Expiration::AtHeight(expiration_block)),
        })?,
        funds: vec![],
    }))
}
