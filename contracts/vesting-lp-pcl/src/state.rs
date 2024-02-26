use cosmwasm_std::Addr;
use cw_storage_plus::Item;
pub(crate) const XYK_VESTING_LP_CONTRACT: Item<Addr> = Item::new("xyk_vesting_lp_contract");
