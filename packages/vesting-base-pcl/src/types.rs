use astroport::asset::AssetInfo;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Order, Uint128};

/// This structure stores the main parameters for the generator vesting contract.
#[cw_serde]
pub struct Config {
    /// Address that's allowed to change contract parameters
    pub owner: Addr,
    /// [`AssetInfo`] of the vested token
    pub vesting_token: Option<AssetInfo>,
    /// Address that's allowed to change vesting token
    pub token_info_manager: Addr,
    /// Contains extensions information of the contract
    pub extensions: Extensions,
    pub xyk_vesting_lp_contract: Addr,
}

/// Contains extensions information for the contract.
#[cw_serde]
pub struct Extensions {
    /// Whether the historical extension is enabled for the contract.
    pub historical: bool,
    /// Whether the managed extension is enabled for the contract.
    pub managed: bool,
    /// Whether the with_managers extension is enabled for the contract.
    pub with_managers: bool,
}

/// This structure stores the accumulated vesting information for all addresses.
#[cw_serde]
#[derive(Default)]
pub struct VestingState {
    /// The total amount of tokens granted to the users
    pub total_granted: Uint128,
    /// The total amount of tokens already claimed
    pub total_released: Uint128,
}

/// This structure stores vesting information for a specific address that is getting tokens.
#[cw_serde]
pub struct VestingAccount {
    /// The address that is getting tokens
    pub address: String,
    /// The vesting schedules targeted at the `address`
    pub schedules: Vec<VestingSchedule>,
}

/// This structure stores parameters for a batch of vesting schedules.
#[cw_serde]
pub struct VestingInfo {
    /// The vesting schedules
    pub schedules: Vec<VestingSchedule>,
    /// The total amount of vested tokens already claimed
    pub released_amount: Uint128,
}

/// This structure stores parameters for a specific vesting schedule
#[cw_serde]
pub struct VestingSchedule {
    /// The start date for the vesting schedule
    pub start_point: VestingSchedulePoint,
    /// The end point for the vesting schedule
    pub end_point: Option<VestingSchedulePoint>,
}

/// This structure stores the parameters used to create a vesting schedule.
#[cw_serde]
pub struct VestingSchedulePoint {
    /// The start time for the vesting schedule
    pub time: u64,
    /// The amount of tokens being vested
    pub amount: Uint128,
}

/// This structure describes a custom struct used to return vesting data about a specific vesting target.
#[cw_serde]
pub struct VestingAccountResponse {
    /// The address that's vesting tokens
    pub address: Addr,
    /// Vesting information
    pub info: VestingInfo,
}

/// This structure describes a custom struct used to return vesting data for multiple vesting targets.
#[cw_serde]
pub struct VestingAccountsResponse {
    /// A list of accounts that are vesting tokens
    pub vesting_accounts: Vec<VestingAccountResponse>,
}

/// This enum describes the types of sorting that can be applied to some piece of data
#[cw_serde]
pub enum OrderBy {
    Asc,
    Desc,
}

// We suppress this clippy warning because Order in cosmwasm doesn't implement Debug and
// PartialEq for usage in QueryMsg. We need to use our own OrderBy and convert the result to cosmwasm's Order
#[allow(clippy::from_over_into)]
impl Into<Order> for OrderBy {
    fn into(self) -> Order {
        if self == OrderBy::Asc {
            Order::Ascending
        } else {
            Order::Descending
        }
    }
}
