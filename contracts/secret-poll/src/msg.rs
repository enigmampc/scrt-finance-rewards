use crate::state::StoredPollConfig;
use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use scrt_finance::secret_vote_types::{PollConfig, PollMetadata};
use scrt_finance::types::SecretContract;
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
    HasVoted { voter: HumanAddr },
    Tally {},
    VoteInfo {},

    // Authenticated
    Vote { voter: HumanAddr, key: String },
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryAnswer {
    Choices {
        choices: Vec<String>,
    },
    HasVoted {
        has_voted: bool,
    },
    Tally {
        choices: Vec<String>,
        tally: Vec<u128>,
    },
    VoteInfo {
        vote_info: StoredPollConfig,
    },
    Vote {
        choice: u8,
        voting_power: Uint128,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Success,
    Failure,
}
