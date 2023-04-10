use cosmwasm_schema::write_api;
use vesting_base::msg::{ExecuteMsg, MigrateMsg, QueryMsg};
use vesting_managed::msg::InstantiateMsg;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg
    }
}
