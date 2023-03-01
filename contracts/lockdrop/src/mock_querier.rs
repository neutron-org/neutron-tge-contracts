use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, Coin, Empty, OwnedDeps, Querier, QuerierResult,
    QueryRequest, SystemError, SystemResult, Uint128, WasmQuery,
};
use std::collections::HashMap;

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::QueryMsg::{Config, FeeInfo};
use astroport::factory::{ConfigResponse, FeeInfoResponse, PairType};
use astroport::pair::QueryMsg::Pair;
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse, TokenInfoResponse};

/// mock_dependencies is a drop-in replacement for cosmwasm_std::testing::mock_dependencies
/// this uses our CustomQuerier.
pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(MOCK_CONTRACT_ADDR, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
        custom_query_type: Default::default(),
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<Empty>,
    token_querier: TokenQuerier,
}

#[derive(Clone, Default)]
pub struct TokenQuerier {
    // this lets us iterate over all pairs that match the first string
    balances: HashMap<String, HashMap<String, Uint128>>,
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {:?}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        match &request {
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                match contract_addr.as_str() {
                    "factory" => match from_binary(&msg).unwrap() {
                        FeeInfo { .. } => SystemResult::Ok(
                            to_binary(&FeeInfoResponse {
                                fee_address: Some(Addr::unchecked("fee_address")),
                                total_fee_bps: 30,
                                maker_fee_bps: 1660,
                            })
                            .into(),
                        ),
                        Config { .. } => SystemResult::Ok(
                            to_binary(&ConfigResponse {
                                owner: Addr::unchecked("owner"),
                                pair_configs: vec![],
                                token_code_id: 0,
                                fee_address: Some(Addr::unchecked("fee_address")),
                                generator_address: Some(Addr::unchecked("gen_address")),
                                whitelist_code_id: 666,
                            })
                            .into(),
                        ),
                        _ => panic!("DO NOT ENTER HERE"),
                    },
                    "minter_address" => match from_binary(&msg).unwrap() {
                        Pair {} => SystemResult::Ok(
                            to_binary(&PairInfo {
                                asset_infos: [
                                    AssetInfo::Token {
                                        contract_addr: Addr::unchecked("token1"),
                                    },
                                    AssetInfo::Token {
                                        contract_addr: Addr::unchecked("token2"),
                                    },
                                ],
                                contract_addr: Addr::unchecked(contract_addr.as_str()),
                                liquidity_token: Addr::unchecked("liquidity_token"),
                                pair_type: PairType::Stable {},
                            })
                            .into(),
                        ),
                        _ => panic!("DO NOT ENTER HERE"),
                    },
                    _ => match from_binary(&msg).unwrap() {
                        Cw20QueryMsg::TokenInfo {} => {
                            let balances: &HashMap<String, Uint128> =
                                match self.token_querier.balances.get(contract_addr) {
                                    Some(balances) => balances,
                                    None => {
                                        return SystemResult::Err(SystemError::Unknown {});
                                    }
                                };

                            let mut total_supply = Uint128::zero();

                            for balance in balances {
                                total_supply += *balance.1;
                            }

                            SystemResult::Ok(
                                to_binary(&TokenInfoResponse {
                                    name: "mAPPL".to_string(),
                                    symbol: "mAPPL".to_string(),
                                    decimals: 6,
                                    total_supply: total_supply,
                                })
                                .into(),
                            )
                        }
                        Cw20QueryMsg::Balance { address } => {
                            let balances: &HashMap<String, Uint128> =
                                match self.token_querier.balances.get(contract_addr) {
                                    Some(balances) => balances,
                                    None => {
                                        return SystemResult::Err(SystemError::Unknown {});
                                    }
                                };

                            let balance = match balances.get(&address) {
                                Some(v) => v,
                                None => {
                                    return SystemResult::Err(SystemError::Unknown {});
                                }
                            };

                            SystemResult::Ok(
                                to_binary(&BalanceResponse { balance: *balance }).into(),
                            )
                        }
                        Cw20QueryMsg::Minter {} => SystemResult::Ok(
                            to_binary(&MinterResponse {
                                minter: "minter_address".to_string(),
                                cap: None,
                            })
                            .into(),
                        ),
                        _ => panic!("DO NOT ENTER HERE"),
                    },
                }
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<Empty>) -> Self {
        WasmMockQuerier {
            base,
            token_querier: TokenQuerier::default(),
        }
    }

    pub fn with_balance(&mut self, balances: &[(&String, &[Coin])]) {
        for (addr, balance) in balances {
            self.base.update_balance(addr.to_string(), balance.to_vec());
        }
    }
}
