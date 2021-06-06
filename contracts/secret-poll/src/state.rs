use cosmwasm_std::HumanAddr;
use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const OWNER_KEY: &[u8] = b"owner";
pub const CHOICE_ID_MAP_KEY: &[u8] = b"choiceidmap";
pub const TALLY_KEY: &[u8] = b"tally";
pub const METADATA_KEY: &[u8] = b"metadata";
pub const CONFIG_KEY: &[u8] = b"config";
pub const STAKING_POOL_KEY: &[u8] = b"stakingpool";

pub type ChoiceIdMap = Vec<(u8, String)>;
pub type Tally = HashMap<u8, u128>;

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
pub struct Vote {
    pub choice: u8,
    pub voting_power: u128,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct StoredPollConfig {
    pub end_timestamp: u64, // In seconds
    pub quorum: u8,         // X/100% (percentage)
    pub min_threshold: u8,  // X/100% (percentage)
}
