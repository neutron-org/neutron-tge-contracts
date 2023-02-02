pub mod airdrop;
pub mod auction;
pub mod helpers;
pub mod lockdrop;
pub mod simple_airdrop;
pub mod utils;

use cw_storage_plus::IntKeyOld;

pub type U64Key = IntKeyOld<u64>;
