use crate::types::SecretContract;
use cosmwasm_std::HumanAddr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PollConfig {
    pub duration: u64,     // In seconds
    pub quorum: u8,        // X/100% (percentage)
    pub min_threshold: u8, // X/100% (percentage)
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PollMetadata {
    pub title: String,
    pub description: String,
    pub author: HumanAddr,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PollInitMsg {
    pub metadata: PollMetadata,
    pub config: PollConfig,
    pub choices: Vec<String>,
    pub staking_pool: SecretContract,
}
