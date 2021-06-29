use cosmwasm_std::{
    debug_print, from_binary, to_binary, Api, Binary, CosmosMsg, Env, Extern, HandleResponse,
    HumanAddr, InitResponse, Querier, ReadonlyStorage, StdError, StdResult, Storage, Uint128,
    WasmMsg,
};
use cosmwasm_storage::{PrefixedStorage, ReadonlyPrefixedStorage};
use secret_toolkit::crypto::sha_256;
use secret_toolkit::snip20;
use secret_toolkit::storage::{TypedStore, TypedStoreMut};
use secret_toolkit::utils::{pad_handle_result, pad_query_result};

use crate::constants::*;
use crate::querier::query_pending;
use crate::state::Config;
use scrt_finance::lp_staking_msg::LPStakingResponseStatus::Success;
use scrt_finance::lp_staking_msg::{
    LPStakingHandleAnswer, LPStakingHandleMsg, LPStakingHookMsg, LPStakingInitMsg,
    LPStakingQueryAnswer, LPStakingQueryMsg, LPStakingReceiveAnswer, LPStakingReceiveMsg,
};
use scrt_finance::master_msg::MasterHandleMsg;
use scrt_finance::types::{RewardPool, TokenInfo, UserInfo};
use scrt_finance::viewing_key::{ViewingKey, VIEWING_KEY_SIZE};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: LPStakingInitMsg,
) -> StdResult<InitResponse> {
    // Initialize state
    let prng_seed_hashed = sha_256(&msg.prng_seed.0);
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    config_store.store(
        CONFIG_KEY,
        &Config {
            admin: env.message.sender.clone(),
            reward_token: msg.reward_token.clone(),
            inc_token: msg.inc_token.clone(),
            master: msg.master,
            viewing_key: msg.viewing_key.clone(),
            prng_seed: prng_seed_hashed.to_vec(),
            is_stopped: false,
            own_addr: env.contract.address,
        },
    )?;

    TypedStoreMut::<RewardPool, S>::attach(&mut deps.storage).store(
        REWARD_POOL_KEY,
        &RewardPool {
            residue: 0,
            inc_token_supply: 0,
            acc_reward_per_share: 0,
        },
    )?;

    TypedStoreMut::<TokenInfo, S>::attach(&mut deps.storage)
        .store(TOKEN_INFO_KEY, &msg.token_info)?;

    // Register sSCRT and incentivized token, set vks
    let messages = vec![
        snip20::register_receive_msg(
            env.contract_code_hash.clone(),
            None,
            1, // This is public data, no need to pad
            msg.reward_token.contract_hash.clone(),
            msg.reward_token.address.clone(),
        )?,
        snip20::register_receive_msg(
            env.contract_code_hash,
            None,
            1,
            msg.inc_token.contract_hash.clone(),
            msg.inc_token.address.clone(),
        )?,
        snip20::set_viewing_key_msg(
            msg.viewing_key.clone(),
            None,
            RESPONSE_BLOCK_SIZE, // This is private data, need to pad
            msg.reward_token.contract_hash,
            msg.reward_token.address,
        )?,
        snip20::set_viewing_key_msg(
            msg.viewing_key,
            None,
            RESPONSE_BLOCK_SIZE,
            msg.inc_token.contract_hash,
            msg.inc_token.address,
        )?,
    ];

    Ok(InitResponse {
        messages,
        log: vec![],
    })
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: LPStakingHandleMsg,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY)?;
    if config.is_stopped {
        return match msg {
            LPStakingHandleMsg::EmergencyRedeem {} => emergency_redeem(deps, env),
            LPStakingHandleMsg::ResumeContract {} => resume_contract(deps, env),
            _ => Err(StdError::generic_err(
                "this contract is stopped and this action is not allowed",
            )),
        };
    }

    let response = match msg {
        LPStakingHandleMsg::Redeem { amount } => redeem(deps, env, amount),
        LPStakingHandleMsg::Receive {
            from, amount, msg, ..
        } => receive(deps, env, from, amount.u128(), msg),
        LPStakingHandleMsg::CreateViewingKey { entropy, .. } => {
            create_viewing_key(deps, env, entropy)
        }
        LPStakingHandleMsg::SetViewingKey { key, .. } => set_viewing_key(deps, env, key),
        LPStakingHandleMsg::StopContract {} => stop_contract(deps, env),
        LPStakingHandleMsg::ChangeAdmin { address } => change_admin(deps, env, address),
        LPStakingHandleMsg::NotifyAllocation { amount: rewards, hook } => notify_allocation(
            deps,
            env,
            rewards.u128(),
            hook.map(|h| from_binary(&h).unwrap()),
        ),
        _ => Err(StdError::generic_err("Unavailable or unknown action")),
    };

    pad_handle_result(response, RESPONSE_BLOCK_SIZE)
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: LPStakingQueryMsg,
) -> StdResult<Binary> {
    let response = match msg {
        LPStakingQueryMsg::ContractStatus {} => query_contract_status(deps),
        LPStakingQueryMsg::RewardToken {} => query_reward_token(deps),
        LPStakingQueryMsg::IncentivizedToken {} => query_incentivized_token(deps),
        LPStakingQueryMsg::TokenInfo {} => query_token_info(deps),
        _ => authenticated_queries(deps, msg),
    };

    pad_query_result(response, RESPONSE_BLOCK_SIZE)
}

pub fn authenticated_queries<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: LPStakingQueryMsg,
) -> StdResult<Binary> {
    let (address, key) = msg.get_validation_params();

    let vk_store = ReadonlyPrefixedStorage::new(VIEWING_KEY_KEY, &deps.storage);
    let expected_key = vk_store.get(address.0.as_bytes());

    if expected_key.is_none() {
        // Checking the key will take significant time. We don't want to exit immediately if it isn't set
        // in a way which will allow to time the command and determine if a viewing key doesn't exist
        key.check_viewing_key(&[0u8; VIEWING_KEY_SIZE]);
    } else if key.check_viewing_key(expected_key.unwrap().as_slice()) {
        return match msg {
            LPStakingQueryMsg::Rewards {
                address, height, ..
            } => query_pending_rewards(deps, &address, height),
            LPStakingQueryMsg::Balance { address, .. } => query_deposit(deps, &address),
            _ => panic!("This should never happen"),
        };
    }

    Ok(to_binary(&LPStakingQueryAnswer::QueryError {
        msg: "Wrong viewing key for this address or viewing key not set".to_string(),
    })?)
}

// Handle functions

fn receive<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    from: HumanAddr,
    amount: u128,
    msg: Binary,
) -> StdResult<HandleResponse> {
    let msg: LPStakingReceiveMsg = from_binary(&msg)?;

    match msg {
        LPStakingReceiveMsg::Deposit {} => deposit(deps, env, from, amount),
    }
}

fn notify_allocation<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    rewards: u128,
    hook: Option<LPStakingHookMsg>,
) -> StdResult<HandleResponse> {
    let config = TypedStore::<Config, S>::attach(&deps.storage).load(CONFIG_KEY)?;
    if env.message.sender != config.master.address && env.message.sender != config.admin {
        return Err(StdError::generic_err(
            "you are not allowed to call this function",
        ));
    }
    //amount == rewards
    let reward_pool = update_rewards(deps, /*&env, &config,*/ rewards)?;

    let mut response = Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: None,
    });

    if let Some(hook_msg) = hook {
        response = match hook_msg {
            LPStakingHookMsg::Deposit { from, amount } => {
                deposit_hook(deps, env, config, reward_pool, from, amount.u128())
            }
            LPStakingHookMsg::Redeem { to, amount } => {
                redeem_hook(deps, env, config, reward_pool, to, amount)
            }
        }
    }

    response
}

fn deposit<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    from: HumanAddr,
    amount: u128,
) -> StdResult<HandleResponse> {
    // Ensure that the sent tokens are from an expected contract address
    let config = TypedStore::<Config, S>::attach(&deps.storage).load(CONFIG_KEY)?;
    if env.message.sender != config.inc_token.address {
        return Err(StdError::generic_err(format!(
            "This token is not supported. Supported: {}, given: {}",
            config.inc_token.address, env.message.sender
        )));
    }

    update_allocation(
        env,
        config,
        Some(to_binary(&LPStakingHookMsg::Deposit {
            from,
            amount: Uint128(amount),
        })?),
    )
}

fn deposit_hook<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    config: Config,
    mut reward_pool: RewardPool,
    from: HumanAddr,
    amount: u128,
) -> StdResult<HandleResponse> {
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut users_store = TypedStoreMut::<UserInfo, S>::attach(&mut deps.storage);
    let mut user = users_store
        .load(from.0.as_bytes())
        .unwrap_or(UserInfo { locked: 0, debt: 0 }); // NotFound is the only possible error

    if user.locked > 0 {
        //giving the extra change in acc_reward_per_share when the last time deposit was called..
        let pending = user.locked * reward_pool.acc_reward_per_share / REWARD_SCALE - user.debt;
        if pending > 0 {
            messages.push(secret_toolkit::snip20::transfer_msg(
                from.clone(),
                Uint128(pending),
                None,
                RESPONSE_BLOCK_SIZE,
                config.reward_token.contract_hash,
                config.reward_token.address,
            )?);
        }
    }

    user.locked += amount;
    user.debt = user.locked * reward_pool.acc_reward_per_share / REWARD_SCALE;
    users_store.store(from.0.as_bytes(), &user)?;

    reward_pool.inc_token_supply += amount;
    TypedStoreMut::attach(&mut deps.storage).store(REWARD_POOL_KEY, &reward_pool)?;

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: Some(to_binary(&LPStakingReceiveAnswer::Deposit {
            status: Success,
        })?),
    })
}

fn redeem<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Option<Uint128>,
) -> StdResult<HandleResponse> {
    let config = TypedStore::<Config, S>::attach(&deps.storage).load(CONFIG_KEY)?;
    update_allocation(
        env.clone(),
        config,
        Some(to_binary(&LPStakingHookMsg::Redeem {
            to: env.message.sender,
            amount,
        })?),
    )
}

fn redeem_hook<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    config: Config,
    mut reward_pool: RewardPool,
    to: HumanAddr,
    amount: Option<Uint128>,
) -> StdResult<HandleResponse> {
    let mut user = TypedStore::<UserInfo, S>::attach(&deps.storage)
        .load(to.0.as_bytes())
        .unwrap_or(UserInfo { locked: 0, debt: 0 }); // NotFound is the only possible error
    let amount = amount.unwrap_or(Uint128(user.locked)).u128();

    if amount > user.locked {
        return Err(StdError::generic_err(format!(
            "insufficient funds to redeem: balance={}, required={}",
            user.locked, amount,
        )));
    }

    let mut messages: Vec<CosmosMsg> = vec![];
    let pending = user.locked * reward_pool.acc_reward_per_share / REWARD_SCALE - user.debt;
    // debug_print(format!("DEBUG DEBUG DEBUG"));
    // debug_print(format!(
    //     "reward pool: | residue: {} | total supply: {} | acc: {} |",
    //     reward_pool.residue, reward_pool.inc_token_supply, reward_pool.acc_reward_per_share
    // ));
    // debug_print(format!(
    //     "user: | locked: {} | debt: {} |",
    //     user.locked, user.debt
    // ));
    // debug_print(format!("pending: {}", pending));
    // debug_print(format!("DEBUG DEBUG DEBUG"));
    if pending > 0 {
        // Transfer rewards
        messages.push(secret_toolkit::snip20::transfer_msg(
            to.clone(),
            Uint128(pending),
            None,
            RESPONSE_BLOCK_SIZE,
            config.reward_token.contract_hash,
            config.reward_token.address,
        )?);
    }

    // Transfer redeemed tokens
    user.locked -= amount;
    user.debt = user.locked * reward_pool.acc_reward_per_share / REWARD_SCALE;
    TypedStoreMut::<UserInfo, S>::attach(&mut deps.storage).store(to.0.as_bytes(), &user)?;

    reward_pool.inc_token_supply -= amount;
    TypedStoreMut::attach(&mut deps.storage).store(REWARD_POOL_KEY, &reward_pool)?;

    messages.push(secret_toolkit::snip20::transfer_msg(
        to,
        Uint128(amount),
        None,
        RESPONSE_BLOCK_SIZE,
        config.inc_token.contract_hash,
        config.inc_token.address,
    )?);

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: Some(to_binary(&LPStakingHandleAnswer::Redeem {
            status: Success,
        })?),
    })
}

pub fn create_viewing_key<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    entropy: String,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY)?;
    let prng_seed = config.prng_seed;

    let key = ViewingKey::new(&env, &prng_seed, (&entropy).as_ref());

    let mut vk_store = PrefixedStorage::new(VIEWING_KEY_KEY, &mut deps.storage);
    vk_store.set(env.message.sender.0.as_bytes(), &key.to_hashed());

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&LPStakingHandleAnswer::CreateViewingKey { key })?),
    })
}

pub fn set_viewing_key<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    key: String,
) -> StdResult<HandleResponse> {
    let vk = ViewingKey(key);

    let mut vk_store = PrefixedStorage::new(VIEWING_KEY_KEY, &mut deps.storage);
    vk_store.set(env.message.sender.0.as_bytes(), &vk.to_hashed());

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&LPStakingHandleAnswer::SetViewingKey {
            status: Success,
        })?),
    })
}

fn stop_contract<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    let mut config: Config = config_store.load(CONFIG_KEY)?;

    enforce_admin(config.clone(), env)?;

    config.is_stopped = true;
    config_store.store(CONFIG_KEY, &config)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&LPStakingHandleAnswer::StopContract {
            status: Success,
        })?),
    })
}

fn resume_contract<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    let mut config: Config = config_store.load(CONFIG_KEY)?;

    enforce_admin(config.clone(), env)?;

    config.is_stopped = false;
    config_store.store(CONFIG_KEY, &config)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&LPStakingHandleAnswer::ResumeContract {
            status: Success,
        })?),
    })
}

fn change_admin<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    address: HumanAddr,
) -> StdResult<HandleResponse> {
    let mut config_store = TypedStoreMut::attach(&mut deps.storage);
    let mut config: Config = config_store.load(CONFIG_KEY)?;

    enforce_admin(config.clone(), env)?;

    config.admin = address;
    config_store.store(CONFIG_KEY, &config)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&LPStakingHandleAnswer::ChangeAdmin {
            status: Success,
        })?),
    })
}

/// YOU SHOULD NEVER USE THIS! This will erase any eligibility for rewards you earned so far
fn emergency_redeem<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    let mut user: UserInfo = TypedStoreMut::attach(&mut deps.storage)
        .load(env.message.sender.0.as_bytes())
        .unwrap_or(UserInfo { locked: 0, debt: 0 });

    let mut reward_pool: RewardPool =
        TypedStoreMut::attach(&mut deps.storage).load(REWARD_POOL_KEY)?;
    reward_pool.inc_token_supply -= user.locked;
    TypedStoreMut::attach(&mut deps.storage).store(REWARD_POOL_KEY, &reward_pool)?;

    let mut messages = vec![];
    if user.locked > 0 {
        messages.push(secret_toolkit::snip20::transfer_msg(
            env.message.sender.clone(),
            Uint128(user.locked),
            None,
            RESPONSE_BLOCK_SIZE,
            config.inc_token.contract_hash,
            config.inc_token.address,
        )?);
    }

    user = UserInfo { locked: 0, debt: 0 };
    TypedStoreMut::attach(&mut deps.storage).store(env.message.sender.0.as_bytes(), &user)?;

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: Some(to_binary(&LPStakingHandleAnswer::EmergencyRedeem {
            status: Success,
        })?),
    })
}

// Query functions

fn query_pending_rewards<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: &HumanAddr,
    block: u64,
) -> StdResult<Binary> {
    let new_rewards = query_pending(deps, block)?;
    let reward_pool = TypedStore::<RewardPool, S>::attach(&deps.storage).load(REWARD_POOL_KEY)?;
    let user = TypedStore::<UserInfo, S>::attach(&deps.storage)
        .load(address.0.as_bytes())
        .unwrap_or(UserInfo { locked: 0, debt: 0 });
    let mut acc_reward_per_share = reward_pool.acc_reward_per_share;

    if reward_pool.inc_token_supply != 0 {
        acc_reward_per_share +=
            (new_rewards + reward_pool.residue) * REWARD_SCALE / reward_pool.inc_token_supply;
    }

    to_binary(&LPStakingQueryAnswer::Rewards {
        // This is not necessarily accurate, since we don't validate new_rewards. It is up to
        // the UI to display accurate numbers
        rewards: Uint128(user.locked * acc_reward_per_share / REWARD_SCALE - user.debt),
    })
}

fn query_deposit<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: &HumanAddr,
) -> StdResult<Binary> {
    let user = TypedStore::attach(&deps.storage)
        .load(address.0.as_bytes())
        .unwrap_or(UserInfo { locked: 0, debt: 0 });

    to_binary(&LPStakingQueryAnswer::Balance {
        amount: Uint128(user.locked),
    })
}

fn query_contract_status<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Binary> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;

    to_binary(&LPStakingQueryAnswer::ContractStatus {
        is_stopped: config.is_stopped,
    })
}

fn query_reward_token<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Binary> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;

    to_binary(&LPStakingQueryAnswer::RewardToken {
        token: config.reward_token,
    })
}

fn query_incentivized_token<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Binary> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;

    to_binary(&LPStakingQueryAnswer::IncentivizedToken {
        token: config.inc_token,
    })
}

// This is only for Keplr support (Viewing Keys)
fn query_token_info<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Binary> {
    let token_info: TokenInfo = TypedStore::attach(&deps.storage).load(TOKEN_INFO_KEY)?;

    to_binary(&LPStakingQueryAnswer::TokenInfo {
        name: token_info.name,
        symbol: token_info.symbol,
        decimals: 1,
        total_supply: None,
    })
}

// Helper functions

fn enforce_admin(config: Config, env: Env) -> StdResult<()> {
    if config.admin != env.message.sender {
        return Err(StdError::generic_err(format!(
            "not an admin: {}",
            env.message.sender
        )));
    }

    Ok(())
}

fn update_rewards<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    newly_allocated: u128,
) -> StdResult<RewardPool> {
    let mut rewards_store = TypedStoreMut::attach(&mut deps.storage);
    let mut reward_pool: RewardPool = rewards_store.load(REWARD_POOL_KEY)?;

    // If there's no new allocation - there is nothing to update because the state of the pool stays the same
    if newly_allocated <= 0 {
        return Ok(reward_pool);
    }

    if reward_pool.inc_token_supply == 0 {
        reward_pool.residue += newly_allocated;
        rewards_store.store(REWARD_POOL_KEY, &reward_pool)?;
        return Ok(reward_pool);
    }

    // Effectively distributes the residue to the first one that stakes to an empty pool
    reward_pool.acc_reward_per_share +=
        (newly_allocated + reward_pool.residue) * REWARD_SCALE / reward_pool.inc_token_supply;
    reward_pool.residue = 0;
    rewards_store.store(REWARD_POOL_KEY, &reward_pool)?;

    Ok(reward_pool)
}

fn update_allocation(env: Env, config: Config, hook: Option<Binary>) -> StdResult<HandleResponse> {
    Ok(HandleResponse {
        messages: vec![WasmMsg::Execute {
            contract_addr: config.master.address,
            callback_code_hash: config.master.contract_hash,
            msg: to_binary(&MasterHandleMsg::UpdateAllocation {
                spy_addr: env.contract.address,
                spy_hash: env.contract_code_hash,
                hook,
            })?,
            send: vec![],
        }
            .into()],
        log: vec![],
        data: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scrt_finance::lp_staking_msg::LPStakingHandleMsg::{Receive, Redeem, SetViewingKey, CreateViewingKey};
    // use scrt_finance::lp_staking_msg::LPStakingQueryMsg::{Rewards};
    // use scrt_finance::lp_staking_msg::LPStakingReceiveMsg;
    use scrt_finance::types::SecretContract;

    use cosmwasm_std::testing::{
        mock_dependencies, MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{
        coins, from_binary, BlockInfo, Coin, ContractInfo, Empty, MessageInfo, StdError, WasmMsg,
    };
    use rand::Rng;
    use scrt_finance::lp_staking_msg::LPStakingHandleAnswer;
    use serde::{Deserialize, Serialize};

    //helper functions


    fn init_helper() -> (
        StdResult<InitResponse>,
        Extern<MockStorage, MockApi, MockQuerier>,
    ) {
        let mut deps = mock_dependencies(20, &[]);
        let env = mock_env("admin", &[], 1);

        let init_msg = LPStakingInitMsg {
            reward_token: SecretContract {
                address: HumanAddr("sefi".to_string()),
                contract_hash: "reward_token".to_string(),
            },
            inc_token: SecretContract {
                address: HumanAddr("Lp-sScrt-sEth".to_string()),
                contract_hash: "inc_token".to_string(),
            },
            prng_seed: Binary::from("lolz fun yay".as_bytes()),
            viewing_key: "123".to_string(),
            master: SecretContract {
                address: HumanAddr("Master".to_string()),
                contract_hash: "".to_string(),
            },
            token_info: TokenInfo {
                name: "".to_string(),
                symbol: "".to_string(),
            },
        };

        (init(&mut deps, env, init_msg), deps)
    }

    /// Just set sender and sent funds for the message. The rest uses defaults.
    /// The sender will be canonicalized internally to allow developers pasing in human readable senders.
    /// This is intended for use in test code only.
    pub fn mock_env<U: Into<HumanAddr>>(sender: U, sent: &[Coin], height: u64) -> Env {
        Env {
            block: BlockInfo {
                height,
                time: 1_571_797_419,
                chain_id: "secret-testnet".to_string(),
            },
            message: MessageInfo {
                sender: sender.into(),
                sent_funds: sent.to_vec(),
            },
            contract: ContractInfo {
                address: HumanAddr::from(MOCK_CONTRACT_ADDR),
            },
            contract_key: Some("".to_string()),
            contract_code_hash: "".to_string(),
        }
    }

    fn master_update_allocation<S: Storage, A: Api, Q: Querier>(
        deps: &mut Extern<S, A, Q>,
        env: Env,
        spy_address: HumanAddr,
        spy_hash: String,
        hook: Option<Binary>,
    ) -> StdResult<HandleResponse> {

        //Master rewards are fixed for unit-testing
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();


        let mut rewards = 100;
        let mut messages = vec![];

        messages.push(snip20::mint_msg(
            spy_address.clone(),
            Uint128(rewards),
            None,
            1,
            config.reward_token.contract_hash.clone(),
            config.reward_token.address.clone(),
        )?);

        // Notify to the spy contract on the new allocation
        messages.push(
            WasmMsg::Execute {
                contract_addr: spy_address.clone(),
                callback_code_hash: spy_hash,
                msg: to_binary(&LPStakingHandleMsg::NotifyAllocation {
                    amount: Uint128(rewards),
                    hook,
                })?,
                send: vec![],
            }
                .into(),
        );

        Ok(HandleResponse {
            messages,
            log: vec![],
            data: None,
        })
    }

    pub enum MasterHandleAnswer {
        Success,
        Failure,
    }

    ///Initialization
    #[test]
    fn initialized_snip20_messages() {
        let (_init_result, _deps) = init_helper();
        //two register receive and two set viewing key msgs
        assert_eq!(4, _init_result.unwrap().messages.len());
    }

    #[test]
    fn initialized_configuration() {
        let (_init_result, mut deps) = init_helper();
        let env = mock_env("admin", &[], 1);
        //two register receive and two set viewing key msgs
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();
        assert_eq!(config.admin.0, "admin");
        assert_eq!(config.reward_token.address.0, "sefi");
        assert_eq!(config.inc_token.address.0, "Lp-sScrt-sEth");
        assert_eq!(config.is_stopped, false);
        assert_eq!(config.master.address.0, "Master");
        assert_eq!(config.own_addr.0, env.contract.address.to_string());
        assert_eq!(config.viewing_key, "123");
    }

    #[test]
    fn initialized_reward_pool() {
        let (_init_result, mut deps) = init_helper();
        let reward_pool: RewardPool = TypedStoreMut::attach(&mut deps.storage).load(REWARD_POOL_KEY).unwrap();
        assert_eq!(reward_pool.residue, 0);
        assert_eq!(reward_pool.acc_reward_per_share, 0);
        assert_eq!(reward_pool.inc_token_supply, 0);
    }

    #[test]
    fn initialized_token_info() {
        let (_init_result, deps) = init_helper();
        let token_info: TokenInfo = TypedStore::attach(&deps.storage).load(TOKEN_INFO_KEY).unwrap();
        assert_eq!(token_info.name, "");
        assert_eq!(token_info.symbol, "");
    }

    ///Handle
    #[test]
    fn checking_create_viewing_key() {
        let env = mock_env("admin", &[], 10);
        let (_init_result, mut deps) = init_helper();
        let entropy = "entropy".to_string();

        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();
        let prng_seed = config.prng_seed;
        let key = ViewingKey::new(&env, &prng_seed, (&entropy.clone()).as_ref());
        let vk: Vec<u8> = key.to_hashed().to_vec();

        let handle_msg = LPStakingHandleMsg::CreateViewingKey { entropy, padding: None };
        &handle(&mut deps, env.clone(), handle_msg);
        let vk_store = ReadonlyPrefixedStorage::new(VIEWING_KEY_KEY, &deps.storage);
        let expected_key = vk_store.get(env.message.sender.0.as_bytes()).unwrap();

        assert_eq!(vk, expected_key);
    }

    #[test]
    fn checking_set_viewing_key() {
        let env = mock_env("admin", &[], 10);
        let (_init_result, mut deps) = init_helper();
        let key = "setting_viewing_key".to_string();
        let vk: Vec<u8> = ViewingKey(key.clone()).to_hashed().to_vec();
        let handle_msg = LPStakingHandleMsg::SetViewingKey { key, padding: None };
        &handle(&mut deps, env.clone(), handle_msg);
        let vk_store = ReadonlyPrefixedStorage::new(VIEWING_KEY_KEY, &deps.storage);
        let expected_key: Vec<u8> = vk_store.get(env.message.sender.0.as_bytes()).unwrap();
        assert_eq!(vk, expected_key);
    }

    #[test]
    fn checking_contract_stopped() {
        let env = mock_env("admin", &[], 10);
        let (_init_result, mut deps) = init_helper();
        let handle_msg = LPStakingHandleMsg::StopContract {};
        &handle(&mut deps, env.clone(), handle_msg);
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();
        assert_eq!(config.is_stopped, true);

        let (init_result, mut deps) = init_helper();

        let stop_msg = LPStakingHandleMsg::StopContract {};
        let handle_response = handle(&mut deps, mock_env("not_admin", &[], 10), stop_msg.clone());
        assert_eq!(
            handle_response.unwrap_err(),
            StdError::GenericErr {
                msg: "not an admin: not_admin".to_string(),
                backtrace: None,
            }
        );

        let handle_response = handle(&mut deps, mock_env("admin", &[], 10), stop_msg);
        let unwrapped_result: LPStakingHandleAnswer =
            from_binary(&handle_response.unwrap().data.unwrap()).unwrap();
        assert_eq!(
            to_binary(&unwrapped_result).unwrap(),
            to_binary(&LPStakingHandleAnswer::StopContract { status: Success }).unwrap()
        );

        let redeem_msg = LPStakingHandleMsg::Redeem { amount: None };
        let handle_response = handle(&mut deps, mock_env("user", &[], 20), redeem_msg);
        assert_eq!(
            handle_response.unwrap_err(),
            StdError::GenericErr {
                msg: "this contract is stopped and this action is not allowed".to_string(),
                backtrace: None,
            }
        );

        let resume_msg = LPStakingHandleMsg::ResumeContract {};
        let handle_response = handle(&mut deps, mock_env("admin", &[], 21), resume_msg);
        let unwrapped_result: LPStakingHandleAnswer =
            from_binary(&handle_response.unwrap().data.unwrap()).unwrap();
        assert_eq!(
            to_binary(&unwrapped_result).unwrap(),
            to_binary(&LPStakingHandleAnswer::ResumeContract { status: Success }).unwrap()
        );
    }

    #[test]
    fn checking_resume_contract() {
        let env = mock_env("admin", &[], 10);
        let (_init_result, mut deps) = init_helper();
        let handle_msg = LPStakingHandleMsg::StopContract {};
        &handle(&mut deps, env.clone(), handle_msg);
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();
        assert_eq!(config.is_stopped, true);

        let handle_msg = LPStakingHandleMsg::ResumeContract {};
        &handle(&mut deps, env.clone(), handle_msg);
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();
        assert_eq!(config.is_stopped, false);
    }


    #[test]
    fn change_admin() {
        let env = mock_env("admin", &[], 10);
        let (_init_result, mut deps) = init_helper();
        let handle_msg = LPStakingHandleMsg::ChangeAdmin { address: HumanAddr("chadBoy".to_string()) };
        &handle(&mut deps, env.clone(), handle_msg);

        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();
        assert_eq!(config.admin, HumanAddr("chadBoy".to_string()));
    }


    #[test]
    fn testing_receive() {
        let env = mock_env("admin", &[], 10);
        let (_init_result, mut deps) = init_helper();

        //1)when sender is not inc token
        let handle_msg = LPStakingHandleMsg::Receive {
            sender: HumanAddr("admin".to_string()),
            from: HumanAddr("haseeb".to_string()),
            amount: Uint128(1000),
            msg: to_binary(&LPStakingReceiveMsg::Deposit {}).unwrap(),
        };
        let unwrapped_errors = handle(&mut deps, env.clone(), handle_msg);
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();


        assert_eq!(unwrapped_errors.unwrap_err(), StdError::generic_err(format!(
            "This token is not supported. Supported: {}, given: {}",
            config.inc_token.address, env.message.sender
        )));

        //2) Checking the HandleResponse
        let env = mock_env("Lp-sScrt-sEth", &[], 10);
        let handle_msg = LPStakingHandleMsg::Receive {
            sender: HumanAddr("Lp-sScrt-sEth".to_string()),
            from: HumanAddr("Lp-sScrt-sEth".to_string()),
            amount: Uint128(1000),
            msg: to_binary(&LPStakingReceiveMsg::Deposit {}).unwrap(),
        };
        let unwrapped_msg = handle(&mut deps, env.clone(), handle_msg).unwrap();
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();

        let hook = Some(to_binary(&LPStakingHookMsg::Deposit { from: HumanAddr("Lp-sScrt-sEth".to_string()), amount: Uint128(1000) }).unwrap());

        assert_eq!(unwrapped_msg.messages[0], WasmMsg::Execute {
            contract_addr: config.master.address,
            callback_code_hash: config.master.contract_hash,
            msg: to_binary(&MasterHandleMsg::UpdateAllocation {
                spy_addr: env.contract.address,
                spy_hash: env.contract_code_hash,
                hook,
            }).unwrap(),
            send: vec![],
        }
            .into());

        //3) Calling master contract
        let env = mock_env("admin", &[], 10);
        let hook = Some(to_binary(&LPStakingReceiveMsg::Deposit {}).unwrap());
        let msgs = master_update_allocation(&mut deps, env.clone(),
                                            env.contract.address.clone(), env.contract_code_hash.clone(), hook.clone());
        let unwrapped_msgs = msgs.unwrap();

        assert_eq!(unwrapped_msgs.messages[1], WasmMsg::Execute {
            contract_addr: env.contract.address.clone(),
            callback_code_hash: env.contract_code_hash.clone(),
            msg: to_binary(&LPStakingHandleMsg::NotifyAllocation {
                amount: Uint128(100),
                hook,
            }).unwrap(),
            send: vec![],
        }
            .into(),
        );
    }

    #[test]
    fn testing_notification_allocation() {

        //1)
        let (_init_result, mut deps) = init_helper();
        let env = mock_env("NotMaster", &[], 10);
        let hook = Some(to_binary(&LPStakingHookMsg::Deposit { from: HumanAddr("User1".to_string()), amount: Uint128(1000) }).unwrap());

        let handle_msg = LPStakingHandleMsg::NotifyAllocation { amount: Uint128(1000), hook };

        let unwrapped_msg = handle(&mut deps, env.clone(), handle_msg).unwrap_err();
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();


        assert_eq!(unwrapped_msg, StdError::generic_err(
            "you are not allowed to call this function",
        ));
    }

    #[test]
    fn testing_reward_pool() {
        //1)
        let (_init_result, mut deps) = init_helper();
        let env = mock_env("Master", &[], 10);
        let hook = Some(to_binary(&LPStakingHookMsg::Deposit { from: HumanAddr("User1".to_string()), amount: Uint128(1000) }).unwrap());
        //amount here means reward send from master contract
        let handle_msg = LPStakingHandleMsg::NotifyAllocation { amount: Uint128(1500), hook };
        handle(&mut deps, env.clone(), handle_msg).unwrap();

        //checking reward pool

        let mut rewards_store = TypedStoreMut::attach(&mut deps.storage);
        let mut reward_pool: RewardPool = rewards_store.load(REWARD_POOL_KEY).unwrap();
        assert_eq!(reward_pool.inc_token_supply, 1000);
        assert_eq!(reward_pool.residue, 1500);
        assert_eq!(reward_pool.acc_reward_per_share, 0);


        let hook = Some(to_binary(&LPStakingHookMsg::Deposit { from: HumanAddr("User2".to_string()), amount: Uint128(2000) }).unwrap());
        //amount here means reward send from master contract
        let handle_msg = LPStakingHandleMsg::NotifyAllocation { amount: Uint128(3000), hook };
        handle(&mut deps, env, handle_msg).unwrap();


        let mut rewards_store = TypedStoreMut::attach(&mut deps.storage);
        let mut reward_pool: RewardPool = rewards_store.load(REWARD_POOL_KEY).unwrap();
        pub const REWARD_SCALE: u128 = 1_000_000_000_000;
        let acc_reward_per_share = 4500 * REWARD_SCALE / 1000;
        //acc_reward_per_share changes before the inc_token_supply is updated.
        assert_eq!(reward_pool.inc_token_supply, 3000);
        assert_eq!(reward_pool.residue, 0);
        assert_eq!(reward_pool.acc_reward_per_share, acc_reward_per_share)
    }

    #[test]
    fn testing_users() {
        //1)
        let (_init_result, mut deps) = init_helper();
        let env = mock_env("Master", &[], 10);
        let hook = Some(to_binary(&LPStakingHookMsg::Deposit { from: HumanAddr("User1".to_string()), amount: Uint128(1000) }).unwrap());
        //amount here means reward send from master contract
        let handle_msg = LPStakingHandleMsg::NotifyAllocation { amount: Uint128(1000), hook };
        handle(&mut deps, env.clone(), handle_msg).unwrap();

        let mut from = HumanAddr("User1".to_string());

        let mut users_store = TypedStoreMut::attach(&mut deps.storage);
        let mut user = users_store
            .load(from.0.as_bytes())
            .unwrap_or(UserInfo { locked: 0, debt: 0 }); // NotFound is the only possible error

        assert_eq!(user.locked, 1000);
        assert_eq!(user.debt, 0);

        let hook = Some(to_binary(&LPStakingHookMsg::Deposit { from: HumanAddr("User1".to_string()), amount: Uint128(1000) }).unwrap());
        //amount here means reward send from master contract
        let handle_msg = LPStakingHandleMsg::NotifyAllocation { amount: Uint128(1000), hook };
        handle(&mut deps, env, handle_msg).unwrap();

        let mut from = HumanAddr("User1".to_string());
        let mut users_store = TypedStoreMut::attach(&mut deps.storage);
        let mut user = users_store
            .load(from.0.as_bytes())
            .unwrap_or(UserInfo { locked: 0, debt: 0 }); // NotFound is the only possible error

        assert_eq!(user.locked, 2000);
        assert_eq!(user.debt, 4000);
    }

    #[test]
    fn testing_inc_token_supply() {
        let (_init_result, mut deps) = init_helper();
        let env = mock_env("Master", &[], 10);
        let hook = Some(to_binary(&LPStakingHookMsg::Deposit { from: HumanAddr("User1".to_string()), amount: Uint128(1000) }).unwrap());
        //amount here means reward send from master contract
        let handle_msg = LPStakingHandleMsg::NotifyAllocation { amount: Uint128(2000), hook };
        handle(&mut deps, env.clone(), handle_msg).unwrap();

        let mut rewards_store = TypedStoreMut::attach(&mut deps.storage);
        let mut reward_pool: RewardPool = rewards_store.load(REWARD_POOL_KEY).unwrap();

        assert_eq!(reward_pool.residue, 2000);
        assert_eq!(reward_pool.inc_token_supply, 1000);
        assert_eq!(reward_pool.acc_reward_per_share, 0);
    }

    #[test]
    fn testing_redeem() {
        // receive -- deposit
        let env = mock_env("admin", &[], 10);
        let (_init_result, mut deps) = init_helper();

        //1)when sender is not inc token
        let handle_msg = LPStakingHandleMsg::Receive {
            sender: HumanAddr("admin".to_string()),
            from: HumanAddr("haseeb".to_string()),
            amount: Uint128(1000),
            msg: to_binary(&LPStakingReceiveMsg::Deposit {}).unwrap(),
        };
        handle(&mut deps, env.clone(), handle_msg);

        //redeem
        let env = mock_env("haseeb", &[], 10);
        let (_init_result, mut deps) = init_helper();
        //1)when sender is not inc token
        let handle_msg = LPStakingHandleMsg::Redeem {
            amount: Some(Uint128(1000)),
        };
        let hook = Some(to_binary(&LPStakingHookMsg::Redeem {
            to: env.message.sender.clone(),
            amount: Some(Uint128(1000)),
        }).unwrap());
        let unwrapped = handle(&mut deps, env.clone(), handle_msg).unwrap();
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();

        assert_eq!(unwrapped.messages[0], WasmMsg::Execute {
            contract_addr: config.master.address,
            callback_code_hash: config.master.contract_hash,
            msg: to_binary(&MasterHandleMsg::UpdateAllocation {
                spy_addr: env.contract.address,
                spy_hash: env.contract_code_hash,
                hook,
            }).unwrap(),
            send: vec![],
        }
            .into());
    }

    //Query
    #[test]
    fn testing_contract_status() {
        let env = mock_env("admin", &[], 10);
        let (_init_result, mut deps) = init_helper();
        // query_contract_status()
        let query_msg = LPStakingQueryMsg::ContractStatus {};
        let results: LPStakingQueryAnswer = from_binary(&query(&deps, query_msg).unwrap()).unwrap();
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();

        assert_eq!(to_binary(&results).unwrap(), to_binary(&LPStakingQueryAnswer::ContractStatus { is_stopped: config.is_stopped }).unwrap());
    }

    #[test]
    fn testing_reward_token() {
        let env = mock_env("admin", &[], 10);
        let (_init_result, mut deps) = init_helper();
        // query_contract_status()
        let query_msg = LPStakingQueryMsg::RewardToken {};
        let results: LPStakingQueryAnswer = from_binary(&query(&deps, query_msg).unwrap()).unwrap();
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();

        assert_eq!(to_binary(&results).unwrap(), to_binary(&LPStakingQueryAnswer::RewardToken {
            token: config.reward_token,
        }).unwrap());
    }

    #[test]
    fn testing_inc_token() {
        let env = mock_env("admin", &[], 10);
        let (_init_result, mut deps) = init_helper();
        // query_contract_status()
        let query_msg = LPStakingQueryMsg::IncentivizedToken {};
        let results: LPStakingQueryAnswer = from_binary(&query(&deps, query_msg).unwrap()).unwrap();
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();
        assert_eq!(to_binary(&results).unwrap(), to_binary(&LPStakingQueryAnswer::IncentivizedToken {
            token: config.inc_token,
        }).unwrap());
    }

    #[test]
    fn testing_token_info() {
        let env = mock_env("admin", &[], 10);
        let (_init_result, mut deps) = init_helper();
        let query_msg = LPStakingQueryMsg::TokenInfo {};
        let results: LPStakingQueryAnswer = from_binary(&query(&deps, query_msg).unwrap()).unwrap();
        let config: Config = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY).unwrap();
        let token_info: TokenInfo = TypedStore::attach(&deps.storage).load(TOKEN_INFO_KEY).unwrap();

        assert_eq!(to_binary(&results).unwrap(), to_binary(&LPStakingQueryAnswer::TokenInfo {
            name: token_info.name,
            symbol: token_info.symbol,
            decimals: 1,
            total_supply: None,
        }).unwrap());
    }
}
