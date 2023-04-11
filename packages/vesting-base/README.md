# Neutron Vesting Base

This library contains basis for configuration and initialisation of vesting contracts. It also contains data models and handlers for interaction with vesting contracts.

## Usage

1. To use the library for initialisation of a simple vesting contract just build a default vesting base in its instantiate message:
```rust
use vesting_base::builder::VestingBaseBuilder;

pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    ...
    VestingBaseBuilder::default().build(deps, msg.owner, msg.vesting_token)?;
    ...
```

Read about more advanced building in the [Extensions](#extensions) section.

2. Simply pass the execute and query requests to the vesting base's execute and query handlers:
```rust
use vesting_base::handlers::{execute as base_execute, query as base_query};

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    base_execute(deps, env, info, msg)
}

pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    base_query(deps, env, msg)
}
```

### Messages

The default version exposes the following messages:

#### ExecuteMsg

```rust
/// This structure describes the execute messages available in a vesting contract.
#[cw_serde]
pub enum ExecuteMsg {
    /// Claim claims vested tokens and sends them to a recipient
    Claim {
        /// The address that receives the vested tokens
        recipient: Option<String>,
        /// The amount of tokens to claim
        amount: Option<Uint128>,
    },
    /// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template
    Receive(Cw20ReceiveMsg),
    /// RegisterVestingAccounts registers vesting targets/accounts
    RegisterVestingAccounts {
        vesting_accounts: Vec<VestingAccount>,
    },
    /// Creates a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the offer to change the owner
        expires_in: u64,
    },
    /// Removes a request to change contract ownership
    /// ## Executor
    /// Only the current owner can execute this
    DropOwnershipProposal {},
    /// Claims contract ownership
    /// ## Executor
    /// Only the newly proposed owner can execute this
    ClaimOwnership {},
    /// Sets vesting token
    /// ## Executor
    /// Only the current owner or token info manager can execute this
    SetVestingToken { vesting_token: AssetInfo },
    /// Contains messages associated with the managed extension for vesting contracts.
    ManagedExtension { msg: ExecuteMsgManaged },
    /// Contains messages associated with the with_managers extension for vesting contracts.
    WithManagersExtension { msg: ExecuteMsgWithManagers },
    /// Contains messages associated with the historical extension for vesting contracts.
    HistoricalExtension { msg: ExecuteMsgHistorical },
}
```

The `ManagedExtension`, `WithManagersExtension`, and `HistoricalExtension` messages are extensiom messages. Read about them in the [Extensions](#extensions) section.

#### QueryMsg

```rust
/// This structure describes the query messages available in a vesting contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the configuration for the contract using a [`ConfigResponse`] object.
    #[returns(ConfigResponse)]
    Config {},
    /// Returns information about an address vesting tokens using a [`VestingAccountResponse`] object.
    #[returns(VestingAccountResponse)]
    VestingAccount { address: String },
    /// Returns a list of addresses that are vesting tokens using a [`VestingAccountsResponse`] object.
    #[returns(VestingAccountsResponse)]
    VestingAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    },
    /// Returns the total unvested amount of tokens for a specific address.
    #[returns(Uint128)]
    AvailableAmount { address: String },
    /// Timestamp returns the current timestamp
    #[returns(u64)]
    Timestamp {},
    /// VestingState returns the current vesting state.
    #[returns(VestingState)]
    VestingState {},
    /// Contains messages associated with the managed extension for vesting contracts.
    #[returns(QueryMsgManaged)]
    ManagedExtension { msg: QueryMsgManaged },
    /// Contains messages associated with the with_managers extension for vesting contracts.
    #[returns(QueryMsgWithManagers)]
    WithManagersExtension { msg: QueryMsgWithManagers },
    /// Contains messages associated with the historical extension for vesting contracts.
    #[returns(QueryMsgHistorical)]
    HistoricalExtension { msg: QueryMsgHistorical },
}
```

The `ManagedExtension`, `WithManagersExtension`, and `HistoricalExtension` messages are extensiom messages. Read about them in the [Extensions](#extensions) section.

## Extensions

Created contracts can be extended with a number of features.

### Managed

The `managed` extension allows the owner of the vesting contract to remove registered vesting accounts and redeem the corresponding funds.

```rust
/// This structure describes the execute messages available in a managed vesting contract.
#[cw_serde]
pub enum ExecuteMsgManaged {
    /// Removes vesting targets/accounts.
    /// ## Executor
    /// Only the current owner can execute this
    RemoveVestingAccounts {
        vesting_accounts: Vec<String>,
        /// Specifies the account that will receive the funds taken from the vesting accounts.
        clawback_account: String,
    },
}

/// This structure describes the query messages available in a managed vesting contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsgManaged {}
```

### WithManagers

The `with_managers` extension allows the owner of the vesting contract to add/remove vesting managers — addresses that just like the owner are capable of registering new vesting accounts.

```rust
/// This structure describes the execute messages available in a with_managers vesting contract.
#[cw_serde]
pub enum ExecuteMsgWithManagers {
    /// Adds vesting managers
    /// ## Executor
    /// Only the current owner can execute this
    AddVestingManagers { managers: Vec<String> },
    /// Removes vesting managers
    /// ## Executor
    /// Only the current owner can execute this
    RemoveVestingManagers { managers: Vec<String> },
}

/// This structure describes the query messages available in a with_managers vesting contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsgWithManagers {
    /// Returns list of vesting managers
    /// (the persons who are able to add/remove vesting schedules)
    #[returns(Vec<Addr>)]
    VestingManagers {},
}
```

### Historical

The `historical` allows to query vesting accounts and total vesting state based on a given height.

```rust
/// This structure describes the execute messages available in a historical vesting contract.
#[cw_serde]
pub enum ExecuteMsgHistorical {}

/// This structure describes the query messages available in a historical vesting contract.
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsgHistorical {
    /// Returns the total unclaimed amount of tokens for a specific address at certain height.
    #[returns(Uint128)]
    UnclaimedAmountAtHeight { address: String, height: u64 },
    /// Returns the total unclaimed amount of tokens for all the users at certain height.
    #[returns(Uint128)]
    UnclaimedTotalAmountAtHeight { height: u64 },
}
```

### Extensions usage

The following example adds all three extensions to the contract, but it's allowed to combine them in any way.
```rust
use vesting_base::builder::VestingBaseBuilder;
use astroport::asset::AssetInfo;
use cosmwasm_schema::cw_serde;

/// This structure describes the parameters used for creating a contract.
#[cw_serde]
pub struct InstantiateMsg {
    /// Address allowed to change contract parameters
    pub owner: String,
    /// [`AssetInfo`] of the token that's being vested
    pub vesting_token: AssetInfo,
    /// Initial list of whitelisted vesting managers
    pub vesting_managers: Vec<String>,
}

pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    ...
    VestingBaseBuilder::default()
        .historical()
        .managed()
        .with_managers(msg.vesting_managers)
        .build(deps, msg.owner, msg.vesting_token)?;
    ...
```
