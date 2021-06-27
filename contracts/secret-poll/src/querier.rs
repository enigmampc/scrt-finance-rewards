use crate::state::STAKING_POOL_KEY;
use cosmwasm_std::{
    to_binary, Api, Extern, Querier, QueryRequest, StdError, StdResult, Storage, WasmQuery,
};
use scrt_finance::lp_staking_msg::{LPStakingQueryAnswer, LPStakingQueryMsg};
use scrt_finance::types::SecretContract;
use secret_toolkit::storage::TypedStore;

pub fn query_staking_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<u128> {
    let staking_pool: SecretContract = TypedStore::attach(&deps.storage).load(STAKING_POOL_KEY)?;

    let response = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        callback_code_hash: staking_pool.contract_hash,
        contract_addr: staking_pool.address,
        msg: to_binary(&LPStakingQueryMsg::TotalLocked {})?,
    }))?;

    match response {
        LPStakingQueryAnswer::TotalLocked { amount } => Ok(amount.u128()),
        _ => Err(StdError::generic_err(
            "something is wrong with the lp staking contract..",
        )),
    }
}
