use cosmwasm_std::{
    attr, from_binary, to_binary, Addr, Attribute, Binary, Deps, DepsMut, Env, MessageInfo, Order,
    Response, StdError, StdResult, SubMsg, Uint128,
};

use crate::state::{BaseVesting, Config};

use crate::error::ContractError;
use astroport::asset::{addr_opt_validate, token_asset_info, AssetInfo, AssetInfoExt};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::vesting::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, OrderBy, QueryMsg, VestingAccount,
    VestingAccountResponse, VestingAccountsResponse, VestingInfo, VestingSchedule,
};
use cw2::set_contract_version;
use cw20::Cw20ReceiveMsg;
use cw_utils::must_pay;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "neutron-vesting";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

impl BaseVesting {
    /// Creates a new contract with the specified parameters in [`InstantiateMsg`].
    pub fn instantiate(
        &self,
        deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        msg: InstantiateMsg,
    ) -> StdResult<Response> {
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

        self.config.save(
            deps.storage,
            &Config {
                owner: deps.api.addr_validate(&msg.owner)?,
                token_info_manager: deps.api.addr_validate(&msg.token_info_manager)?,
                vesting_token: None,
            },
        )?;

        for m in msg.vesting_managers {
            let ma = deps.api.addr_validate(&m)?;
            self.vesting_managers.save(deps.storage, &ma, &())?;
        }

        Ok(Response::new())
    }

    /// Exposes execute functions available in the contract.
    ///
    /// ## Variants
    /// * **ExecuteMsg::Claim { recipient, amount }** Claims vested tokens and transfers them to the vesting recipient.
    ///
    /// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes it
    /// depending on the received template.
    pub fn execute(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: ExecuteMsg,
    ) -> Result<Response, ContractError> {
        match msg {
            ExecuteMsg::Claim { recipient, amount } => {
                self.claim(deps, env, info, recipient, amount)
            }
            ExecuteMsg::Receive(msg) => self.receive_cw20(deps, env, info, msg),
            ExecuteMsg::RegisterVestingAccounts { vesting_accounts } => {
                let config = self.config.load(deps.storage)?;
                let vesting_token = get_vesting_token(&config)?;

                match &vesting_token {
                    AssetInfo::NativeToken { denom }
                        if self.is_sender_whitelisted(deps.as_ref(), &config, &info.sender) =>
                    {
                        let amount = must_pay(&info, denom)?;
                        self.register_vesting_accounts(
                            deps,
                            vesting_accounts,
                            amount,
                            env.block.height,
                        )
                    }
                    _ => Err(ContractError::Unauthorized {}),
                }
            }
            ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
                let config: Config = self.config.load(deps.storage)?;

                propose_new_owner(
                    deps,
                    info,
                    env,
                    owner,
                    expires_in,
                    config.owner,
                    &self.ownership_proposal,
                )
                .map_err(Into::into)
            }
            ExecuteMsg::DropOwnershipProposal {} => {
                let config: Config = self.config.load(deps.storage)?;

                drop_ownership_proposal(deps, info, config.owner, &self.ownership_proposal)
                    .map_err(Into::into)
            }
            ExecuteMsg::ClaimOwnership {} => claim_ownership(
                deps,
                info,
                env,
                &self.ownership_proposal,
                |deps, new_owner| {
                    self.config.update::<_, StdError>(deps.storage, |mut v| {
                        v.owner = new_owner;
                        Ok(v)
                    })?;

                    Ok(())
                },
            )
            .map_err(Into::into),
            ExecuteMsg::AddVestingManagers { managers } => {
                self.add_vesting_managers(deps, env, info, managers)
            }
            ExecuteMsg::RemoveVestingManagers { managers } => {
                self.remove_vesting_managers(deps, env, info, managers)
            }
            ExecuteMsg::SetVestingToken { vesting_token } => {
                self.set_vesting_token(deps, env, info, vesting_token)
            }
        }
    }

    fn is_sender_whitelisted(&self, deps: Deps, config: &Config, sender: &Addr) -> bool {
        if *sender == config.owner {
            return true;
        }
        if self.vesting_managers.has(deps.storage, sender) {
            return true;
        }
        false
    }

    /// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
    ///
    /// * **cw20_msg** CW20 message to process.
    pub fn receive_cw20(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        cw20_msg: Cw20ReceiveMsg,
    ) -> Result<Response, ContractError> {
        let config = self.config.load(deps.storage)?;
        let vesting_token = get_vesting_token(&config)?;

        // Permission check
        if !self.is_sender_whitelisted(
            deps.as_ref(),
            &config,
            &deps.api.addr_validate(&cw20_msg.sender)?,
        ) || token_asset_info(info.sender) != vesting_token
        {
            return Err(ContractError::Unauthorized {});
        }

        match from_binary(&cw20_msg.msg)? {
            Cw20HookMsg::RegisterVestingAccounts { vesting_accounts } => self
                .register_vesting_accounts(
                    deps,
                    vesting_accounts,
                    cw20_msg.amount,
                    env.block.height,
                ),
        }
    }

    pub fn set_vesting_token(
        &self,
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        token: AssetInfo,
    ) -> Result<Response, ContractError> {
        let mut config = self.config.load(deps.storage)?;
        if info.sender != config.owner && info.sender != config.token_info_manager {
            return Err(ContractError::Unauthorized {});
        }
        token.check(deps.api)?;
        config.vesting_token = Some(token);

        self.config.save(deps.storage, &config)?;
        Ok(Response::new())
    }

    /// Adds new vesting managers, which have a permission to add/remove vesting schedule
    ///
    /// * **managers** list of accounts to be added to the whitelist.
    pub fn add_vesting_managers(
        &self,
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        managers: Vec<String>,
    ) -> Result<Response, ContractError> {
        let config = self.config.load(deps.storage)?;
        if info.sender != config.owner {
            return Err(ContractError::Unauthorized {});
        }
        let mut attrs: Vec<Attribute> = vec![];
        for m in managers {
            let ma = deps.api.addr_validate(&m)?;
            if !self.vesting_managers.has(deps.storage, &ma) {
                self.vesting_managers.save(deps.storage, &ma, &())?;
                attrs.push(attr("vesting_manager", &m))
            }
        }
        Ok(Response::new()
            .add_attribute("action", "add_vesting_managers")
            .add_attributes(attrs))
    }

    /// Removes new vesting managers from the whitelist
    ///
    /// * **managers** list of accounts to be removed from the whitelist.
    pub fn remove_vesting_managers(
        &self,
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        managers: Vec<String>,
    ) -> Result<Response, ContractError> {
        let config = self.config.load(deps.storage)?;
        if info.sender != config.owner {
            return Err(ContractError::Unauthorized {});
        }
        let mut attrs: Vec<Attribute> = vec![];
        for m in managers {
            let ma = deps.api.addr_validate(&m)?;
            if self.vesting_managers.has(deps.storage, &ma) {
                self.vesting_managers.remove(deps.storage, &ma);
                attrs.push(attr("vesting_manager", &m))
            }
        }
        Ok(Response::new()
            .add_attribute("action", "remove_vesting_managers")
            .add_attributes(attrs))
    }

    /// Create new vesting schedules.
    ///
    /// * **vesting_accounts** list of accounts and associated vesting schedules to create.
    ///
    /// * **cw20_amount** sets the amount that confirms the total amount of all accounts to register.
    pub fn register_vesting_accounts(
        &self,
        deps: DepsMut,
        vesting_accounts: Vec<VestingAccount>,
        amount: Uint128,
        height: u64,
    ) -> Result<Response, ContractError> {
        let response = Response::new();

        let mut to_deposit = Uint128::zero();

        for mut vesting_account in vesting_accounts {
            let mut released_amount = Uint128::zero();
            let account_address = deps.api.addr_validate(&vesting_account.address)?;

            assert_vesting_schedules(&account_address, &vesting_account.schedules)?;

            for sch in &vesting_account.schedules {
                let amount = if let Some(end_point) = &sch.end_point {
                    end_point.amount
                } else {
                    sch.start_point.amount
                };
                to_deposit = to_deposit.checked_add(amount)?;
            }

            if let Some(mut old_info) =
                self.vesting_info.may_load(deps.storage, &account_address)?
            {
                released_amount = old_info.released_amount;
                vesting_account.schedules.append(&mut old_info.schedules);
            }

            self.vesting_info.save(
                deps.storage,
                &account_address,
                &VestingInfo {
                    schedules: vesting_account.schedules,
                    released_amount,
                },
                height,
            )?;
        }

        if to_deposit != amount {
            return Err(ContractError::VestingScheduleAmountError {});
        }

        self.vesting_state
            .update::<_, ContractError>(deps.storage, height, |s| {
                let mut state = s.unwrap_or_default();
                state.total_granted = state.total_granted.checked_add(to_deposit)?;
                Ok(state)
            })?;

        Ok(response.add_attributes({
            vec![
                attr("action", "register_vesting_accounts"),
                attr("deposited", to_deposit),
            ]
        }))
    }

    /// Claims vested tokens and transfers them to the vesting recipient.
    ///
    /// * **recipient** vesting recipient for which to claim tokens.
    ///
    /// * **amount** amount of vested tokens to claim.
    pub fn claim(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        recipient: Option<String>,
        amount: Option<Uint128>,
    ) -> Result<Response, ContractError> {
        let config = self.config.load(deps.storage)?;
        let vesting_token = get_vesting_token(&config)?;
        let mut vesting_info = self.vesting_info.load(deps.storage, &info.sender)?;

        let available_amount = compute_available_amount(env.block.time.seconds(), &vesting_info)?;

        let claim_amount = if let Some(a) = amount {
            if a > available_amount {
                return Err(ContractError::AmountIsNotAvailable {});
            };
            a
        } else {
            available_amount
        };

        let mut response = Response::new();

        if !claim_amount.is_zero() {
            let transfer_msg = vesting_token.with_balance(claim_amount).into_msg(
                &deps.querier,
                recipient.unwrap_or_else(|| info.sender.to_string()),
            )?;
            response = response.add_submessage(SubMsg::new(transfer_msg));

            vesting_info.released_amount =
                vesting_info.released_amount.checked_add(claim_amount)?;
            self.vesting_info
                .save(deps.storage, &info.sender, &vesting_info, env.block.height)?;
            self.vesting_state
                .update::<_, ContractError>(deps.storage, env.block.height, |s| {
                    let mut state = s.ok_or(ContractError::AmountIsNotAvailable {})?;
                    state.total_released = state.total_released.checked_add(claim_amount)?;
                    Ok(state)
                })?;
        };

        Ok(response.add_attributes(vec![
            attr("action", "claim"),
            attr("address", &info.sender),
            attr("available_amount", available_amount),
            attr("claimed_amount", claim_amount),
        ]))
    }

    /// Exposes all the queries available in the contract.
    ///
    /// ## Queries
    /// * **QueryMsg::Config {}** Returns the contract configuration in an object of type [`Config`].
    ///
    /// * **QueryMsg::VestingAccount { address }** Returns information about the vesting schedules that have a specific vesting recipient.
    ///
    /// * **QueryMsg::VestingAccounts {
    ///             start_after,
    ///             limit,
    ///             order_by,
    ///         }** Returns a list of vesting schedules together with their vesting recipients.
    ///
    /// * **QueryMsg::AvailableAmount { address }** Returns the available amount of tokens that can be claimed by a specific vesting recipient.
    pub fn query(&self, deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
        match msg {
            QueryMsg::Config {} => Ok(to_binary(&self.query_config(deps)?)?),
            QueryMsg::VestingAccount { address } => {
                Ok(to_binary(&self.query_vesting_account(deps, address)?)?)
            }
            QueryMsg::VestingAccounts {
                start_after,
                limit,
                order_by,
            } => Ok(to_binary(&self.query_vesting_accounts(
                deps,
                start_after,
                limit,
                order_by,
            )?)?),
            QueryMsg::AvailableAmount { address } => Ok(to_binary(
                &self.query_vesting_available_amount(deps, env, address)?,
            )?),
            QueryMsg::Timestamp {} => Ok(to_binary(&self.query_timestamp(env)?)?),
            QueryMsg::VestingState {} => Ok(to_binary(&self.vesting_state.load(deps.storage)?)?),
            QueryMsg::VestingManagers {} => Ok(to_binary(&self.query_vesting_managers(deps)?)?),
        }
    }

    /// Returns the vesting contract configuration using a [`Config`] object.
    pub fn query_config(&self, deps: Deps) -> StdResult<Config> {
        let config = self.config.load(deps.storage)?;
        Ok(config)
    }

    /// Return the current block timestamp (in seconds)
    /// * **env** is an object of type [`Env`].
    pub fn query_timestamp(&self, env: Env) -> StdResult<u64> {
        Ok(env.block.time.seconds())
    }

    /// Returns a list of vesting schedules using a [`VestingAccountsResponse`] object.
    pub fn query_vesting_managers(&self, deps: Deps) -> StdResult<Vec<Addr>> {
        let managers = self
            .vesting_managers
            .keys(deps.storage, None, None, Order::Ascending)
            .collect::<Result<Vec<Addr>, StdError>>()?;
        Ok(managers)
    }

    /// Returns the vesting data for a specific vesting recipient using a [`VestingAccountResponse`] object.
    ///
    /// * **address** vesting recipient for which to return vesting data.
    pub fn query_vesting_account(
        &self,
        deps: Deps,
        address: String,
    ) -> StdResult<VestingAccountResponse> {
        let address = deps.api.addr_validate(&address)?;
        let info = self.vesting_info.load(deps.storage, &address)?;

        Ok(VestingAccountResponse { address, info })
    }

    /// Returns a list of vesting schedules using a [`VestingAccountsResponse`] object.
    ///
    /// * **start_after** index from which to start reading vesting schedules.
    ///
    /// * **limit** amount of vesting schedules to return.
    ///
    /// * **order_by** whether results should be returned in an ascending or descending order.
    pub fn query_vesting_accounts(
        &self,
        deps: Deps,
        start_after: Option<String>,
        limit: Option<u32>,
        order_by: Option<OrderBy>,
    ) -> StdResult<VestingAccountsResponse> {
        let start_after = addr_opt_validate(deps.api, &start_after)?;

        let vesting_infos = self.read_vesting_infos(deps, start_after, limit, order_by)?;

        let vesting_accounts: Vec<_> = vesting_infos
            .into_iter()
            .map(|(address, info)| VestingAccountResponse { address, info })
            .collect();

        Ok(VestingAccountsResponse { vesting_accounts })
    }

    /// Returns the available amount of vested and yet to be claimed tokens for a specific vesting recipient.
    ///
    /// * **address** vesting recipient for which to return the available amount of tokens to claim.
    pub fn query_vesting_available_amount(
        &self,
        deps: Deps,
        env: Env,
        address: String,
    ) -> StdResult<Uint128> {
        let address = deps.api.addr_validate(&address)?;

        let info = self.vesting_info.load(deps.storage, &address)?;
        let available_amount = compute_available_amount(env.block.time.seconds(), &info)?;
        Ok(available_amount)
    }

    /// Manages contract migration.
    pub fn migrate(
        &self,
        _deps: DepsMut,
        _env: Env,
        _msg: MigrateMsg,
    ) -> Result<Response, ContractError> {
        Ok(Response::default())
    }
}

/// Asserts the validity of a list of vesting schedules.
///
/// * **addr** receiver of the vested tokens.
///
/// * **vesting_schedules** vesting schedules to validate.
fn assert_vesting_schedules(
    addr: &Addr,
    vesting_schedules: &[VestingSchedule],
) -> Result<(), ContractError> {
    for sch in vesting_schedules {
        if let Some(end_point) = &sch.end_point {
            if !(sch.start_point.time < end_point.time && sch.start_point.amount < end_point.amount)
            {
                return Err(ContractError::VestingScheduleError(addr.to_string()));
            }
        }
    }

    Ok(())
}

/// Computes the amount of vested and yet unclaimed tokens for a specific vesting recipient.
/// Returns the computed amount if the operation is successful.
///
/// * **current_time** timestamp from which to start querying for vesting schedules.
/// Schedules that started later than current_time will be omitted.
///
/// * **vesting_info** vesting schedules for which to compute the amount of tokens
/// that are vested and can be claimed by the recipient.
fn compute_available_amount(current_time: u64, vesting_info: &VestingInfo) -> StdResult<Uint128> {
    let mut available_amount: Uint128 = Uint128::zero();
    for sch in &vesting_info.schedules {
        if sch.start_point.time > current_time {
            continue;
        }

        available_amount = available_amount.checked_add(sch.start_point.amount)?;

        if let Some(end_point) = &sch.end_point {
            let passed_time = current_time.min(end_point.time) - sch.start_point.time;
            let time_period = end_point.time - sch.start_point.time;
            if passed_time != 0 && time_period != 0 {
                let release_amount = Uint128::from(passed_time).multiply_ratio(
                    end_point.amount.checked_sub(sch.start_point.amount)?,
                    time_period,
                );
                available_amount = available_amount.checked_add(release_amount)?;
            }
        }
    }

    available_amount
        .checked_sub(vesting_info.released_amount)
        .map_err(StdError::from)
}

fn get_vesting_token(config: &Config) -> Result<AssetInfo, ContractError> {
    config
        .vesting_token
        .clone()
        .ok_or(ContractError::VestingTokenIsNotSet {})
}
