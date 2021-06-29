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
use scrt_finance::secret_vote_types::PollFactoryHandleMsg;
use scrt_finance::types::{RewardPool, SecretContract, TokenInfo, UserInfo};
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

    if let Some(subs) = msg.subscribers {
        TypedStoreMut::attach(&mut deps.storage).store(SUBSCRIBERS_KEY, &subs)?;
    } else {
        TypedStoreMut::attach(&mut deps.storage)
            .store(SUBSCRIBERS_KEY, &Vec::<SecretContract>::new())?;
    }

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
        LPStakingHandleMsg::NotifyAllocation { amount, hook } => notify_allocation(
            deps,
            env,
            amount.u128(),
            hook.map(|h| from_binary(&h).unwrap()),
        ),
        LPStakingHandleMsg::AddSubs { contracts } => add_subscribers(deps, env, contracts),
        LPStakingHandleMsg::RemoveSubs { contracts } => remove_subscribers(deps, env, contracts),
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
        LPStakingQueryMsg::TotalLocked {} => query_total_locked(deps),
        LPStakingQueryMsg::Subscribers {} => query_subscribers(deps),
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
    amount: u128,
    hook: Option<LPStakingHookMsg>,
) -> StdResult<HandleResponse> {
    let config = TypedStore::<Config, S>::attach(&deps.storage).load(CONFIG_KEY)?;
    if env.message.sender != config.master.address && env.message.sender != config.admin {
        return Err(StdError::generic_err(
            "you are not allowed to call this function",
        ));
    }

    let reward_pool = update_rewards(deps, /*&env, &config,*/ amount)?;

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

    let subs: Vec<SecretContract> = TypedStore::attach(&deps.storage).load(SUBSCRIBERS_KEY)?;
    let sub_messages: StdResult<Vec<CosmosMsg>> = subs
        .into_iter()
        .map(|s| create_subscriber_msg(s, &from, user.locked))
        .collect();
    messages.extend(sub_messages?);

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
    _env: Env,
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
    debug_print(format!("DEBUG DEBUG DEBUG"));
    debug_print(format!(
        "reward pool: | residue: {} | total supply: {} | acc: {} |",
        reward_pool.residue, reward_pool.inc_token_supply, reward_pool.acc_reward_per_share
    ));
    debug_print(format!(
        "user: | locked: {} | debt: {} |",
        user.locked, user.debt
    ));
    debug_print(format!("pending: {}", pending));
    debug_print(format!("DEBUG DEBUG DEBUG"));
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
        to.clone(),
        Uint128(amount),
        None,
        RESPONSE_BLOCK_SIZE,
        config.inc_token.contract_hash,
        config.inc_token.address,
    )?);

    let subs: Vec<SecretContract> = TypedStore::attach(&deps.storage).load(SUBSCRIBERS_KEY)?;
    let sub_messages: StdResult<Vec<CosmosMsg>> = subs
        .into_iter()
        .map(|s| create_subscriber_msg(s, &to, user.locked))
        .collect();
    messages.extend(sub_messages?);

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

fn add_subscribers<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    new_subs: Vec<SecretContract>,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    enforce_admin(config, env)?;

    let mut subs_store = TypedStoreMut::attach(&mut deps.storage);
    let mut subs: Vec<SecretContract> = subs_store.load(SUBSCRIBERS_KEY)?;
    subs.extend(new_subs);
    subs_store.store(SUBSCRIBERS_KEY, &subs)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&LPStakingHandleAnswer::AddSubs {
            status: Success,
        })?),
    })
}

fn remove_subscribers<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    subs_to_remove: Vec<HumanAddr>,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    enforce_admin(config, env)?;

    let mut subs_store = TypedStoreMut::attach(&mut deps.storage);
    let mut subs: Vec<SecretContract> = subs_store.load(SUBSCRIBERS_KEY)?;

    // TODO is there a better way to do this?
    subs = subs
        .into_iter()
        .filter(|s| !subs_to_remove.contains(&s.address))
        .collect();

    subs_store.store(SUBSCRIBERS_KEY, &subs)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&LPStakingHandleAnswer::RemoveSubs {
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

fn query_total_locked<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Binary> {
    // let subs: Vec<SecretContract> = TypedStore::attach(&deps.storage).load(SUBSCRIBERS_KEY)?;
    let reward_pool: RewardPool = TypedStore::attach(&deps.storage).load(REWARD_POOL_KEY)?;

    to_binary(&LPStakingQueryAnswer::TotalLocked {
        amount: Uint128(reward_pool.inc_token_supply),
    })
}

fn query_subscribers<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Binary> {
    let subs: Vec<SecretContract> = TypedStore::attach(&deps.storage).load(SUBSCRIBERS_KEY)?;

    to_binary(&LPStakingQueryAnswer::Subscribers { contracts: subs })
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
    if newly_allocated == 0 {
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

fn create_subscriber_msg(
    sub: SecretContract,
    user: &HumanAddr,
    new_vp: u128,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: sub.address,
        callback_code_hash: sub.contract_hash,
        msg: to_binary(&PollFactoryHandleMsg::UpdateVotingPower {
            voter: user.clone(),
            new_power: Uint128(new_vp),
        })?,
        send: vec![],
    }))
}

#[cfg(test)]
mod tests {
    use crate::constants::{CONFIG_KEY, RESPONSE_BLOCK_SIZE, REWARD_POOL_KEY};
    use crate::contract::{
        add_subscribers, create_subscriber_msg, deposit_hook, init, redeem_hook, remove_subscribers,
    };
    use crate::state::Config;
    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{
        to_binary, BlockInfo, Coin, ContractInfo, CosmosMsg, Env, Extern, HandleResponse,
        HumanAddr, InitResponse, MessageInfo, StdResult, Uint128, WasmMsg,
    };
    use scrt_finance::lp_staking_msg::LPStakingResponseStatus::Success;
    use scrt_finance::lp_staking_msg::{
        LPStakingHandleAnswer, LPStakingInitMsg, LPStakingReceiveAnswer,
    };
    use scrt_finance::secret_vote_types::PollFactoryHandleMsg;
    use scrt_finance::types::{RewardPool, SecretContract, TokenInfo};
    use secret_toolkit::storage::TypedStore;

    fn init_helper(
        subscribers: Option<Vec<SecretContract>>,
    ) -> (
        StdResult<InitResponse>,
        Extern<MockStorage, MockApi, MockQuerier>,
    ) {
        let mut deps = mock_dependencies(20, &[]);
        let env = mock_env("admin", &[]);

        let init_msg = LPStakingInitMsg {
            reward_token: SecretContract {
                address: HumanAddr("reward_t".to_string()),
                contract_hash: "".to_string(),
            },
            inc_token: SecretContract {
                address: HumanAddr("inc_t".to_string()),
                contract_hash: "".to_string(),
            },
            master: SecretContract {
                address: HumanAddr("master".to_string()),
                contract_hash: "".to_string(),
            },
            viewing_key: "123".to_string(),
            token_info: TokenInfo {
                name: "".to_string(),
                symbol: "".to_string(),
            },
            prng_seed: Default::default(),
            subscribers,
        };

        (init(&mut deps, env, init_msg), deps)
    }

    fn deposit_helper(
        deps: &mut Extern<MockStorage, MockApi, MockQuerier>,
        addr: String,
        amount: u128,
    ) -> HandleResponse {
        let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY).unwrap();
        let reward_pool: RewardPool = TypedStore::attach(&deps.storage)
            .load(REWARD_POOL_KEY)
            .unwrap_or(RewardPool {
                residue: 0,
                inc_token_supply: 0,
                acc_reward_per_share: 0,
            });

        deposit_hook(
            deps,
            mock_env(addr.clone(), &[]),
            config,
            reward_pool,
            HumanAddr(addr),
            amount,
        )
        .unwrap()
    }

    fn redeem_helper(
        deps: &mut Extern<MockStorage, MockApi, MockQuerier>,
        addr: String,
        amount: u128,
    ) -> HandleResponse {
        let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY).unwrap();
        let reward_pool: RewardPool = TypedStore::attach(&deps.storage)
            .load(REWARD_POOL_KEY)
            .unwrap_or(RewardPool {
                residue: 0,
                inc_token_supply: 0,
                acc_reward_per_share: 0,
            });

        redeem_hook(
            deps,
            mock_env(addr.clone(), &[]),
            config,
            reward_pool,
            HumanAddr(addr),
            Some(Uint128(amount)),
        )
        .unwrap()
    }

    #[test]
    fn test_subs_deposit() {
        let sub_a = SecretContract {
            address: HumanAddr("sub_a".to_string()),
            contract_hash: "".to_string(),
        };
        let sub_b = SecretContract {
            address: HumanAddr("sub_b".to_string()),
            contract_hash: "".to_string(),
        };

        let (init_result, mut deps) = init_helper(Some(vec![sub_a.clone(), sub_b.clone()]));
        assert!(init_result.is_ok());

        let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY).unwrap();
        let result = deposit_helper(&mut deps, "user".into(), 100);
        assert_eq!(
            result,
            HandleResponse {
                messages: vec![
                    create_subscriber_msg(sub_a, &HumanAddr("user".to_string()), 100).unwrap(),
                    create_subscriber_msg(sub_b, &HumanAddr("user".to_string()), 100).unwrap()
                ],
                log: vec![],
                data: Some(
                    to_binary(&LPStakingReceiveAnswer::Deposit { status: Success }).unwrap()
                )
            }
        )
    }

    #[test]
    fn test_nosubs_deposit() {
        let (init_result, mut deps) = init_helper(Some(vec![]));
        assert!(init_result.is_ok());

        let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY).unwrap();
        let result = deposit_helper(&mut deps, "user".into(), 100);
        assert_eq!(
            result,
            HandleResponse {
                messages: vec![],
                log: vec![],
                data: Some(
                    to_binary(&LPStakingReceiveAnswer::Deposit { status: Success }).unwrap()
                )
            }
        )
    }

    #[test]
    fn test_create_sub_msg() {
        let sub_a = SecretContract {
            address: HumanAddr("sub_a".to_string()),
            contract_hash: "".to_string(),
        };
        let sub_msg =
            create_subscriber_msg(sub_a.clone(), &HumanAddr("user".to_string()), 100).unwrap();

        assert_eq!(
            sub_msg,
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: sub_a.address,
                callback_code_hash: sub_a.contract_hash,
                msg: to_binary(&PollFactoryHandleMsg::UpdateVotingPower {
                    voter: HumanAddr("user".to_string()),
                    new_power: Uint128(100),
                })
                .unwrap(),
                send: vec![],
            })
        )
    }

    #[test]
    fn test_subs_redeem() {
        let sub_a = SecretContract {
            address: HumanAddr("sub_a".to_string()),
            contract_hash: "".to_string(),
        };
        let sub_b = SecretContract {
            address: HumanAddr("sub_b".to_string()),
            contract_hash: "".to_string(),
        };

        let (init_result, mut deps) = init_helper(Some(vec![sub_a.clone(), sub_b.clone()]));
        assert!(init_result.is_ok());

        let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY).unwrap();
        let result = deposit_helper(&mut deps, "user".into(), 100);

        let reward_pool: RewardPool = TypedStore::attach(&deps.storage)
            .load(REWARD_POOL_KEY)
            .unwrap();
        let result = redeem_helper(&mut deps, "user".into(), 10);
        assert_eq!(
            result,
            HandleResponse {
                messages: vec![
                    secret_toolkit::snip20::transfer_msg(
                        HumanAddr("user".to_string()),
                        Uint128(10),
                        None,
                        RESPONSE_BLOCK_SIZE,
                        config.inc_token.contract_hash,
                        config.inc_token.address,
                    )
                    .unwrap(),
                    create_subscriber_msg(sub_a, &HumanAddr("user".to_string()), 90).unwrap(),
                    create_subscriber_msg(sub_b, &HumanAddr("user".to_string()), 90).unwrap()
                ],
                log: vec![],
                data: Some(to_binary(&LPStakingHandleAnswer::Redeem { status: Success }).unwrap())
            }
        )
    }

    #[test]
    fn test_add_remove_subs() {
        let sub_a = SecretContract {
            address: HumanAddr("sub_a".to_string()),
            contract_hash: "".to_string(),
        };
        let sub_b = SecretContract {
            address: HumanAddr("sub_b".to_string()),
            contract_hash: "".to_string(),
        };
        let sub_c = SecretContract {
            address: HumanAddr("sub_c".to_string()),
            contract_hash: "".to_string(),
        };

        let (init_result, mut deps) = init_helper(Some(vec![sub_a.clone(), sub_b.clone()]));
        assert!(init_result.is_ok());

        let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY).unwrap();
        deposit_helper(&mut deps, "user".into(), 100);

        add_subscribers(&mut deps, mock_env("admin", &[]), vec![sub_c.clone()]).unwrap();
        let result = deposit_helper(&mut deps, "user".into(), 100);
        assert_eq!(
            result,
            HandleResponse {
                messages: vec![
                    create_subscriber_msg(sub_a.clone(), &HumanAddr("user".to_string()), 200)
                        .unwrap(),
                    create_subscriber_msg(sub_b.clone(), &HumanAddr("user".to_string()), 200)
                        .unwrap(),
                    create_subscriber_msg(sub_c.clone(), &HumanAddr("user".to_string()), 200)
                        .unwrap()
                ],
                log: vec![],
                data: Some(
                    to_binary(&LPStakingReceiveAnswer::Deposit { status: Success }).unwrap()
                )
            }
        );

        remove_subscribers(
            &mut deps,
            mock_env("admin", &[]),
            vec![sub_a.address, sub_b.address, sub_c.address],
        )
        .unwrap();
        let result = redeem_helper(&mut deps, "user".into(), 150);
        assert_eq!(
            result,
            HandleResponse {
                messages: vec![secret_toolkit::snip20::transfer_msg(
                    HumanAddr("user".to_string()),
                    Uint128(150),
                    None,
                    RESPONSE_BLOCK_SIZE,
                    config.inc_token.contract_hash,
                    config.inc_token.address,
                )
                .unwrap()],
                log: vec![],
                data: Some(to_binary(&LPStakingHandleAnswer::Redeem { status: Success }).unwrap())
            }
        );
    }
}
