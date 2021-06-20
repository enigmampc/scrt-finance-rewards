use crate::msg::{FinalizeAnswer, HandleMsg, QueryAnswer, QueryMsg, ResponseStatus};
use crate::querier::query_staking_balance;
use crate::state::{
    read_vote, store_vote, StoredPollConfig, Vote, CONFIG_KEY, METADATA_KEY, OWNER_KEY,
    STAKING_POOL_KEY, TALLY_KEY,
};
use cosmwasm_std::{
    log, to_binary, Api, Binary, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier,
    StdError, StdResult, Storage, Uint128,
};
use scrt_finance::secret_vote_types::PollInitMsg;
use scrt_finance::types::SecretContract;
use secret_toolkit::snip20;
use secret_toolkit::snip20::balance_query;
use secret_toolkit::storage::{TypedStore, TypedStoreMut};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: PollInitMsg,
) -> StdResult<InitResponse> {
    let owner = env.message.sender;
    TypedStoreMut::attach(&mut deps.storage).store(OWNER_KEY, &owner)?; // This is in fact the factory contract
    TypedStoreMut::attach(&mut deps.storage).store(METADATA_KEY, &msg.metadata)?;
    TypedStoreMut::attach(&mut deps.storage).store(STAKING_POOL_KEY, &msg.staking_pool)?;

    if msg.choices.len() < 2 {
        return Err(StdError::generic_err(
            "you have to provide at least two choices",
        ));
    }
    // Sanity checks to prevent starting a new poll by mistake
    if msg.metadata.title.len() < 2 {
        return Err(StdError::generic_err(
            "poll title must be at least 2 characters long",
        ));
    }
    if msg.metadata.description.len() < 3 {
        return Err(StdError::generic_err(
            "poll description must be at least 2 characters long",
        ));
    }

    let tally: Vec<u128> = vec![0; msg.choices.len()];
    TypedStoreMut::attach(&mut deps.storage).store(TALLY_KEY, &tally)?;

    let ending = env.block.time + msg.config.duration;
    TypedStoreMut::attach(&mut deps.storage).store(
        CONFIG_KEY,
        &StoredPollConfig {
            end_timestamp: ending,
            quorum: msg.config.quorum,
            min_threshold: msg.config.min_threshold,
            choices: msg.choices,
            ended: false,
            valid: false,
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
        HandleMsg::Finalize {} => finalize(deps, env),
    }
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Choices {} => query_choices(deps),
        QueryMsg::HasVoted { voter } => query_has_voted(deps, voter),
        QueryMsg::Tally {} => query_tally(deps),
        QueryMsg::Vote { voter, key } => query_vote(deps, voter, key),
        QueryMsg::VoteInfo {} => query_vote_info(deps),
    }
}

// Handle

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
    require_vote_ongoing(deps)?;

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

pub fn finalize<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let mut config: StoredPollConfig = TypedStoreMut::attach(&mut deps.storage).load(CONFIG_KEY)?;
    if config.end_timestamp < env.block.time {
        return Err(StdError::generic_err("vote has not ended yet"));
    }

    config.ended = true;

    let tally: Vec<u128> = TypedStore::attach(&deps.storage).load(TALLY_KEY)?;

    // Validation tests
    let sefi_balance = query_staking_balance(deps)?;
    let total_vote_count: u128 = tally.iter().sum();
    let participation = 100 * total_vote_count / sefi_balance; // This should give a percentage integer X/100%
    if participation > config.quorum as u128 {
        config.valid = true;
    }
    if let Some(winning_choice) = tally.iter().max() {
        config.valid = config.valid && (*winning_choice > config.min_threshold as u128)
    } else {
        return Err(StdError::generic_err("storage is corrupted")); // iter().max() returns `None` only when the Vec is empty
    }

    TypedStoreMut::attach(&mut deps.storage).store(CONFIG_KEY, &config)?;
    Ok(HandleResponse {
        messages: vec![],
        log: vec![],
        data: Some(to_binary(&FinalizeAnswer {
            valid: config.valid,
            choices: config.choices,
            tally,
        })?),
    })
}

// Query

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

pub fn query_tally<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Binary> {
    require_vote_ended_and_valid(deps)?; // Hopefully this provide a good enough anonymity set to resist offline attacks

    let tally: Vec<u128> = TypedStore::attach(&deps.storage).load(TALLY_KEY)?;
    let config: StoredPollConfig = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;
    Ok(to_binary(&QueryAnswer::Tally {
        choices: config.choices,
        tally,
    })?)
}

pub fn query_vote<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    voter: HumanAddr,
    key: String,
) -> StdResult<Binary> {
    let staking_pool: SecretContract = TypedStore::attach(&deps.storage).load(STAKING_POOL_KEY)?;
    balance_query(
        &deps.querier,
        voter.clone(),
        key,
        256,
        staking_pool.contract_hash,
        staking_pool.address,
    )?; // Balance doesn't matter, we're just verifying the viewing key

    let vote: Vote = TypedStore::attach(&deps.storage).load(voter.0.as_bytes())?;
    Ok(to_binary(&QueryAnswer::Vote {
        choice: vote.choice,
        voting_power: Uint128(vote.voting_power),
    })?)
}

// Helper functions

fn update_vote<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    voter: &HumanAddr,
    previous_vote: Option<Vote>,
    new_vote: Vote,
) -> StdResult<()> {
    let mut tally: Vec<u128> = TypedStoreMut::attach(&mut deps.storage).load(TALLY_KEY)?;

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
    store_vote(deps, voter, new_vote.choice, new_vote.voting_power)?; // This also discards the old vote

    Ok(())
}

fn require_vote_ongoing<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
) -> StdResult<()> {
    let config: StoredPollConfig = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;

    if config.ended {
        return Err(StdError::generic_err("vote has ended"));
    }

    Ok(())
}

fn require_vote_ended_and_valid<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<()> {
    let config: StoredPollConfig = TypedStore::attach(&deps.storage).load(CONFIG_KEY)?;

    if !config.ended {
        return Err(StdError::generic_err("vote hasn't ended yet"));
    } else if !config.valid {
        return Err(StdError::generic_err("vote hasn't passed quorum"));
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
