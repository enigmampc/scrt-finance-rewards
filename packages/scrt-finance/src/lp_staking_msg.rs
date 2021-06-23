use crate::types::{SecretContract, TokenInfo};
use crate::viewing_key::ViewingKey;
use cosmwasm_std::{Binary, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct LPStakingInitMsg {
    pub reward_token: SecretContract,
    pub inc_token: SecretContract,
    pub master: SecretContract,
    pub viewing_key: String,
    pub token_info: TokenInfo,
    pub prng_seed: Binary,
    pub subscribers: Option<Vec<SecretContract>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LPStakingHandleMsg {
    Redeem {
        amount: Option<Uint128>,
    },
    CreateViewingKey {
        entropy: String,
        padding: Option<String>,
    },
    SetViewingKey {
        key: String,
        padding: Option<String>,
    },
    EmergencyRedeem {},

    // Registered commands
    Receive {
        sender: HumanAddr,
        from: HumanAddr,
        amount: Uint128,
        msg: Binary,
    },

    // Admin commands
    StopContract {},
    ResumeContract {},
    ChangeAdmin {
        address: HumanAddr,
    },
    AddSubs {
        contracts: Vec<SecretContract>,
    },
    RemoveSubs {
        contracts: Vec<HumanAddr>,
    },

    // Master callbacks
    NotifyAllocation {
        amount: Uint128,
        hook: Option<Binary>,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LPStakingHandleAnswer {
    Redeem { status: LPStakingResponseStatus },
    CreateViewingKey { key: ViewingKey },
    SetViewingKey { status: LPStakingResponseStatus },
    StopContract { status: LPStakingResponseStatus },
    ResumeContract { status: LPStakingResponseStatus },
    ChangeAdmin { status: LPStakingResponseStatus },
    SetDeadline { status: LPStakingResponseStatus },
    ClaimRewardPool { status: LPStakingResponseStatus },
    EmergencyRedeem { status: LPStakingResponseStatus },
    AddSubs { status: LPStakingResponseStatus },
    RemoveSubs { status: LPStakingResponseStatus },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LPStakingReceiveMsg {
    Deposit {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LPStakingHookMsg {
    Deposit {
        from: HumanAddr,
        amount: Uint128,
    },
    Redeem {
        to: HumanAddr,
        amount: Option<Uint128>,
    },
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LPStakingReceiveAnswer {
    Deposit { status: LPStakingResponseStatus },
    DepositRewards { status: LPStakingResponseStatus },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LPStakingQueryMsg {
    TokenInfo {},
    ContractStatus {},
    RewardToken {},
    IncentivizedToken {},
    Subscribers {},

    // Authenticated
    Rewards {
        address: HumanAddr,
        key: String,
        height: u64,
    },
    Balance {
        address: HumanAddr,
        key: String,
    },
}

impl LPStakingQueryMsg {
    pub fn get_validation_params(&self) -> (&HumanAddr, ViewingKey) {
        match self {
            LPStakingQueryMsg::Rewards { address, key, .. } => (address, ViewingKey(key.clone())),
            LPStakingQueryMsg::Balance { address, key } => (address, ViewingKey(key.clone())),
            _ => panic!("This should never happen"),
        }
    }
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LPStakingQueryAnswer {
    TokenInfo {
        name: String,
        symbol: String,
        decimals: u8,
        total_supply: Option<Uint128>,
    },
    Rewards {
        rewards: Uint128,
    },
    Balance {
        amount: Uint128,
    },
    ContractStatus {
        is_stopped: bool,
    },
    RewardToken {
        token: SecretContract,
    },
    IncentivizedToken {
        token: SecretContract,
    },
    Subscribers {
        contracts: Vec<SecretContract>,
    },

    QueryError {
        msg: String,
    },
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LPStakingResponseStatus {
    Success,
    Failure,
}
