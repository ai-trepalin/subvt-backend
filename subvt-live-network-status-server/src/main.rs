//! See `./lib.rs` for details.

use lazy_static::lazy_static;
use subvt_live_network_status_server::LiveNetworkStatusServer;
use subvt_service_common::Service;

lazy_static! {
    static ref SERVICE: LiveNetworkStatusServer = LiveNetworkStatusServer::default();
}

#[tokio::main]
async fn main() {
    SERVICE.start().await;
}
