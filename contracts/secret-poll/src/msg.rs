use crate::state::StoredPollConfig;
use cosmwasm_std::{HumanAddr, Uint128};
use schemars::JsonSchema;
use scrt_finance::secret_vote_types::{PollMetadata, RevealCommittee};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct FinalizeAnswer {
    pub ended: bool,
    pub valid: Option<bool>,
    pub choices: Option<Vec<String>>,
    pub tally: Option<Vec<u128>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // Public
    Choices {},
    VoteInfo {},
    HasVoted { voter: HumanAddr },
    Tally {},
    NumberOfVoters {},
    RevealCommittee {},
    Revealed {},

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
        metadata: PollMetadata,
        config: StoredPollConfig,
    },
    HasVoted {
        has_voted: bool,
    },
    Tally {
        choices: Vec<String>,
        tally: Vec<Uint128>,
    },
    Vote {
        choice: u8,
        voting_power: Uint128,
    },
    NumberOfVoters {
        count: u64,
    },
    RevealCommittee {
        committee: RevealCommittee,
    },
    Revealed {
        required: u64,
        num_revealed: u64,
        revealed: Vec<HumanAddr>,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Success,
    Failure,
}
