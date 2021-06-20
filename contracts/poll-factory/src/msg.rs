use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use scrt_finance::secret_vote_types::{PollConfig, PollMetadata};
use scrt_finance::types::SecretContract;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InitMsg {
    pub poll_code_id: u64,
    pub staking_pool: SecretContract,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    NewPoll {
        poll_metadata: PollMetadata,
        poll_config: Option<PollConfig>,
        poll_choices: Vec<String>,
    },

    // Staking contrat callback
    UpdateVotingPower {
        voter: HumanAddr,
        new_power: Uint128,
    },

    // Admin
    UpdatePollCodeId {
        new_id: u64,
    },

    UpdateDefaultPollConfig {
        duration: Option<u64>,     // In seconds
        quorum: Option<u8>,        // X/100% (percentage)
        min_threshold: Option<u8>, // X/100% (percentage)
    },
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryAnswer {}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    Success,
    Failure,
}
