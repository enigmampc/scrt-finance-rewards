use crate::msg::{HandleMsg, InitMsg, QueryAnswer, QueryMsg, ResponseStatus};
use crate::querier::query_staking_balance;
use crate::state::{ADMIN_KEY, CONFIG_KEY, STAKING_POOL_KEY};
use cosmwasm_std::{
    log, to_binary, Api, Binary, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier,
    StdError, StdResult, Storage, Uint128,
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
    TypedStoreMut::attach(&mut deps.storage).store(STAKING_POOL_KEY, &msg.staking_pool)?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    unimplemented!()
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coins, from_binary, StdError};

    #[test]
    fn test() {}
}
