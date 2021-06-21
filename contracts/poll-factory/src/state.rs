use cosmwasm_std::{Api, Extern, HumanAddr, Querier, StdResult, Storage};

use schemars::JsonSchema;
use scrt_finance::types::SecretContract;
use serde::{Deserialize, Serialize};

pub const ADMIN_KEY: &[u8] = b"admin";
pub const CONFIG_KEY: &[u8] = b"config";
pub const DEFAULT_POLL_CONFIG_KEY: &[u8] = b"defaultconfig";
pub const CURRENT_CHALLENGE_KEY: &[u8] = b"prngseed";
pub const ACTIVE_POLLS_KEY: &[u8] = b"active_polls";

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub poll_contract: PollContract,
    pub staking_pool: SecretContract,
    pub id_counter: u128,
    pub prng_seed: [u8; 32],
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PollContract {
    pub code_id: u64,
    pub code_hash: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ActivePoll {
    pub address: HumanAddr,
    pub end_time: u64,
}
