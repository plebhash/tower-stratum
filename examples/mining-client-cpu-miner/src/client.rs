use crate::config::MyMiningClientConfig;
use crate::handler::MyMiningClientHandler;
use anyhow::{Result, anyhow};
use tower_stratum::client::service::Sv2ClientService;
use tower_stratum::client::service::config::Sv2ClientServiceConfig;
use tower_stratum::client::service::config::Sv2ClientServiceMiningConfig;
use tower_stratum::client::service::request::RequestToSv2Client;
use tower_stratum::client::service::response::ResponseFromSv2Client;
use tower_stratum::client::service::subprotocols::mining::request::RequestToSv2MiningClientService;
use tower_stratum::client::service::subprotocols::mining::response::ResponseToMiningTrigger;
use tower_stratum::client::service::subprotocols::template_distribution::handler::NullSv2TemplateDistributionClientHandler;
use tower_stratum::tower::{Service, ServiceExt};
use tracing::info;
pub struct MyMiningClient {
    sv2_client_service:
        Sv2ClientService<MyMiningClientHandler, NullSv2TemplateDistributionClientHandler>,
    user_identity: String,
    extranonce_rolling: bool,
}

impl MyMiningClient {
    pub async fn new(config: MyMiningClientConfig) -> Result<Self> {
        let service_config = Sv2ClientServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            endpoint_host: None,
            endpoint_port: None,
            vendor: None,
            hardware_version: None,
            firmware: None,
            device_id: None,
            mining_config: Some(Sv2ClientServiceMiningConfig {
                server_addr: config.server_addr,
                auth_pk: config.auth_pk,
                // REQUIRES_VERSION_ROLLING, !REQUIRES_WORK_SELECTION, REQUIRES_STANDARD_JOBS
                setup_connection_flags: 0b001_u32,
            }),
            job_declaration_config: None,
            template_distribution_config: None,
        };

        let template_distribution_handler = NullSv2TemplateDistributionClientHandler;

        let mining_handler = MyMiningClientHandler::default();

        let sv2_client_service = Sv2ClientService::new(
            service_config,
            mining_handler,
            template_distribution_handler,
        )
        .map_err(|e| anyhow!("Failed to create Sv2ClientService: {:?}", e))?;

        Ok(Self {
            sv2_client_service,
            user_identity: config.user_identity,
            extranonce_rolling: config.extranonce_rolling,
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

        // todo: try to open extended or standard channel with the server
        // depending on the extranonce_rolling config parameter
        match self.extranonce_rolling {
            false => {
                let open_standard_mining_channel_response = self
                    .sv2_client_service
                    .call(RequestToSv2Client::MiningTrigger(
                        RequestToSv2MiningClientService::OpenStandardMiningChannel(
                            0, // todo
                            self.user_identity.clone(),
                            10.0,                                  // todo
                            vec![0xFF_u8; 32].try_into().unwrap(), // todo
                        ),
                    ))
                    .await
                    .map_err(|e| anyhow!("Failed to open standard mining channel: {:?}", e))?;

                match open_standard_mining_channel_response {
                    ResponseFromSv2Client::ResponseToMiningTrigger(
                        ResponseToMiningTrigger::SuccessfullySentOpenStandardMiningChannelMessage,
                    ) => {
                        info!("Successfully sent open standard mining channel request");
                    }
                    _ => {
                        return Err(anyhow!("Failed to open standard mining channel"));
                    }
                }
            }
            true => {
                let open_extended_mining_channel_response = self
                    .sv2_client_service
                    .call(RequestToSv2Client::MiningTrigger(
                        RequestToSv2MiningClientService::OpenExtendedMiningChannel(
                            0, // todo
                            self.user_identity.clone(),
                            10.0,                                  // todo
                            vec![0xFF_u8; 32].try_into().unwrap(), // todo
                            1,
                        ),
                    ))
                    .await
                    .map_err(|e| anyhow!("Failed to open extended mining channel: {:?}", e))?;

                match open_extended_mining_channel_response {
                    ResponseFromSv2Client::ResponseToMiningTrigger(
                        ResponseToMiningTrigger::SuccessfullySentOpenExtendedMiningChannelMessage,
                    ) => {
                        info!("Successfully sent open extended mining channel request");
                    }
                    _ => {
                        return Err(anyhow!("Failed to open extended mining channel"));
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) {
        info!("Shutting down Mining Client");
        self.sv2_client_service.shutdown().await;
    }
}
