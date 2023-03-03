// Migration logic for contracts with version: 0.12.1
pub mod v0_12_1 {
    use crate::state::PAUSED;
    use crate::ContractError;
    use cosmwasm_std::DepsMut;
    pub fn set_initial_pause_status(deps: DepsMut) -> Result<(), ContractError> {
        PAUSED.save(deps.storage, &false)?;
        Ok(())
    }
}
