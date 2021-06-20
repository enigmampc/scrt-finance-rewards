use crate::state::CONFIG_KEY;
use cosmwasm_std::{
    to_binary, Api, Extern, HumanAddr, Querier, QueryRequest, StdError, StdResult, Storage,
    WasmQuery,
};
use scrt_finance::types::SecretContract;
use secret_toolkit::storage::TypedStore;

// TODO: finish implementing when staking pool contract is done
pub fn query_staking_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<u128> {
    // let staking_pool: SecretContract = TypedStore::attach(&deps.storage).load(STAKING_POOL_KEY)?;
    //
    // let response = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
    //     callback_code_hash: staking_pool.contract_hash,
    //     contract_addr: staking_pool.address,
    //     msg: unimplemented!(),
    // }))?;
    unimplemented!()
}
