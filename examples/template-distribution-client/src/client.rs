use crate::config::MyTemplateDistributionClientConfig;
use crate::handler::MyTemplateDistributionHandler;
use anyhow::{Result, anyhow};
use tower::{Service, ServiceExt};
use tower_stratum::client::service::Sv2ClientService;
use tower_stratum::client::service::config::Sv2ClientServiceConfig;
use tower_stratum::client::service::config::Sv2ClientServiceTemplateDistributionConfig;
use tower_stratum::client::service::request::RequestToSv2Client;
use tower_stratum::client::service::response::ResponseFromSv2Client;
use tower_stratum::client::service::subprotocols::template_distribution::request::RequestToSv2TemplateDistributionClientService;
use tower_stratum::client::service::subprotocols::template_distribution::response::ResponseToTemplateDistributionTrigger;
use tracing::info;

pub struct MyTemplateDistributionClient {
    sv2_client_service: Sv2ClientService<MyTemplateDistributionHandler>,
    coinbase_output_max_additional_size: u32,
    coinbase_output_max_additional_sigops: u16,
}

impl MyTemplateDistributionClient {
    pub async fn new(config: MyTemplateDistributionClientConfig) -> Result<Self> {
        let service_config = Sv2ClientServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            endpoint_host: None,
            endpoint_port: None,
            vendor: None,
            hardware_version: None,
            firmware: None,
            device_id: None,
            mining_config: None,
            job_declaration_config: None,
            template_distribution_config: Some(Sv2ClientServiceTemplateDistributionConfig {
                coinbase_output_constraints: (
                    config.coinbase_output_max_additional_size,
                    config.coinbase_output_max_additional_sigops,
                ),
                server_addr: config.server_addr,
                auth_pk: config.auth_pk,
            }),
            encrypted: true
        };

        // Create the handler instance
        let template_distribution_handler = MyTemplateDistributionHandler::default();

        // Initialize the service with config and handler
        let sv2_client_service =
            Sv2ClientService::new(service_config, template_distribution_handler)
                .map_err(|e| anyhow!("Failed to create Sv2ClientService: {:?}", e))?;

        Ok(Self {
            sv2_client_service,
            coinbase_output_max_additional_size: config.coinbase_output_max_additional_size,
            coinbase_output_max_additional_sigops: config.coinbase_output_max_additional_sigops,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.sv2_client_service
            .start()
            .await
            .map_err(|e| anyhow!("Failed to start Sv2ClientService: {:?}", e))?;

        self.sv2_client_service
            .ready()
            .await
            .map_err(|e| anyhow!("Service is not ready: {:?}", e))?;

        let set_coinbase_output_constraints_response = self
            .sv2_client_service
            .call(RequestToSv2Client::TemplateDistributionTrigger(
                RequestToSv2TemplateDistributionClientService::SetCoinbaseOutputConstraints(
                    self.coinbase_output_max_additional_size,
                    self.coinbase_output_max_additional_sigops,
                ),
            ))
            .await
            .map_err(|e| anyhow!("Failed to request coinbase output constraints: {:?}", e))?;

        match set_coinbase_output_constraints_response {
            ResponseFromSv2Client::ResponseToTemplateDistributionTrigger(
                ResponseToTemplateDistributionTrigger::SuccessfullySetCoinbaseOutputConstraints,
            ) => {
                info!("Coinbase output constraints set");
            }
            _ => {
                return Err(anyhow!("Failed to set coinbase output constraints"));
            }
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) {
        info!("Shutting down Template Distribution Client");
        self.sv2_client_service.shutdown().await;
    }
}
