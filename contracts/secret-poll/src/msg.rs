use crate::state::StoredPollConfig;
use cosmwasm_std::{HumanAddr, Uint128};
use schemars::JsonSchema;
use scrt_finance::secret_vote_types::PollMetadata;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct FinalizeAnswer {
    pub valid: bool,
    pub choices: Vec<String>,
    pub tally: Vec<u128>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // Public
    Choices {},
    VoteInfo {},
    HasVoted { voter: HumanAddr },
    Tally {},
    VoteConfig {},
    NumberOfVoters {},

    // Authenticated
    Vote { voter: HumanAddr, key: String },
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryAnswer {
    Choices {
        choices: Vec<String>,
    },
    VoteInfo {
        info: PollMetadata,
    },
    HasVoted {
        has_voted: bool,
    },
    Tally {
        choices: Vec<String>,
        tally: Vec<u128>,
    },
    VoteConfig {
        vote_config: StoredPollConfig,
    },
    Vote {
        choice: u8,
        voting_power: Uint128,
    },
    NumberOfVoters {
        count: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Success,
    Failure,
}
