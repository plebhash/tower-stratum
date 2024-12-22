mod client;
mod config;
mod handler;

use crate::client::MyTemplateDistributionClient;
use crate::config::MyTemplateDistributionClientConfig;
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
    let config = MyTemplateDistributionClientConfig::from_file(args.config)?;

    // Create and start the client
    let mut client = MyTemplateDistributionClient::new(config).await?;

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
    use crate::client::MyTemplateDistributionClient;
    use crate::config::MyTemplateDistributionClientConfig;
    use const_sv2::{
        MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS, MESSAGE_TYPE_NEW_TEMPLATE,
        MESSAGE_TYPE_SET_NEW_PREV_HASH, MESSAGE_TYPE_SETUP_CONNECTION,
        MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS,
    };
    use integration_tests_sv2::{
        sniffer::MessageDirection, start_sniffer, start_template_provider,
    };

    #[tokio::test]
    async fn test_template_distribution_client() {
        tracing_subscriber::fmt().init();

        let (_tp, tp_addr) = start_template_provider(None);

        let (sniffer, sniffer_addr) = start_sniffer("".to_string(), tp_addr, false, None).await;

        let config = MyTemplateDistributionClientConfig {
            server_addr: sniffer_addr,
            auth_pk: None,
            coinbase_output_max_additional_size: 1,
            coinbase_output_max_additional_sigops: 1,
        };

        // Give sniffer time to initialize
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let mut client = MyTemplateDistributionClient::new(config).await.unwrap();

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
                MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS,
            )
            .await;

        sniffer
            .wait_for_message_type(MessageDirection::ToDownstream, MESSAGE_TYPE_NEW_TEMPLATE)
            .await;
        sniffer
            .wait_for_message_type(
                MessageDirection::ToDownstream,
                MESSAGE_TYPE_SET_NEW_PREV_HASH,
            )
            .await;

        client.shutdown().await;
    }
}
