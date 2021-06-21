use crate::challenge::{sha_256, Challenge};
use crate::msg::{InitMsg, QueryAnswer, QueryMsg, ResponseStatus};
use crate::querier::query_staking_balance;
use crate::state::{
    ActivePoll, Config, PollContract, ACTIVE_POLLS_KEY, ADMIN_KEY, CONFIG_KEY,
    CURRENT_CHALLENGE_KEY, DEFAULT_POLL_CONFIG_KEY,
};
use cosmwasm_std::{
    log, to_binary, Api, Binary, CosmosMsg, Env, Extern, HandleResponse, HumanAddr, InitResponse,
    Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use scrt_finance::secret_vote_types::PollFactoryHandleMsg::RegisterForUpdates;
use scrt_finance::secret_vote_types::{
    InitHook, PollConfig, PollFactoryHandleMsg, PollHandleMsg, PollInitMsg, PollMetadata,
};
use scrt_finance::types::SecretContract;
use secret_toolkit::snip20;
use secret_toolkit::snip20::balance_query;
use secret_toolkit::storage::{TypedStore, TypedStoreMut};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let owner = env.message.sender;
    TypedStoreMut::attach(&mut deps.storage).store(ADMIN_KEY, &owner)?;

    let default_poll_config = PollConfig {
        duration: 1209600, // 2 weeks
        quorum: 34,        // X/100% (percentage)
        min_threshold: 0,
    };
    TypedStoreMut::attach(&mut deps.storage)
        .store(DEFAULT_POLL_CONFIG_KEY, &default_poll_config)?;

    let prng_seed_hashed = sha_256(&msg.prng_seed.0);
    TypedStoreMut::attach(&mut deps.storage).store(
        CONFIG_KEY,
        &Config {
            poll_contract: PollContract {
                code_id: msg.poll_contract.code_id,
                code_hash: msg.poll_contract.code_hash,
            },
            staking_pool: msg.staking_pool,
            id_counter: 0,
            prng_seed: prng_seed_hashed,
        },
    )?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: PollFactoryHandleMsg,
) -> StdResult<HandleResponse> {
    let active_pools = remove_inactive_polls(deps, &env)?; // TODO probably can call this only on `UpdateVotingPower`, leaving it here for now

    match msg {
        PollFactoryHandleMsg::NewPoll {
            poll_metadata,
            poll_config,
            poll_choices,
        } => new_poll(
            deps,
            env,
            poll_metadata,
            poll_config.unwrap_or(TypedStore::attach(&deps.storage).load(DEFAULT_POLL_CONFIG_KEY)?),
            poll_choices,
        ),
        PollFactoryHandleMsg::UpdateVotingPower { voter, new_power } => {
            update_voting_power(deps, env, voter, new_power, active_pools)
        }
        PollFactoryHandleMsg::UpdatePollContract {
            new_id,
            new_code_hash,
        } => update_poll_contract(deps, env, new_id, new_code_hash),
        PollFactoryHandleMsg::UpdateDefaultPollConfig {
            duration,
            quorum,
            min_threshold,
        } => update_default_poll_config(deps, env, duration, quorum, min_threshold),
        PollFactoryHandleMsg::RegisterForUpdates {
            challenge,
            end_time,
        } => register_for_updates(deps, env, Challenge(challenge), end_time),
    }
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    unimplemented!()
}

fn new_poll<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    poll_metadata: PollMetadata,
    poll_config: PollConfig,
    poll_choices: Vec<String>,
) -> StdResult<HandleResponse> {
    let mut config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    let key = Challenge::new(&env, &config.prng_seed);
    TypedStoreMut::attach(&mut deps.storage).store(CURRENT_CHALLENGE_KEY, &key)?;

    let init_msg = PollInitMsg {
        metadata: poll_metadata.clone(),
        config: poll_config.clone(),
        choices: poll_choices,
        staking_pool: config.staking_pool.clone(),
        init_hook: Some(InitHook {
            contract_addr: env.contract.address,
            code_hash: env.contract_code_hash,
            msg: to_binary(&RegisterForUpdates {
                challenge: key.0,
                end_time: env.block.time + poll_config.duration, // If this fails, we have bigger problems than this :)
            })?,
        }),
    };
    let label: String = format!("secret-poll-{}", config.id_counter);

    config.id_counter += 1;
    TypedStoreMut::attach(&mut deps.storage).store(CONFIG_KEY, &config)?;

    Ok(HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
            code_id: config.poll_contract.code_id,
            callback_code_hash: config.poll_contract.code_hash,
            msg: to_binary(&init_msg)?,
            send: vec![],
            label,
        })],
        log: vec![],
        data: None,
    })
}

fn register_for_updates<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    response: Challenge,
    end_time: u64,
) -> StdResult<HandleResponse> {
    let challenge: Challenge = TypedStore::attach(&deps.storage).load(CURRENT_CHALLENGE_KEY)?;
    if !response.check_challenge(&challenge.to_hashed()) {
        return Err(StdError::generic_err("challenge did not match. This function can be called only as a callback from a new poll contract"));
    } else {
        TypedStoreMut::<Challenge, S>::attach(&mut deps.storage).remove(CURRENT_CHALLENGE_KEY);
    }

    let mut active_polls_store = TypedStoreMut::<Vec<ActivePoll>, S>::attach(&mut deps.storage);
    let mut active_polls = active_polls_store
        .load(ACTIVE_POLLS_KEY)
        .unwrap_or_default();
    active_polls.push(ActivePoll {
        address: env.message.sender.clone(),
        end_time,
    });
    active_polls_store.store(ACTIVE_POLLS_KEY, &active_polls)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("new_poll", env.message.sender)],
        data: Some(to_binary(&ResponseStatus::Success)?),
    })
}

fn update_voting_power<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    voter: HumanAddr,
    new_power: Uint128,
    active_polls: Vec<ActivePoll>,
) -> StdResult<HandleResponse> {
    let config: Config = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    if env.message.sender != config.staking_pool.address {
        return Err(StdError::unauthorized());
    }

    let update_msg = to_binary(&PollHandleMsg::UpdateVotingPower {
        voter: voter.clone(),
        new_power,
    })?; // This API should be kept if a new poll contract is introduced

    let mut messages = vec![];
    for poll in active_polls {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: poll.address,
            callback_code_hash: config.poll_contract.code_hash.clone(),
            msg: update_msg.clone(),
            send: vec![],
        }))
    }

    Ok(HandleResponse {
        messages,
        log: vec![log("voting power update", voter.0)],
        data: None,
    })
}

fn update_poll_contract<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    new_id: u64,
    new_code_hash: String,
) -> StdResult<HandleResponse> {
    enforce_admin(deps, &env)?;

    let mut config_store = TypedStoreMut::<Config, S>::attach(&mut deps.storage);
    let mut config = config_store.load(CONFIG_KEY)?;
    config.poll_contract = PollContract {
        code_id: new_id,
        code_hash: new_code_hash,
    };
    config_store.store(CONFIG_KEY, &config)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&ResponseStatus::Success)?),
    })
}

fn update_default_poll_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    duration: Option<u64>,
    quorum: Option<u8>,
    min_threshold: Option<u8>,
) -> StdResult<HandleResponse> {
    enforce_admin(deps, &env)?;

    let mut poll_config_store = TypedStoreMut::<PollConfig, S>::attach(&mut deps.storage);
    let mut default_config = poll_config_store.load(DEFAULT_POLL_CONFIG_KEY)?;

    if let Some(new_duration) = duration {
        default_config.duration = new_duration;
    }

    if let Some(new_quorum) = quorum {
        default_config.quorum = new_quorum;
    }

    if let Some(new_threshold) = min_threshold {
        default_config.min_threshold = new_threshold;
    }

    poll_config_store.store(DEFAULT_POLL_CONFIG_KEY, &default_config)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: None,
    })
}

// Helper functions

fn remove_inactive_polls<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
) -> StdResult<Vec<ActivePoll>> {
    let active_polls = get_active_polls(deps, env.block.time)?;
    TypedStoreMut::<Vec<ActivePoll>, S>::attach(&mut deps.storage)
        .store(ACTIVE_POLLS_KEY, &active_polls)?;

    Ok(active_polls)
}

fn get_active_polls<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    current_time: u64,
) -> StdResult<Vec<ActivePoll>> {
    let active_polls_to_update: Vec<ActivePoll> =
        TypedStore::attach(&deps.storage).load(ACTIVE_POLLS_KEY)?;

    let active_polls = active_polls_to_update
        .into_iter()
        .filter(|p| p.end_time >= current_time)
        .collect();

    Ok(active_polls)
}

fn enforce_admin<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
) -> StdResult<()> {
    let admin: HumanAddr = TypedStore::attach(&deps.storage).load(ADMIN_KEY)?;

    if admin != env.message.sender {
        return Err(StdError::unauthorized());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coins, from_binary, StdError};

    #[test]
    fn test() {}
}
