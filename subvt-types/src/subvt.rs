//! SubVT types. These types are used to communicate the network status to
//! the database as a buffer or direct to the applications depending on the
//! implementation.

use serde::{Deserialize, Serialize};
use crate::crypto::AccountId;
use crate::substrate::{
    Account, AccountSummary, Balance, Era, Epoch, Nomination, NominationsSummary, RewardDestination,
    Stake, StakeSummary, ValidatorPreferences,
};
use std::convert::From;
use subvt_proc_macro::Diff;

/// Represents the network's status that changes with every block.
#[derive(Clone, Debug, Diff, Default, Deserialize, Serialize)]
pub struct LiveNetworkStatus {
    pub finalized_block_number: u64,
    pub finalized_block_hash: String,
    pub best_block_number: u64,
    pub best_block_hash: String,
    pub active_era: Era,
    pub current_epoch: Epoch,
    pub active_validator_count: u32,
    pub inactive_validator_count: u32,
    pub last_era_total_reward: Balance,
    pub total_stake: Balance,
    pub return_rate_per_million: u32,
    pub min_stake: Balance,
    pub max_stake: Balance,
    pub average_stake: Balance,
    pub median_stake: Balance,
    pub era_reward_points: u32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LiveNetworkStatusUpdate {
    pub network: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<LiveNetworkStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff_base_block_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<LiveNetworkStatusDiff>,
}

/// Represents an inactive validator, waiting to be in the active set.
#[derive(Clone, Debug, Default, Deserialize, Diff, Eq, Hash, PartialEq, Serialize)]
pub struct InactiveValidator {
    #[diff_key]
    pub account: Account,
    pub controller_account: Account,
    pub preferences: ValidatorPreferences,
    pub self_stake: Stake,
    pub reward_destination: RewardDestination,
    pub next_session_keys: String,
    pub active_next_session: bool,
    pub nominations: Vec<Nomination>,
    pub oversubscribed: bool,
    pub slashed: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Diff, Eq, Hash, PartialEq, Serialize)]
pub struct InactiveValidatorSummary {
    #[diff_key]
    pub account: AccountSummary,
    pub preferences: ValidatorPreferences,
    pub self_stake: StakeSummary,
    pub active_next_session: bool,
    pub nominations: NominationsSummary,
    pub oversubscribed: bool,
    pub slashed: bool,
}

impl From<&InactiveValidator> for InactiveValidatorSummary {
    fn from(validator: &InactiveValidator) -> InactiveValidatorSummary {
        InactiveValidatorSummary {
            account: AccountSummary::from(&validator.account),
            preferences: validator.preferences.clone(),
            self_stake: StakeSummary::from(&validator.self_stake),
            active_next_session: validator.active_next_session,
            nominations: NominationsSummary::from(&validator.nominations),
            oversubscribed: validator.oversubscribed,
            slashed: validator.slashed,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct InactiveValidatorListUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finalized_block_number: Option<u64>,
    pub insert: Vec<InactiveValidatorSummary>,
    pub update: Vec<InactiveValidatorSummaryDiff>,
    pub remove_ids: Vec<AccountId>,
}