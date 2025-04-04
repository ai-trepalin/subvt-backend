//! These types are used when reading Substrate extrinsics from PostgreSQL into the SubVT domain.
use crate::crypto::AccountId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ValidateExtrinsic {
    pub id: u32,
    pub block_hash: String,
    pub extrinsic_index: u32,
    pub is_nested_call: bool,
    pub stash_account_id: AccountId,
    pub controller_account_id: AccountId,
    pub commission_per_billion: u64,
    pub blocks_nominations: bool,
    pub is_successful: bool,
}
