use cosmwasm_std::{Api, Extern, HumanAddr, Querier, StdResult, Storage};
use schemars::JsonSchema;
use scrt_finance::secret_vote_types::RevealCommittee;
use secret_toolkit::storage::{TypedStore, TypedStoreMut};
use serde::{Deserialize, Serialize};

pub const OWNER_KEY: &[u8] = b"owner";
pub const TALLY_KEY: &[u8] = b"tally";
pub const METADATA_KEY: &[u8] = b"metadata";
pub const CONFIG_KEY: &[u8] = b"config";
pub const STAKING_POOL_KEY: &[u8] = b"stakingpool";
pub const NUM_OF_VOTERS_KEY: &[u8] = b"numofvoters";
pub const REVEAL_CONFIG: &[u8] = b"revealconfig";

#[derive(Serialize, Deserialize, JsonSchema, Clone)]
pub struct Vote {
    pub choice: u8,
    pub voting_power: u128,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct StoredPollConfig {
    pub end_timestamp: u64, // In seconds
    pub quorum: u8,         // X/100% (percentage)
    pub min_threshold: u8,  // X/100% (percentage)
    pub choices: Vec<String>,
    pub ended: bool,
    pub valid: bool,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct StoredRevealConfig {
    pub committee: RevealCommittee,
    pub num_revealed: u64,
    pub revealed: Vec<HumanAddr>,
}

pub fn store_vote<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    voter: &HumanAddr,
    choice: u8,
    voting_power: u128,
) -> StdResult<()> {
    TypedStoreMut::attach(&mut deps.storage).store(
        voter.0.as_bytes(),
        &Vote {
            choice,
            voting_power,
        },
    )?;

    Ok(())
}

pub fn read_vote<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    voter: &HumanAddr,
) -> StdResult<Vote> {
    Ok(TypedStore::attach(&deps.storage).load(voter.0.as_bytes())?)
}
