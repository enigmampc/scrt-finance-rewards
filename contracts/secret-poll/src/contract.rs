use crate::msg::{HandleMsg, InitMsg, QueryMsg, ResponseStatus};
use crate::state::{
    ChoiceIdMap, StoredPollConfig, Tally, Vote, CHOICE_ID_MAP_KEY, CONFIG_KEY, METADATA_KEY,
    OWNER_KEY, STAKING_POOL_KEY, TALLY_KEY,
};
use cosmwasm_std::{
    log, to_binary, Api, Binary, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier,
    StdError, StdResult, Storage, Uint128, WasmMsg,
};
use scrt_finance::types::SecretContract;
use secret_toolkit::snip20;
use secret_toolkit::storage::{TypedStore, TypedStoreMut};
use std::collections::HashMap;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let owner = env.message.sender;
    TypedStoreMut::attach(&mut deps.storage).store(OWNER_KEY, &owner)?; // This is in fact the factory contract
    TypedStoreMut::attach(&mut deps.storage).store(METADATA_KEY, &msg.metadata)?;

    let ending = env.block.time + msg.config.duration;
    TypedStoreMut::attach(&mut deps.storage).store(
        CONFIG_KEY,
        &StoredPollConfig {
            end_timestamp: ending,
            quorum: msg.config.quorum,
            min_threshold: msg.config.min_threshold,
        },
    )?;

    TypedStoreMut::attach(&mut deps.storage).store(STAKING_POOL_KEY, &msg.staking_pool)?;

    if msg.choices.len() > (u8::MAX - 1) as usize {
        return Err(StdError::generic_err(format!(
            "the number of choices for a poll cannot exceed {}",
            u8::MAX - 1
        )));
    }

    // Creating a mapping between a choice's text and it's ID for convenience
    let mut i = 0;
    let choice_id_map: ChoiceIdMap = msg
        .choices
        .iter()
        .map(|c| {
            i += 1;
            (i, c.clone())
        })
        .collect();
    TypedStoreMut::attach(&mut deps.storage).store(CHOICE_ID_MAP_KEY, &choice_id_map)?;

    let mut tally: Tally = HashMap::new();
    for choice in choice_id_map {
        tally.insert(choice.0, 0);
    }
    TypedStoreMut::attach(&mut deps.storage).store(TALLY_KEY, &tally)?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::Vote {
            choice,
            staking_pool_viewing_key,
        } => vote(deps, env, choice, staking_pool_viewing_key),
        HandleMsg::UpdateVotingPower { voter, new_power } => {
            update_voting_power(deps, env, voter, new_power.u128())
        }
        HandleMsg::Finalize { .. } => unimplemented!(),
    }
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Choices { .. } => unimplemented!(),
        QueryMsg::HasVoted { .. } => unimplemented!(),
        QueryMsg::Voters { .. } => unimplemented!(),
        QueryMsg::Tally { .. } => unimplemented!(),
        QueryMsg::Vote { .. } => unimplemented!(),
        QueryMsg::VoteInfo { .. } => unimplemented!(),
    }
}

pub fn vote<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    choice: u8,
    key: String,
) -> StdResult<HandleResponse> {
    require_vote_ongoing(deps, &env)?;

    let mut tally: Tally = TypedStoreMut::attach(&mut deps.storage).load(TALLY_KEY)?;

    if let Some(choice_tally) = tally.get_mut(&choice) {
        let staking_pool: SecretContract =
            TypedStore::attach(&deps.storage).load(STAKING_POOL_KEY)?;
        let voting_power = snip20::balance_query(
            &deps.querier,
            env.message.sender.clone(),
            key,
            256,
            staking_pool.contract_hash,
            staking_pool.address,
        )?;
        *choice_tally += voting_power.amount.u128();

        TypedStoreMut::attach(&mut deps.storage).store(TALLY_KEY, &tally)?;
        store_vote(
            deps,
            env.message.sender.clone(),
            choice,
            voting_power.amount.u128(),
        )?;
    } else {
        return Err(StdError::generic_err(format!(
            "choice {} does not exist in this poll",
            choice
        )));
    }

    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("voted", env.message.sender.to_string())],
        data: Some(to_binary(&ResponseStatus::Success)?),
    })
}

pub fn update_voting_power<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    voter: HumanAddr,
    new_power: u128,
) -> StdResult<HandleResponse> {
    require_vote_ongoing(deps, &env)?; // TODO Should maybe just return an empty Ok(HandleResponse) here?

    let owner: HumanAddr = TypedStore::attach(&deps.storage).load(OWNER_KEY)?;
    if env.message.sender != owner {
        return Err(StdError::unauthorized());
    }

    let vote: Vote = TypedStore::attach(&deps.storage).load(voter.0.as_bytes())?;
    let mut tally: Tally = TypedStoreMut::attach(&mut deps.storage).load(TALLY_KEY)?;
    if let Some(choice_tally) = tally.get_mut(&vote.choice) {
        *choice_tally = *choice_tally - vote.voting_power + new_power;

        TypedStoreMut::attach(&mut deps.storage).store(TALLY_KEY, &tally)?;
        store_vote(deps, voter.clone(), vote.choice, vote.voting_power)?;
    } else {
        // Shouldn't really happen since user already voted, but just in case
        return Err(StdError::generic_err(format!(
            "choice {} does not exist in this poll",
            vote.choice
        )));
    }

    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("voting_power_updated", voter.to_string())],
        data: Some(to_binary(&ResponseStatus::Success)?),
    })
}

fn store_vote<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    voter: HumanAddr,
    choice: u8,
    voting_power: u128,
) -> StdResult<()> {
    TypedStoreMut::attach(&mut deps.storage).store(
        // TODO: We might want to iterate over every voter at some point (or e.g. return a list of voters).
        // TODO: In that case we'd want to store it differently
        // TODO: As an alternative, someone can just look for addresses which interacted with this contract
        voter.0.as_bytes(),
        &Vote {
            choice,
            voting_power,
        },
    )?;

    Ok(())
}

fn require_vote_ongoing<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
) -> StdResult<()> {
    if is_ended(deps, &env)? {
        return Err(StdError::generic_err("vote has ended"));
    }

    Ok(())
}

fn require_vote_ended<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
) -> StdResult<()> {
    if !is_ended(deps, &env)? {
        return Err(StdError::generic_err("vote hasn't ended yet"));
    }

    Ok(())
}

fn is_ended<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: &Env,
) -> StdResult<bool> {
    let config: StoredPollConfig = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    Ok(env.block.time > config.end_timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coins, from_binary, StdError};
}
