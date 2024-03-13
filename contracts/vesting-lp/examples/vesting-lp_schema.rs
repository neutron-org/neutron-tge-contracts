use cosmwasm_schema::write_api;
use vesting_base::msg::QueryMsg;
use vesting_lp::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg};

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg
    }
}
