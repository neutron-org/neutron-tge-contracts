use cosmwasm_schema::write_api;
use vesting_base::msg::{MigrateMsg, QueryMsg};
use vesting_lp_pcl::msg::{ExecuteMsg, InstantiateMsg};

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsg,
        migrate: MigrateMsg
    }
}
