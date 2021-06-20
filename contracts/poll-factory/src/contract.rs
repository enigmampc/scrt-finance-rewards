use crate::msg::{HandleMsg, InitMsg, QueryAnswer, QueryMsg, ResponseStatus};
use crate::querier::query_staking_balance;
use crate::state::{
    ADMIN_KEY, CONFIG_KEY, DEFAULT_POLL_CONFIG_KEY, INTERNAL_ID_COUNTER_KEY, POLL_CODE_ID_KEY,
    STAKING_POOL_KEY,
};
use cosmwasm_std::{
    log, to_binary, Api, Binary, CosmosMsg, Env, Extern, HandleResponse, HumanAddr, InitResponse,
    Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use scrt_finance::secret_vote_types::{PollConfig, PollInitMsg, PollMetadata};
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
    TypedStoreMut::attach(&mut deps.storage).store(STAKING_POOL_KEY, &msg.staking_pool)?;
    TypedStoreMut::attach(&mut deps.storage).store(POLL_CODE_ID_KEY, &msg.poll_code_id)?;
    TypedStoreMut::attach(&mut deps.storage).store(INTERNAL_ID_COUNTER_KEY, &(0 as u128))?;

    let default_poll_config = PollConfig {
        duration: 1209600, // 2 weeks
        quorum: 34,        // X/100% (percentage)
        min_threshold: 0,
    };
    TypedStoreMut::attach(&mut deps.storage)
        .store(DEFAULT_POLL_CONFIG_KEY, &default_poll_config)?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::NewPoll {
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
        HandleMsg::UpdateVotingPower { voter, new_power } => unimplemented!(),
        HandleMsg::UpdatePollCodeId { .. } => unimplemented!(),
        HandleMsg::UpdateDefaultPollConfig { .. } => unimplemented!(),
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
    let code_id = TypedStore::attach(&deps.storage).load(POLL_CODE_ID_KEY)?;
    let staking_pool = TypedStore::attach(&deps.storage).load(STAKING_POOL_KEY)?;
    let id_counter: u128 = TypedStore::attach(&deps.storage).load(INTERNAL_ID_COUNTER_KEY)?;

    let init_msg = PollInitMsg {
        metadata: poll_metadata.clone(),
        config: poll_config,
        choices: poll_choices,
        staking_pool,
    };
    let label: String = format!("secret-poll-{}", id_counter);

    TypedStoreMut::attach(&mut deps.storage).store(INTERNAL_ID_COUNTER_KEY, &(id_counter + 1))?;

    Ok(HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
            code_id,
            callback_code_hash: env.contract_code_hash,
            msg: to_binary(&init_msg)?,
            send: vec![],
            label,
        })],
        log: vec![log("new poll", "new poll")],
        data: Some(to_binary(&ResponseStatus::Success)?),
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
