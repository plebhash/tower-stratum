use crate::config::MyTemplateDistributionClientConfig;
use crate::handler::MyTemplateDistributionHandler;
use anyhow::{Result, anyhow};
use roles_logic_sv2::common_messages_sv2::Protocol;
use tower::{Service, ServiceExt};
use tower_stratum::client::service::Sv2ClientService;
use tower_stratum::client::service::config::Sv2ClientServiceConfig;
use tower_stratum::client::service::config::Sv2ClientServiceTemplateDistributionConfig;
use tower_stratum::client::service::request::RequestToSv2ClientService;
use tower_stratum::client::service::response::ResponseFromSv2ClientService;
use tower_stratum::client::service::subprotocols::template_distribution::request::RequestToSv2TemplateDistributionClientService;
use tower_stratum::client::service::subprotocols::template_distribution::response::ResponseToTemplateDistributionTrigger;
use tracing::{error, info};

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
                supported_flags: 0,
                coinbase_output_constraints: (
                    config.coinbase_output_max_additional_size,
                    config.coinbase_output_max_additional_sigops,
                ),
                server_addr: config.server_addr,
                auth_pk: config.auth_pk,
            }),
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
            .ready()
            .await
            .map_err(|e| anyhow!("Service is not ready: {:?}", e))?;

        let initiate_connection_response = self
            .sv2_client_service
            .call(RequestToSv2ClientService::SetupConnectionTrigger(
                Protocol::TemplateDistributionProtocol,
            ))
            .await
            .map_err(|e| panic!("Failed to initiate connection: {:?}", e))?;

        match initiate_connection_response {
            ResponseFromSv2ClientService::ConnectionEstablished => {
                info!("Connection established with Template Distribution Server");
            }
            _ => {
                return Err(anyhow!(
                    "Failed to initiate connection with Template Distribution Server"
                ));
            }
        }

        self.sv2_client_service
            .ready()
            .await
            .map_err(|e| anyhow!("Service is not ready: {:?}", e))?;

        let set_coinbase_output_constraints_response = self
            .sv2_client_service
            .call(RequestToSv2ClientService::TemplateDistributionTrigger(
                RequestToSv2TemplateDistributionClientService::SetCoinbaseOutputConstraints(
                    self.coinbase_output_max_additional_size,
                    self.coinbase_output_max_additional_sigops,
                ),
            ))
            .await
            .map_err(|e| anyhow!("Failed to request coinbase output constraints: {:?}", e))?;

        match set_coinbase_output_constraints_response {
            ResponseFromSv2ClientService::ResponseToTemplateDistributionTrigger(
                ResponseToTemplateDistributionTrigger::SuccessfullySetCoinbaseOutputConstraints,
            ) => {
                info!("Coinbase output constraints set");
            }
            _ => {
                return Err(anyhow!("Failed to set coinbase output constraints"));
            }
        }

        let mut service = self.sv2_client_service.clone();
        tokio::spawn(async move {
            if let Err(e) = service
                .listen_for_messages(Protocol::TemplateDistributionProtocol)
                .await
            {
                error!("Error listening for messages: {:?}", e);
            }
        });

        Ok(())
    }

    pub async fn shutdown(&mut self) {
        info!("Shutting down Template Distribution Client");
        self.sv2_client_service.shutdown().await;
    }
}
