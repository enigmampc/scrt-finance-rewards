use crate::msg::{HandleMsg, InitMsg, QueryAnswer, QueryMsg, ResponseStatus};
use crate::state::{
    read_vote, store_vote, ChoiceIdMap, StoredPollConfig, Tally, Vote, CONFIG_KEY, METADATA_KEY,
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
    TypedStoreMut::attach(&mut deps.storage).store(STAKING_POOL_KEY, &msg.staking_pool)?;

    if msg.choices.len() > (u8::MAX) as usize {
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

    let mut tally: Tally = vec![0]; // The actual values will start at 1
    for choice in &choice_id_map {
        tally.insert(choice.0 as usize, 0);
    }
    TypedStoreMut::attach(&mut deps.storage).store(TALLY_KEY, &tally)?;

    let ending = env.block.time + msg.config.duration;
    TypedStoreMut::attach(&mut deps.storage).store(
        CONFIG_KEY,
        &StoredPollConfig {
            end_timestamp: ending,
            quorum: msg.config.quorum,
            min_threshold: msg.config.min_threshold,
            choices: choice_id_map,
            ended: false,
        },
    )?;

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
        QueryMsg::Choices {} => query_choices(deps),
        QueryMsg::HasVoted { voter } => query_has_voted(deps, voter),
        // QueryMsg::Voters { .. } => unimplemented!(),
        QueryMsg::Tally {} => unimplemented!(),
        QueryMsg::Vote { voter, key } => unimplemented!(),
        QueryMsg::VoteInfo {} => query_vote_info(deps),
    }
}

pub fn vote<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    choice: u8,
    key: String,
) -> StdResult<HandleResponse> {
    require_vote_ongoing(deps)?;

    let staking_pool: SecretContract = TypedStore::attach(&deps.storage).load(STAKING_POOL_KEY)?;
    let voting_power = snip20::balance_query(
        &deps.querier,
        env.message.sender.clone(),
        key,
        256,
        staking_pool.contract_hash,
        staking_pool.address,
    )?;

    let prev_vote = read_vote(deps, &env.message.sender);
    update_vote(
        deps,
        &env.message.sender,
        prev_vote.ok(),
        Vote {
            choice,
            voting_power: voting_power.amount.u128(),
        },
    )?;

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
    require_vote_ongoing(deps)?; // TODO Should maybe just return Ok(HandleResponse::Failure) here?

    let owner: HumanAddr = TypedStore::attach(&deps.storage).load(OWNER_KEY)?;
    if env.message.sender != owner {
        return Err(StdError::unauthorized());
    }

    let mut logs = vec![];
    if let Ok(prev_vote) = read_vote(deps, &voter) {
        update_vote(
            deps,
            &voter,
            Some(prev_vote.clone()),
            Vote {
                choice: prev_vote.choice,
                voting_power: new_power,
            },
        )?;

        logs.push(log("voting_power_updated", voter.to_string()));
    }

    Ok(HandleResponse {
        messages: vec![],
        log: logs,
        data: Some(to_binary(&ResponseStatus::Success)?),
    })
}

pub fn query_choices<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Binary> {
    let config: StoredPollConfig = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    Ok(to_binary(&QueryAnswer::Choices {
        choices: config.choices,
    })?)
}

pub fn query_vote_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Binary> {
    let config: StoredPollConfig = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    Ok(to_binary(&QueryAnswer::VoteInfo { vote_info: config })?)
}

pub fn query_has_voted<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    voter: HumanAddr,
) -> StdResult<Binary> {
    let has_voted = read_vote(deps, &voter).is_ok();
    Ok(to_binary(&QueryAnswer::HasVoted { has_voted })?)
}

// pub fn query_tally<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Binary> {
//     require_vote_ended(deps)?;
//
//     let tally: Tally = TypedStore::attach(&deps.storage).load(TALLY_KEY)?;
//     to_binary(&tally.iter().map(||))
//     unimplemented!()
// }

fn update_vote<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    voter: &HumanAddr,
    previous_vote: Option<Vote>,
    new_vote: Vote,
) -> StdResult<()> {
    let mut tally: Tally = TypedStoreMut::attach(&mut deps.storage).load(TALLY_KEY)?;

    if let Some(previous_vote) = previous_vote {
        if let Some(choice_tally) = tally.get_mut(previous_vote.choice as usize) {
            *choice_tally -= previous_vote.voting_power; // Can't underflow, `choice_tally` >= `old_vote.voting_power`
        } else {
            // Shouldn't really happen since user already voted, but just in case
            return Err(StdError::generic_err(format!(
                "previous choice {} does not exist in this poll",
                previous_vote.choice
            )));
        }
    }

    if let Some(choice_tally) = tally.get_mut(new_vote.choice as usize) {
        *choice_tally += new_vote.voting_power; // Can't overflow, `choice_tally` <= `gov_token.total_supply()`
    } else {
        return Err(StdError::generic_err(format!(
            "choice {} does not exist in this poll",
            new_vote.choice
        )));
    }

    TypedStoreMut::attach(&mut deps.storage).store(TALLY_KEY, &tally)?;
    store_vote(deps, voter, new_vote.choice, new_vote.voting_power)?; // This also discards the old vote, if exists

    Ok(())
}

fn require_vote_ongoing<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
) -> StdResult<()> {
    if is_ended(deps)? {
        return Err(StdError::generic_err("vote has ended"));
    }

    Ok(())
}

fn require_vote_ended<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<()> {
    if !is_ended(deps)? {
        return Err(StdError::generic_err("vote hasn't ended yet"));
    }

    Ok(())
}

fn is_ended<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<bool> {
    let config: StoredPollConfig = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    Ok(config.ended)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coins, from_binary, StdError};
}
