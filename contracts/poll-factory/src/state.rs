use cosmwasm_std::{Api, Extern, HumanAddr, Querier, StdResult, Storage};

use schemars::JsonSchema;
use secret_toolkit::storage::{TypedStore, TypedStoreMut};
use serde::{Deserialize, Serialize};

pub const ADMIN_KEY: &[u8] = b"admin";
pub const CONFIG_KEY: &[u8] = b"config";
pub const STAKING_POOL_KEY: &[u8] = b"stakingpool";
