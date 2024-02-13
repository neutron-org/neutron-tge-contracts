use cosmwasm_schema::write_api;
use vesting_lp::msg::{ExecuteMsg, MigrateMsg, InstantiateMsg};
use vesting_base::msg:: QueryMsg;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg
    }
}
