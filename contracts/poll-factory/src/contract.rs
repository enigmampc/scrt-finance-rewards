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
    InitHook, PollConfig, PollFactoryHandleMsg, PollInitMsg, PollMetadata,
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
    remove_inactive_polls(deps, &env)?; // TODO probably can call this only on `UpdateVotingPower`, leaving it here for now

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
        PollFactoryHandleMsg::UpdateVotingPower { voter, new_power } => unimplemented!(),
        PollFactoryHandleMsg::UpdatePollCodeId { .. } => unimplemented!(),
        PollFactoryHandleMsg::UpdateDefaultPollConfig { .. } => unimplemented!(),
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

    let mut active_polls: Vec<ActivePoll> = TypedStoreMut::attach(&mut deps.storage)
        .load(ACTIVE_POLLS_KEY)
        .unwrap_or_default();
    active_polls.push(ActivePoll {
        address: env.message.sender.clone(),
        end_time,
    });

    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("new_poll", env.message.sender)],
        data: Some(to_binary(&ResponseStatus::Success)?),
    })
}

// Helper functions

fn remove_inactive_polls<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
) -> StdResult<()> {
    let mut active_polls: Vec<ActivePoll> =
        TypedStoreMut::attach(&mut deps.storage).load(ACTIVE_POLLS_KEY)?;

    active_polls = active_polls
        .into_iter()
        .filter(|p| p.end_time >= env.block.time)
        .collect();

    TypedStoreMut::attach(&mut deps.storage).store(ACTIVE_POLLS_KEY, &active_polls)?;

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
