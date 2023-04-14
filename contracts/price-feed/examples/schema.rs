use astroport_periphery::pricefeed::{ExecuteMsg, InstantiateMsg, QueryMsg};
use cosmwasm_schema::write_api;
use cosmwasm_std::Empty;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: Empty
    }
}
