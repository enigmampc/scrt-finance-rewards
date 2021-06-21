use crate::state::{ActivePoll, PollContract};
use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use scrt_finance::secret_vote_types::{PollConfig, PollMetadata};
use scrt_finance::types::SecretContract;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InitMsg {
    pub prng_seed: Binary,
    pub poll_contract: PollContract,
    pub staking_pool: SecretContract,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    ActivePolls {
        current_time: u64, /* linux epoch */
    },
    DefaultPollConfig {},
    StakingPool {},
    PollCode {},
    Admin {},
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryAnswer {
    ActivePolls { active_polls: Vec<ActivePoll> },
    DefaultPollConfig { poll_config: PollConfig },
    StakingPool { contract: SecretContract },
    PollCode { contract: PollContract },
    Admin { address: HumanAddr },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Success,
    Failure,
}
