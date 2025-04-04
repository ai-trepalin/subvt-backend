//! Validator list WebSocket server. Operates on the configured port. Serves the inactive validator
//! list if the `--inactive` command-line flag is provided at startup, otherwise serves the active
//! validator list.
//!
//! Supports two RPC methods: `subscribe_validator_list` and `unsubscribe_validator_list`.
//! Gives the complete list at first connection, then publishes only the changed validators' fields
//! after each update from `subvt-validator-list-updater`.
use anyhow::Context;
use async_trait::async_trait;
use bus::Bus;
use clap::{App, Arg};
use jsonrpsee::ws_server::{RpcModule, WsServerBuilder, WsServerHandle};
use lazy_static::lazy_static;
use log::{debug, error, warn};
use std::collections::{hash_map::DefaultHasher, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::{Arc, Mutex, RwLock};
use subvt_config::Config;
use subvt_service_common::Service;
use subvt_types::{
    crypto::AccountId,
    subvt::{ValidatorDetails, ValidatorDetailsDiff, ValidatorListUpdate, ValidatorSummary},
};

lazy_static! {
    static ref CONFIG: Config = Config::default();
}

#[derive(Clone, Debug)]
pub enum BusEvent {
    Update(ValidatorListUpdate),
    Error,
}

#[derive(Default)]
pub struct ValidatorListServer;

impl ValidatorListServer {
    pub async fn run_rpc_server(
        host: &str,
        port: u16,
        validator_map: &Arc<RwLock<HashMap<AccountId, ValidatorDetails>>>,
        bus: &Arc<Mutex<Bus<BusEvent>>>,
    ) -> anyhow::Result<WsServerHandle> {
        let rpc_ws_server = WsServerBuilder::default()
            .max_request_body_size(u32::MAX)
            .build(format!("{}:{}", host, port))
            .await?;
        let mut rpc_module = RpcModule::new(());
        let validator_map = validator_map.clone();
        let bus = bus.clone();
        rpc_module.register_subscription(
            "subscribe_validator_list",
            "subscribe_validator_list",
            "unsubscribe_validator_list",
            move |_params, mut sink, _| {
                debug!("New subscription.");
                let mut bus_receiver = bus.lock().unwrap().add_rx();
                {
                    let validator_summaries: Vec<ValidatorSummary> = {
                        let validator_map = validator_map.read().unwrap();
                        validator_map.iter().map(|value| value.1.into()).collect()
                    };
                    let update = ValidatorListUpdate {
                        insert: validator_summaries,
                        ..Default::default()
                    };
                    let _ = sink.send(&update);
                }
                std::thread::spawn(move || loop {
                    if let Ok(update) = bus_receiver.recv() {
                        match update {
                            BusEvent::Update(update) => {
                                let send_result = sink.send(&update);
                                if let Err(error) = send_result {
                                    debug!("Subscription closed. {:?}", error);
                                    return;
                                } else {
                                    debug!("Published diff.");
                                }
                            }
                            BusEvent::Error => {
                                return;
                            }
                        }
                    }
                });
                Ok(())
            },
        )?;
        Ok(rpc_ws_server.start(rpc_module)?)
    }
}

#[async_trait(?Send)]
impl Service for ValidatorListServer {
    async fn run(&'static self) -> anyhow::Result<()> {
        let matches = App::new("SubVT Validator List Server")
            .version("0.1.0")
            .author("Kutsal Kaan Bilgin <kutsal@helikon.io>")
            .about("Serves the live active or inactive validator list for the SubVT app.")
            .arg(Arg::new("inactive").long("inactive").short('i').help(
                "Active list is served by default. Use this flag to serve the inactive list.",
            ))
            .get_matches();
        let is_active_list = !matches.is_present("inactive");
        let mut last_finalized_block_number = 0;
        let bus = Arc::new(Mutex::new(Bus::new(100)));
        let validator_map = Arc::new(RwLock::new(HashMap::<AccountId, ValidatorDetails>::new()));

        let redis_client = redis::Client::open(CONFIG.redis.url.as_str()).context(format!(
            "Cannot connect to Redis at URL {}.",
            CONFIG.redis.url
        ))?;
        let mut pub_sub_connection = redis_client.get_connection()?;
        let mut pub_sub = pub_sub_connection.as_pubsub();
        pub_sub.subscribe(format!(
            "subvt:{}:validators:publish:finalized_block_number",
            CONFIG.substrate.chain
        ))?;
        let mut data_connection = redis_client.get_connection()?;
        let server_stop_handle = ValidatorListServer::run_rpc_server(
            &CONFIG.rpc.host,
            if is_active_list {
                CONFIG.rpc.active_validator_list_port
            } else {
                CONFIG.rpc.inactive_validator_list_port
            },
            &validator_map,
            &bus,
        )
        .await?;

        let error: anyhow::Error = 'outer: loop {
            let message = pub_sub.get_message();
            if let Err(error) = message {
                break error.into();
            }
            let payload = message.unwrap().get_payload();
            if let Err(error) = payload {
                break error.into();
            }
            let finalized_block_number: u64 = payload.unwrap();
            if last_finalized_block_number == finalized_block_number {
                warn!(
                    "Skip duplicate finalized block #{}.",
                    finalized_block_number
                );
                continue 'outer;
            }
            debug!("New finalized block #{}.", finalized_block_number);
            let prefix = format!(
                "subvt:{}:validators:{}:{}",
                CONFIG.substrate.chain,
                finalized_block_number,
                if is_active_list { "active" } else { "inactive" }
            );
            let validator_account_ids: HashSet<String> = redis::cmd("SMEMBERS")
                .arg(format!("{}:account_id_set", prefix))
                .query(&mut data_connection)
                .context("Can't read validator account ids from Redis.")?;
            debug!(
                "Got {} validator account ids. Checking for changes...",
                validator_account_ids.len()
            );
            let mut update = ValidatorListUpdate {
                finalized_block_number: Some(finalized_block_number),
                ..Default::default()
            };
            {
                // find the ones to remove
                let validator_map = validator_map.read().unwrap();
                for validator_account_id in validator_map.keys() {
                    if !validator_account_ids.contains(&validator_account_id.to_string()) {
                        update.remove_ids.push(validator_account_id.clone());
                    }
                }
            }
            {
                // remove
                let mut validator_map = validator_map.write().unwrap();
                for remove_id in &update.remove_ids {
                    validator_map.remove(remove_id);
                }
            }
            let mut new_validators: Vec<ValidatorDetails> = Vec::new();
            let mut validator_updates: Vec<ValidatorDetailsDiff> = Vec::new();
            {
                // update/insert
                let validator_map = validator_map.read().unwrap();
                for validator_account_id in validator_account_ids {
                    let validator_account_id = AccountId::from_str(&validator_account_id).unwrap();
                    let prefix = format!("{}:validator:{}", prefix, validator_account_id);
                    if let Some(validator) = validator_map.get(&validator_account_id) {
                        // check hash, if different, fetch, calculate and add to list
                        let summary_hash = {
                            let mut hasher = DefaultHasher::new();
                            ValidatorSummary::from(validator).hash(&mut hasher);
                            hasher.finish()
                        };
                        let db_summary_hash: u64 = redis::cmd("GET")
                            .arg(format!("{}:summary_hash", prefix))
                            .query(&mut data_connection)
                            .context("Can't read validator summary hash from Redis.")?;
                        if summary_hash != db_summary_hash {
                            debug!("Summary hash changed for {}.", validator_account_id);
                            let validator_json_string: String = redis::cmd("GET")
                                .arg(prefix)
                                .query(&mut data_connection)
                                .context("Can't read validator JSON string (1) from Redis.")?;
                            let db_validator: ValidatorDetails =
                                serde_json::from_str(&validator_json_string)?;
                            let db_validator_summary: ValidatorSummary =
                                ValidatorSummary::from(&db_validator);
                            let validator_summary: ValidatorSummary = validator.into();
                            update
                                .update
                                .push(validator_summary.get_diff(&db_validator_summary));
                            validator_updates.push(validator.get_diff(&db_validator));
                        }
                    } else {
                        let validator_json_string: String = redis::cmd("GET")
                            .arg(&prefix)
                            .query(&mut data_connection)
                            .context(format!(
                                "Can't read validator JSON string (2) from Redis :: {}",
                                &prefix
                            ))?;
                        let validator_deser_result: serde_json::error::Result<ValidatorDetails> =
                            serde_json::from_str(&validator_json_string);
                        match validator_deser_result {
                            Ok(validator) => {
                                let validator_summary = ValidatorSummary::from(&validator);
                                update.insert.push(validator_summary);
                                new_validators.push(validator);
                            }
                            Err(error) => {
                                break 'outer error.into();
                            }
                        }
                    }
                }
            }
            {
                let mut validator_map = validator_map.write().unwrap();
                for diff in validator_updates {
                    let validator = validator_map.get_mut(&diff.account.id).unwrap();
                    validator.apply_diff(&diff);
                }
                for validator in new_validators {
                    validator_map.insert(validator.account.id.clone(), validator);
                }
            }
            debug!(
                "Completed checks. Remove {} validators. {} new validators. {} updated validators.",
                update.remove_ids.len(),
                update.insert.len(),
                update.update.len(),
            );
            {
                let mut bus = bus.lock().unwrap();
                bus.broadcast(BusEvent::Update(update));
                debug!("Update published to the bus.");
            }
            last_finalized_block_number = finalized_block_number;
        };
        error!("{:?}", error);
        {
            let mut bus = bus.lock().unwrap();
            bus.broadcast(BusEvent::Error);
        }
        debug!("Stopping RPC server...");
        server_stop_handle.stop()?;
        debug!("RPC server fully stopped.");
        Err(error)
    }
}
