//! Logging configuration and initializer.

use env_logger::{Builder, Env, Target, WriteStyle};
use log::LevelFilter;
use std::str::FromStr;

/// Initializes the logging facade using the application configuration reference.
pub fn init(config: &subvt_config::Config) {
    let other_modules_log_level = LevelFilter::from_str(config.log.other_level.as_str())
        .expect("Cannot read log level configuration for outside modules.");
    let log_level = LevelFilter::from_str(config.log.subvt_level.as_str())
        .expect("Cannot read log level configuration for SubVT modules.");
    let mut builder = Builder::from_env(Env::default());
    builder.target(Target::Stdout);
    builder.filter(None, other_modules_log_level);
    // include all executable SubVT modules here
    builder.filter(Some("subvt_app_service"), log_level);
    builder.filter(Some("subvt_block_processor"), log_level);
    builder.filter(Some("subvt_live_network_status_server"), log_level);
    builder.filter(Some("subvt_live_network_status_updater"), log_level);
    builder.filter(Some("subvt_notification_generator"), log_level);
    builder.filter(Some("subvt_notification_sender"), log_level);
    builder.filter(Some("subvt_onekv_updater"), log_level);
    builder.filter(Some("subvt_persistence"), log_level);
    builder.filter(Some("subvt_report_service"), log_level);
    builder.filter(Some("subvt_substrate_client"), log_level);
    builder.filter(Some("subvt_telemetry_processor"), log_level);
    builder.filter(Some("subvt_thousand_validators_updater"), log_level);
    builder.filter(Some("subvt_types"), log_level);
    builder.filter(Some("subvt_validator_details_server"), log_level);
    builder.filter(Some("subvt_validator_list_server"), log_level);
    builder.filter(Some("subvt_validator_list_updater"), log_level);
    builder.write_style(WriteStyle::Always);
    builder.init();
}
