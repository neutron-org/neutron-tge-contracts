use cosmwasm_schema::QueryResponses;
use cosmwasm_std::{to_json_binary, CosmosMsg, Decimal, Env, StdResult, Uint128, WasmMsg};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct InstantiateMsg {
    pub axl_pool: String,
    pub axl_usdc_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    /// Callbacks; only callable by the contract itself.
    Callback(CallbackMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    SwapCallback {
        transmuter_pool: String,
        axl_usdc_prev_balance: Uint128,
        ntrn_prev_balance: Uint128,
        provide_liquidity_slippage_tolerance: Decimal,
        caller: String,
        noble_pool: String,
        noble_usdc_denom: String,
    },
    StakeCallback {
        noble_usdc_prev_balance: Uint128,
        ntrn_to_provide: Uint128,
        provide_liquidity_slippage_tolerance: Decimal,
        caller: String,
        noble_pool: String,
        noble_usdc_denom: String,
    },
}

// Modified from
// https://github.com/CosmWasm/cosmwasm-plus/blob/v0.2.3/packages/cw20/src/receiver.rs#L15
impl CallbackMsg {
    pub fn to_cosmos_msg(self, env: &Env) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_json_binary(&ExecuteMsg::Callback(self))?,
            funds: vec![],
        }))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(crate::state::Config)]
    Config {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    ConvertAndStake {
        transmuter_pool: String,
        noble_pool: String,
        noble_usdc_denom: String,
        provide_liquidity_slippage_tolerance: Decimal,
    },
}
