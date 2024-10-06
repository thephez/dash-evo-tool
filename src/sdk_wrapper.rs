use crate::config::Config;
use crate::logging::initialize_logger;
use dash_sdk::{RequestSettings, Sdk, SdkBuilder}; // Adjust imports
use dpp::version::PlatformVersion;
use std::time::Duration;
use tracing::info;

pub fn initialize_sdk(config: &Config) -> Sdk {
    initialize_logger();

    // Setup Platform SDK
    let address_list = config.dapi_address_list();
    let request_settings = RequestSettings {
        connect_timeout: Some(Duration::from_secs(10)),
        timeout: Some(Duration::from_secs(10)),
        retries: None,
        ban_failed_address: Some(false),
    };

    let sdk = SdkBuilder::new(address_list)
        .with_version(PlatformVersion::get(1).unwrap())
        .with_network(config.core_network())
        .with_core(
            &config.core_host,
            config.core_rpc_port,
            &config.core_rpc_user,
            &config.core_rpc_password,
        )
        .with_settings(request_settings)
        .build()
        .expect("Failed to build SDK");

    info!("SDK initialized successfully");

    sdk
}
