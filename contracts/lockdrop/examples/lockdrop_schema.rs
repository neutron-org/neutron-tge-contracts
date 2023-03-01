use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

use astroport_periphery::lockdrop::{
    Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, LockUpInfoResponse, MigrateMsg, PoolInfo,
    QueryMsg, StateResponse, UpdateConfigMsg, UserInfoResponse,
};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(Cw20HookMsg), &out_dir);
    export_schema(&schema_for!(UpdateConfigMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(MigrateMsg), &out_dir);

    export_schema(&schema_for!(Config), &out_dir);
    export_schema(&schema_for!(StateResponse), &out_dir);
    export_schema(&schema_for!(PoolInfo), &out_dir);
    export_schema(&schema_for!(UserInfoResponse), &out_dir);
    export_schema(&schema_for!(LockUpInfoResponse), &out_dir);
}
