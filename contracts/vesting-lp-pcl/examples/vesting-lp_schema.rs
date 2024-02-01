use cosmwasm_schema::write_api;
use vesting_base_pcl::msg::{ExecuteMsg, MigrateMsg, QueryMsg};
use vesting_lp_pcl::msg::InstantiateMsg;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg
    }
}
