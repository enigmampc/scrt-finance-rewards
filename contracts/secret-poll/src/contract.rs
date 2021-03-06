use cosmwasm_std::{
    log, to_binary, Api, Binary, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier,
    StdError, StdResult, Storage, Uint128, WasmMsg,
};

use crate::msg::{HandleMsg, InitMsg, QueryMsg, ResponseStatus};
use crate::state::{
    ChoiceIdMap, Tally, Vote, CHOICE_ID_MAP_KEY, CONFIG_KEY, METADATA_KEY, OWNER_KEY,
    STAKING_POOL_KEY, TALLY_KEY,
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
    TypedStoreMut::attach(&mut deps.storage).store(OWNER_KEY, &owner)?;
    TypedStoreMut::attach(&mut deps.storage).store(METADATA_KEY, &msg.metadata)?;
    TypedStoreMut::attach(&mut deps.storage).store(CONFIG_KEY, &msg.config)?;
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
        HandleMsg::Vote { .. } => unimplemented!(),
        HandleMsg::UpdateVotingPower { .. } => unimplemented!(),
        HandleMsg::Finalize { .. } => unimplemented!(),
    }
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    unimplemented!()
}

pub fn vote<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    choice: u8,
    key: String,
) -> StdResult<HandleResponse> {
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

pub fn store_vote<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    voter: HumanAddr,
    choice: u8,
    voting_power: u128,
) -> StdResult<()> {
    TypedStoreMut::attach(&mut deps.storage).store(
        // TODO: We might want to iterate over every voter at some point (or e.g. return a list of voters).
        // TODO: In that case we'd want to store it differently
        voter.0.as_bytes(),
        &Vote {
            choice,
            voting_power,
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coins, from_binary, StdError};
}
