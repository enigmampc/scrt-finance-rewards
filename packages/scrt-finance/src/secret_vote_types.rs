use crate::types::SecretContract;
use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PollContract {
    pub code_id: u64,
    pub code_hash: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone)]
pub struct PollConfig {
    pub duration: u64,     // In seconds
    pub quorum: u8,        // X/100% (percentage)
    pub min_threshold: u8, // X/100% (percentage)
}

#[derive(Serialize, Deserialize, JsonSchema, Clone)]
pub struct PollMetadata {
    pub title: String,
    pub description: String,
    pub author: Option<HumanAddr>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PollInitMsg {
    pub metadata: PollMetadata,
    pub config: PollConfig,
    pub choices: Vec<String>,
    pub staking_pool: SecretContract,
    pub init_hook: Option<InitHook>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PollHandleMsg {
    Vote {
        choice: u8, // Arbitrary id that is given by the contract
        staking_pool_viewing_key: String,
    },
    UpdateVotingPower {
        voter: HumanAddr,
        new_power: Uint128,
    },
    Finalize {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitHook {
    pub contract_addr: HumanAddr,
    pub code_hash: String,
    pub msg: Binary,
}
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PollFactoryHandleMsg {
    NewPoll {
        poll_metadata: PollMetadata,
        poll_config: Option<PollConfig>,
        poll_choices: Vec<String>,
        pool_viewing_key: String,
    },

    // Staking contrat callback
    UpdateVotingPower {
        voter: HumanAddr,
        new_power: Uint128,
    },

    // Poll contract callback
    RegisterForUpdates {
        challenge: String,
        end_time: u64,
    },

    // Admin
    UpdateDefaultPollConfig {
        duration: Option<u64>,     // In seconds
        quorum: Option<u8>,        // X/100% (percentage)
        min_threshold: Option<u8>, // X/100% (percentage)
    },
    UpdateConfig {
        new_poll_code: Option<PollContract>,
        new_staking_pool: Option<SecretContract>,
        new_min_stake_amount: Option<Uint128>,
    },
    ChangeAdmin {
        new_admin: HumanAddr,
    },
}