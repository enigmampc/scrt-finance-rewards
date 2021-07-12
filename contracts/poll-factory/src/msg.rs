use crate::state::ActivePoll;
use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use scrt_finance::secret_vote_types::{PollConfig, PollContract, RevealCommittee};
use scrt_finance::types::SecretContract;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InitMsg {
    pub prng_seed: Binary,
    pub poll_contract: PollContract,
    pub staking_pool: SecretContract,
    pub default_poll_config: PollConfig,
    pub min_staked: Uint128,
    pub reveal_com: RevealCommittee,
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
    RevealCommittee {},
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryAnswer {
    ActivePolls { active_polls: Vec<ActivePoll> },
    DefaultPollConfig { poll_config: PollConfig },
    StakingPool { contract: SecretContract },
    PollCode { contract: PollContract },
    Admin { address: HumanAddr },
    RevealCommittee { committee: RevealCommittee },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Success,
    Failure,
}
