use crate::state::{CONFIG, VESTING_MANAGERS};
use crate::types::{Config, Extensions};
use cosmwasm_std::{DepsMut, StdResult};
use astroport::asset::AssetInfo;

/// A builder for vesting contracts with different extensions.
#[derive(Default)]
pub struct VestingBaseBuilder {
    vesting_managers: Vec<String>,
    historical: bool,
    managed: bool,
    with_managers: bool,
}

impl VestingBaseBuilder {
    /// Appends the `managed` extension to the created vesting contract.
    pub fn managed(&mut self) -> &mut VestingBaseBuilder {
        self.managed = true;
        self
    }

    /// Appends the `with_managers` extension to the created vesting contract.
    pub fn with_managers(&mut self, managers: Vec<String>) -> &mut VestingBaseBuilder {
        self.vesting_managers.extend(managers);
        self.with_managers = true;
        self
    }

    /// Appends the `historical` extension to the created vesting contract.
    pub fn historical(&mut self) -> &mut VestingBaseBuilder {
        self.historical = true;
        self
    }

    /// Validates the inputs and initialises the created contract state.
    pub fn build(
        &self,
        deps: DepsMut,
        owner: String,
        token_info_manager: String,
        xyk_vesting_lp_contract: String,
        vesting_token: AssetInfo,
    ) -> StdResult<()> {
        let owner = deps.api.addr_validate(&owner)?;
        CONFIG.save(
            deps.storage,
            &Config {
                owner,
                vesting_token: Option::from(vesting_token),
                token_info_manager: deps.api.addr_validate(&token_info_manager)?,
                extensions: Extensions {
                    historical: self.historical,
                    managed: self.managed,
                    with_managers: self.with_managers,
                },
                xyk_vesting_lp_contract: deps.api.addr_validate(&xyk_vesting_lp_contract)?,
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
