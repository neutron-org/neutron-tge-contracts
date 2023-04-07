use crate::state::{CONFIG, VESTING_MANAGERS};
use crate::types::{Config, Extensions};
use cosmwasm_std::{DepsMut, StdResult};

/// A builder for vesting contracts with different extensions.
#[derive(Default)]
pub struct VestingBaseBuilder {
    vesting_managers: Vec<String>,
    historical: bool,
    managed: bool,
    with_managers: bool,
}

impl VestingBaseBuilder {
    /// Appends a managed extension to the created vesting contract.
    pub fn managed(&mut self) -> &mut VestingBaseBuilder {
        self.managed = true;
        self
    }

    /// Appends a with_managers extension to the created vesting contract.
    pub fn with_managers(&mut self, managers: Vec<String>) -> &mut VestingBaseBuilder {
        self.vesting_managers.extend(managers);
        self.with_managers = true;
        self
    }

    /// Appends a historical extension to the created vesting contract.
    pub fn historical(&mut self) -> &mut VestingBaseBuilder {
        self.historical = true;
        self
    }

    /// Validates the inputs and initialises the created contract state.
    pub fn build(&self, deps: DepsMut, owner: String, token_info_manager: String) -> StdResult<()> {
        let owner = deps.api.addr_validate(&owner)?;
        CONFIG.save(
            deps.storage,
            &Config {
                owner,
                vesting_token: None,
                token_info_manager: deps.api.addr_validate(&token_info_manager)?,
                extensions: Extensions {
                    historical: self.historical,
                    managed: self.managed,
                    with_managers: self.with_managers,
                },
            },
        )?;

        if self.with_managers {
            for m in self.vesting_managers.iter() {
                let ma = deps.api.addr_validate(m)?;
                VESTING_MANAGERS.save(deps.storage, ma, &())?;
            }
        };

        Ok(())
    }
}
