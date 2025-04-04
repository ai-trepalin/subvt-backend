//! Updates the Redis database with the complete validator list after every block.
//! Subscribes to the new blocks using the Substrate client in `subvt-substrate-client`.
use anyhow::Context;
use async_lock::RwLock;
use async_trait::async_trait;
use lazy_static::lazy_static;
use log::{debug, error, trace};
use redis::Pipeline;
use std::collections::{hash_map::DefaultHasher, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use subvt_config::Config;
use subvt_persistence::postgres::network::PostgreSQLNetworkStorage;
use subvt_service_common::Service;
use subvt_substrate_client::SubstrateClient;
use subvt_types::substrate::{BlockHeader, Era};
use subvt_types::subvt::{ValidatorDetails, ValidatorSummary};

lazy_static! {
    static ref CONFIG: Config = Config::default();
}

const HISTORY_BLOCK_DEPTH: u64 = 3;

#[derive(Default)]
pub struct ValidatorListUpdater;

impl ValidatorListUpdater {
    async fn update_redis(
        active_era: &Era,
        processed_block_numbers: &Arc<RwLock<Vec<u64>>>,
        finalized_block_number: u64,
        validators: &[ValidatorDetails],
    ) -> anyhow::Result<()> {
        // get redis connection
        let redis_client = redis::Client::open(CONFIG.redis.url.as_str())?;
        let mut redis_connection = redis_client.get_connection().context(format!(
            "Cannot connect to Redis at URL {}.",
            CONFIG.redis.url
        ))?;
        let prefix = format!(
            "subvt:{}:validators:{}",
            CONFIG.substrate.chain, finalized_block_number
        );
        // prepare first command pipeline
        let mut redis_cmd_pipeline = Pipeline::new();
        // delete history
        {
            debug!("Clean Redis history.");
            let mut processed_block_numbers = processed_block_numbers.write().await;
            let to_delete: Vec<u64> = processed_block_numbers
                .iter()
                .cloned()
                .take(
                    processed_block_numbers
                        .len()
                        .saturating_sub(HISTORY_BLOCK_DEPTH as usize),
                )
                .collect();
            for delete in to_delete {
                let keys: Vec<String> = redis::cmd("KEYS")
                    .arg(format!(
                        "subvt:{}:validators:{}:*",
                        CONFIG.substrate.chain, delete
                    ))
                    .query(&mut redis_connection)?;
                debug!("Delete {} records for block #{}.", keys.len(), delete);
                for key in keys {
                    redis_cmd_pipeline.cmd("DEL").arg(key);
                }
                processed_block_numbers.remove(0);
            }
        }
        let active_account_ids: HashSet<String> = validators
            .iter()
            .filter_map(|validator| {
                if validator.is_active {
                    Some(validator.account.id.to_string())
                } else {
                    None
                }
            })
            .collect();
        let inactive_account_ids: HashSet<String> = validators
            .iter()
            .filter_map(|validator| {
                if !validator.is_active {
                    Some(validator.account.id.to_string())
                } else {
                    None
                }
            })
            .collect();
        redis_cmd_pipeline
            .cmd("SADD")
            .arg(format!("{}:active:{}", prefix, "account_id_set"))
            .arg(active_account_ids);
        redis_cmd_pipeline
            .cmd("SADD")
            .arg(format!("{}:inactive:{}", prefix, "account_id_set"))
            .arg(inactive_account_ids);
        // each validator
        redis_cmd_pipeline.cmd("MSET");
        // set era
        redis_cmd_pipeline
            .arg(format!("{}:active_era", prefix))
            .arg(serde_json::to_string(active_era)?);
        // set validator details
        for validator in validators {
            let validator_prefix = format!(
                "{}:{}:validator:{}",
                prefix,
                if validator.is_active {
                    "active"
                } else {
                    "inactive"
                },
                validator.account.id
            );
            // calculate hash
            let hash = {
                let mut hasher = DefaultHasher::new();
                validator.hash(&mut hasher);
                hasher.finish()
            };
            // calculate summary hash
            let summary_hash = {
                let mut hasher = DefaultHasher::new();
                ValidatorSummary::from(validator).hash(&mut hasher);
                hasher.finish()
            };
            let validator_json_string = serde_json::to_string(validator)?;
            redis_cmd_pipeline
                .arg(format!("{}:hash", validator_prefix))
                .arg(hash)
                .arg(format!("{}:summary_hash", validator_prefix))
                .arg(summary_hash)
                .arg(validator_prefix)
                .arg(validator_json_string);
        }
        // publish event
        redis_cmd_pipeline
            .cmd("PUBLISH")
            .arg(format!(
                "subvt:{}:validators:publish:finalized_block_number",
                CONFIG.substrate.chain
            ))
            .arg(finalized_block_number);
        redis_cmd_pipeline
            .query(&mut redis_connection)
            .context("Error while setting Redis validators.")?;
        let mut processed_block_numbers = processed_block_numbers.write().await;
        processed_block_numbers.push(finalized_block_number);
        Ok(())
    }

    async fn fetch_and_update_validator_list(
        client: &SubstrateClient,
        postgres: &PostgreSQLNetworkStorage,
        processed_block_numbers: &Arc<RwLock<Vec<u64>>>,
        finalized_block_header: &BlockHeader,
    ) -> anyhow::Result<Vec<ValidatorDetails>> {
        let finalized_block_number = finalized_block_header
            .get_number()
            .context("Error while extracting finalized block number.")?;
        debug!("Process new finalized block #{}.", finalized_block_number);
        let finalized_block_hash = client
            .get_block_hash(finalized_block_number)
            .await
            .context("Error while fetching finalized block hash.")?;
        let active_era = client.get_active_era(&finalized_block_hash).await?;
        // validator account ids
        let mut validators = client
            .get_all_validators(finalized_block_hash.as_str(), &active_era)
            .await
            .context("Error while getting validators.")?;
        // enrich data with data from the relational database
        debug!("Get RDB content.");
        for validator in validators.iter_mut() {
            let db_validator_info = postgres
                .get_validator_info(
                    &finalized_block_hash,
                    &validator.account.id,
                    validator.is_active,
                    active_era.index,
                )
                .await?;
            validator.account.discovered_at = db_validator_info.discovered_at;
            validator.account.killed_at = db_validator_info.killed_at;
            validator.slash_count = db_validator_info.slash_count;
            validator.offline_offence_count = db_validator_info.offline_offence_count;
            validator.active_era_count = db_validator_info.active_era_count;
            validator.inactive_era_count = db_validator_info.inactive_era_count;
            validator.total_reward_points = db_validator_info.total_reward_points;
            validator.unclaimed_era_indices = db_validator_info.unclaimed_era_indices.clone();
            validator.blocks_authored = db_validator_info.blocks_authored;
            validator.reward_points = db_validator_info.reward_points;
            validator.heartbeat_received = db_validator_info.heartbeat_received;
            validator.onekv_candidate_record_id = db_validator_info.onekv_candidate_record_id;
            validator.onekv_rank = db_validator_info.onekv_rank;
            validator.onekv_is_valid = db_validator_info.onekv_is_valid;
        }
        debug!("Got RDB content. Update Redis.");
        let start = std::time::Instant::now();
        ValidatorListUpdater::update_redis(
            &active_era,
            processed_block_numbers,
            finalized_block_number,
            &validators,
        )
        .await?;
        let elapsed = start.elapsed();
        debug!("Redis updated. Took {} ms.", elapsed.as_millis());
        Ok(validators)
    }
}

#[async_trait(?Send)]
impl Service for ValidatorListUpdater {
    async fn run(&'static self) -> anyhow::Result<()> {
        loop {
            let postgres = Arc::new(
                PostgreSQLNetworkStorage::new(&CONFIG, CONFIG.get_network_postgres_url()).await?,
            );
            let substrate_client = Arc::new(SubstrateClient::new(&CONFIG).await?);
            let is_busy = Arc::new(AtomicBool::new(false));
            let processed_block_numbers: Arc<RwLock<Vec<u64>>> = Arc::new(RwLock::new(Vec::new()));
            // clean Redis history
            {
                debug!("Clean Redis history.");
                let redis_client = redis::Client::open(CONFIG.redis.url.as_str())?;
                let mut connection = redis_client.get_connection().context(format!(
                    "Cannot connect to Redis at URL {}.",
                    CONFIG.redis.url
                ))?;
                let keys: Vec<String> = redis::cmd("KEYS")
                    .arg(format!("subvt:{}:*", CONFIG.substrate.chain))
                    .query(&mut connection)?;
                let mut redis_cmd_pipeline = Pipeline::new();
                for key in keys {
                    redis_cmd_pipeline.cmd("DEL").arg(key);
                }
                redis_cmd_pipeline.query(&mut connection)?;
            }
            substrate_client.subscribe_to_finalized_blocks(|finalized_block_header| {
                let finalized_block_number = match finalized_block_header.get_number() {
                    Ok(block_number) => block_number,
                    Err(_) => return error!("Cannot get block number for header: {:?}", finalized_block_header)
                };
                if is_busy.load(Ordering::SeqCst) {
                    trace!("Busy processing a past block. Skip block #{}.", finalized_block_number);
                    return;
                }
                is_busy.store(true, Ordering::SeqCst);
                let processed_block_numbers = processed_block_numbers.clone();
                let substrate_client = Arc::clone(&substrate_client);
                let postgres = postgres.clone();
                let is_busy = Arc::clone(&is_busy);
                tokio::spawn(async move {
                    let update_result = ValidatorListUpdater::fetch_and_update_validator_list(
                        &substrate_client,
                        &postgres,
                        &processed_block_numbers,
                        &finalized_block_header,
                    ).await;
                    if let Err(error) = update_result {
                        error!("{:?}", error);
                        error!(
                            "Validator list update failed for block #{}. Will try again with the next block.",
                            finalized_block_header.get_number().unwrap_or(0),
                        );
                    }
                    is_busy.store(false, Ordering::SeqCst);
                });
            }).await?;
            let delay_seconds = CONFIG.common.recovery_retry_seconds;
            error!(
                "New block subscription exited. Will refresh connection and subscription after {} seconds.",
                delay_seconds
            );
            std::thread::sleep(std::time::Duration::from_secs(delay_seconds));
        }
    }
}
