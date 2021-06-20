use crate::challenge::{sha_256, Challenge};
use crate::msg::{InitMsg, QueryAnswer, QueryMsg, ResponseStatus};
use crate::querier::query_staking_balance;
use crate::state::{
    Config, PollContract, ADMIN_KEY, CONFIG_KEY, CURRENT_CHALLENGE_KEY, DEFAULT_POLL_CONFIG_KEY,
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
        PollFactoryHandleMsg::RegisterForUpdates { .. } => unimplemented!(),
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
        config: poll_config,
        choices: poll_choices,
        staking_pool: config.staking_pool.clone(),
        init_hook: Some(InitHook {
            contract_addr: env.contract.address,
            code_hash: env.contract_code_hash,
            msg: to_binary(&RegisterForUpdates { challenge: key.0 })?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coins, from_binary, StdError};

    #[test]
    fn test() {}
}
