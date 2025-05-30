mod client;
mod config;
mod handler;

use crate::client::MyMiningClient;
use crate::config::MyMiningClientConfig;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the TOML configuration file
    #[arg(short, long)]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().init();

    // Parse command line arguments
    let args = Args::parse();

    // Load configuration from file
    let config = MyMiningClientConfig::from_file(args.config)?;

    // Create and start the client
    let mut client = MyMiningClient::new(config).await?;

    // Start the client service
    client.start().await?;

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;

    // Shutdown the client
    client.shutdown().await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::client::MyMiningClient;
    use crate::config::MyMiningClientConfig;
    use integration_tests_sv2::{
        interceptor::MessageDirection, start_pool, start_sniffer, start_template_provider,
    };
    use roles_logic_sv2::common_messages_sv2::*;
    use roles_logic_sv2::mining_sv2::*;
    #[tokio::test]
    async fn test_mining_client_standard_channel() {
        tracing_subscriber::fmt().init();

        let (_tp, tp_addr) = start_template_provider(None);
        let (_pool, pool_addr) = start_pool(Some(tp_addr)).await;
        let (sniffer, sniffer_addr) = start_sniffer("", pool_addr, false, vec![]);

        // Give sniffer time to initialize
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let config = MyMiningClientConfig {
            server_addr: sniffer_addr,
            auth_pk: None,
            extranonce_rolling: false, // standard jobs
            user_identity: "test".to_string(),
        };

        let mut client = MyMiningClient::new(config).await.unwrap();
        client.start().await.unwrap();

        sniffer
            .wait_for_message_type(MessageDirection::ToUpstream, MESSAGE_TYPE_SETUP_CONNECTION)
            .await;
        sniffer
            .wait_for_message_type(
                MessageDirection::ToDownstream,
                MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
            )
            .await;
        sniffer
            .wait_for_message_type(
                MessageDirection::ToUpstream,
                MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
            )
            .await;
        sniffer
            .wait_for_message_type(
                MessageDirection::ToDownstream,
                MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL_SUCCESS,
            )
            .await;
        sniffer
            .wait_for_message_type(MessageDirection::ToDownstream, MESSAGE_TYPE_NEW_MINING_JOB)
            .await;
        sniffer
            .wait_for_message_type(
                MessageDirection::ToDownstream,
                MESSAGE_TYPE_MINING_SET_NEW_PREV_HASH,
            )
            .await;
    }
}
