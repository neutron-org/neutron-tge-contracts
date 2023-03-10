use astroport_periphery::auction::{Config, State, UserInfo};
use cosmwasm_std::Addr;
use cw_storage_plus::{Index, IndexList, IndexedMap, Item, MultiIndex};

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");

pub struct UserIndexes<'a> {
    pub vested: MultiIndex<'a, u8, UserInfo, Addr>,
}

impl<'a> IndexList<UserInfo> for UserIndexes<'a> {
    fn get_indexes(&'_ self) -> Box<dyn Iterator<Item = &'_ dyn Index<UserInfo>> + '_> {
        let v: Vec<&dyn Index<UserInfo>> = vec![&self.vested];
        Box::new(v.into_iter())
    }
}

pub fn get_users_store<'a>() -> IndexedMap<'a, &'a Addr, UserInfo, UserIndexes<'a>> {
    let indexes = UserIndexes {
        vested: MultiIndex::new(|_, v| v.is_vested.into(), "users", "users__vested"),
    };
    IndexedMap::new("users", indexes)
}
