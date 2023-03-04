use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};
use cw20::{
    AllAccountsResponse, AllAllowancesResponse, AllowanceResponse, BalanceResponse, MinterResponse,
    TokenInfoResponse,
};

use credits::msg::{
    ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, TotalSupplyResponse, VestedAmountResponse,
    WithdrawableAmountResponse,
};
use credits::state::{Allocation, Config, Schedule};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(MigrateMsg), &out_dir);

    export_schema(&schema_for!(Config), &out_dir);
    export_schema(&schema_for!(Allocation), &out_dir);
    export_schema(&schema_for!(Schedule), &out_dir);

    export_schema(&schema_for!(TotalSupplyResponse), &out_dir);
    export_schema(&schema_for!(WithdrawableAmountResponse), &out_dir);
    export_schema(&schema_for!(VestedAmountResponse), &out_dir);

    export_schema(&schema_for!(BalanceResponse), &out_dir);
    export_schema(&schema_for!(TokenInfoResponse), &out_dir);
    export_schema(&schema_for!(MinterResponse), &out_dir);
    export_schema(&schema_for!(AllowanceResponse), &out_dir);
    export_schema(&schema_for!(AllAllowancesResponse), &out_dir);
    export_schema(&schema_for!(AllAccountsResponse), &out_dir);
}