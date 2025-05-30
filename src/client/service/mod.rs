use crate::client::service::config::Sv2ClientServiceConfig;
use crate::client::service::error::Sv2ClientServiceError;
use crate::client::service::request::{RequestToSv2Client, RequestToSv2ClientError};
use crate::client::service::response::ResponseFromSv2Client;
use crate::client::service::sibling::Sv2SiblingServerServiceIo;
use crate::client::service::subprotocols::mining::handler::NullSv2MiningClientHandler;
use crate::client::service::subprotocols::mining::handler::Sv2MiningClientHandler;
use crate::client::service::subprotocols::mining::request::RequestToSv2MiningClientService;
use crate::client::service::subprotocols::template_distribution::handler::NullSv2TemplateDistributionClientHandler;
use crate::client::service::subprotocols::template_distribution::handler::Sv2TemplateDistributionClientHandler;
use crate::client::service::subprotocols::template_distribution::request::RequestToSv2TemplateDistributionClientService;
use crate::client::tcp::encrypted::Sv2EncryptedTcpClient;
use roles_logic_sv2::common_messages_sv2::MESSAGE_TYPE_SETUP_CONNECTION;
use roles_logic_sv2::common_messages_sv2::{Protocol, SetupConnection};
use roles_logic_sv2::mining_sv2::MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL;
use roles_logic_sv2::mining_sv2::MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL;
use roles_logic_sv2::mining_sv2::{OpenExtendedMiningChannel, OpenStandardMiningChannel};
use roles_logic_sv2::parsers::{AnyMessage, CommonMessages, Mining, TemplateDistribution};
use roles_logic_sv2::template_distribution_sv2::CoinbaseOutputConstraints;
use roles_logic_sv2::template_distribution_sv2::MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS;
use roles_logic_sv2::template_distribution_sv2::MESSAGE_TYPE_SUBMIT_SOLUTION;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{broadcast, RwLock};
use tower::{Service, ServiceExt};
use tracing::{debug, error};

pub mod config;
pub mod error;
pub mod request;
pub mod response;
pub mod sibling;
pub mod subprotocols;

/// A [`tower::Service`] implementer that provides:
/// - TCP connection to the server
/// - Connection management
/// - Optional handlers for the Mining, Job Declaration and Template Distribution protocols
/// - Ability to listen for messages from the server and trigger Service Requests
///
/// The `M` generic paramenter is the handler for the Mining protocol.
/// If the service does not support the Mining protocol, it should be set to `NullSv2MiningClientHandler`.
///
/// The `J` generic paramenter is the handler for the Job Declaration protocol.
/// If the service does not support the Job Declaration protocol, it should be set to `NullSv2JobDeclarationClientHandler`.
///
/// The `T` generic paramenter is the handler for the Template Distribution protocol.
/// If the service does not support the Template Distribution protocol, it should be set to `NullSv2TemplateDistributionClientHandler`.
#[derive(Debug, Clone)]
pub struct Sv2ClientService<M, T>
// todo: add J generic parameter
where
    M: Sv2MiningClientHandler + Clone + Send + Sync + 'static,
    T: Sv2TemplateDistributionClientHandler + Clone + Send + Sync + 'static,
{
    config: Sv2ClientServiceConfig,
    mining_tcp_client: Arc<RwLock<Option<Sv2EncryptedTcpClient>>>,
    job_declaration_tcp_client: Arc<RwLock<Option<Sv2EncryptedTcpClient>>>,
    template_distribution_tcp_client: Arc<RwLock<Option<Sv2EncryptedTcpClient>>>,
    mining_handler: M,
    // todo: add job_declaration_handler: J,
    template_distribution_handler: T,
    shutdown_tx: broadcast::Sender<()>,
    sibling_server_service_io: Option<Sv2SiblingServerServiceIo>,
}

impl<M, T> Sv2ClientService<M, T>
where
    M: Sv2MiningClientHandler + Clone + Send + Sync + 'static,
    T: Sv2TemplateDistributionClientHandler + Clone + Send + Sync + 'static,
{
    /// Creates a new [`Sv2ClientService`]
    ///
    /// No sibling server service is required.
    pub fn new(
        config: Sv2ClientServiceConfig,
        mining_handler: M,
        template_distribution_handler: T,
        // todo: add job_declaration_handler: J,
    ) -> Result<Self, Sv2ClientServiceError> {
        let sv2_client_service =
            Self::_new(config, mining_handler, template_distribution_handler, None)?;
        Ok(sv2_client_service)
    }

    /// Creates a new [`Sv2ClientService`] with a sibling server service.
    ///
    /// The sibling server service is used to send and receive requests to a sibling [`crate::server::service::Sv2ServerService`] that pairs with this client.    
    ///
    /// Before calling this, you need to create a [`Sv2SiblingServerServiceIo`] using [`crate::server::service::Sv2ServerService::new_with_sibling_io`].
    pub fn new_with_sibling_io(
        config: Sv2ClientServiceConfig,
        mining_handler: M,
        template_distribution_handler: T,
        sibling_server_service_io: Sv2SiblingServerServiceIo,
    ) -> Result<Self, Sv2ClientServiceError> {
        let sv2_client_service = Self::_new(
            config,
            mining_handler,
            template_distribution_handler,
            Some(sibling_server_service_io),
        )?;
        Ok(sv2_client_service)
    }

    // internal constructor
    fn _new(
        config: Sv2ClientServiceConfig,
        mining_handler: M,
        // todo: add job_declaration_handler: J,
        template_distribution_handler: T,
        sibling_server_service_io: Option<Sv2SiblingServerServiceIo>,
    ) -> Result<Self, Sv2ClientServiceError> {
        Self::validate_protocol_handlers(&config)?;

        let sv2_client_service = Sv2ClientService {
            config,
            mining_tcp_client: Arc::new(RwLock::new(None)),
            job_declaration_tcp_client: Arc::new(RwLock::new(None)),
            template_distribution_tcp_client: Arc::new(RwLock::new(None)),
            mining_handler,
            template_distribution_handler,
            shutdown_tx: broadcast::channel(1).0,
            sibling_server_service_io,
        };

        Ok(sv2_client_service)
    }

    // Validates that the protocol handlers are consistent with the supported protocols.
    // Returns an error if:
    // - A protocol is configured as supported but the corresponding handler is null.
    // - A protocol is not configured as supported but a non-null handler is provided.
    fn validate_protocol_handlers(
        config: &Sv2ClientServiceConfig,
    ) -> Result<(), Sv2ClientServiceError> {
        let supported_protocols: Vec<Protocol> = config
            .supported_protocols()
            .iter()
            .map(|(p, _)| *p)
            .collect();

        // Check if mining_handler is NullSv2MiningClientHandler
        let is_null_mining_handler =
            std::any::TypeId::of::<M>() == std::any::TypeId::of::<NullSv2MiningClientHandler>();

        if supported_protocols.contains(&Protocol::MiningProtocol) {
            if is_null_mining_handler {
                return Err(Sv2ClientServiceError::NullHandlerForSupportedProtocol {
                    protocol: Protocol::MiningProtocol,
                });
            }
        } else if !is_null_mining_handler {
            return Err(
                Sv2ClientServiceError::NonNullHandlerForUnsupportedProtocol {
                    protocol: Protocol::MiningProtocol,
                },
            );
        }

        // Check if template_distribution_handler is NullSv2TemplateDistributionClientHandler
        let is_null_template_distribution_handler = std::any::TypeId::of::<T>()
            == std::any::TypeId::of::<NullSv2TemplateDistributionClientHandler>();

        // Check if template_distribution_handler is compatible with the supported protocols
        if supported_protocols.contains(&Protocol::TemplateDistributionProtocol) {
            if is_null_template_distribution_handler {
                return Err(Sv2ClientServiceError::NullHandlerForSupportedProtocol {
                    protocol: Protocol::TemplateDistributionProtocol,
                });
            }
        } else if !is_null_template_distribution_handler {
            return Err(
                Sv2ClientServiceError::NonNullHandlerForUnsupportedProtocol {
                    protocol: Protocol::TemplateDistributionProtocol,
                },
            );
        }
        Ok(())
    }

    /// Checks if the client is connected to the server
    pub async fn is_connected(&self, protocol: Protocol) -> bool {
        match protocol {
            Protocol::MiningProtocol => {
                let guard = self.mining_tcp_client.read().await;
                guard.is_some()
            }
            Protocol::JobDeclarationProtocol => {
                let guard = self.job_declaration_tcp_client.read().await;
                guard.is_some()
            }
            Protocol::TemplateDistributionProtocol => {
                let guard = self.template_distribution_tcp_client.read().await;
                guard.is_some()
            }
        }
    }

    pub async fn start(&mut self) -> Result<(), Sv2ClientServiceError> {
        self.ready()
            .await
            .map_err(|_| Sv2ClientServiceError::ServiceNotReady)?;

        for (protocol, flags) in self.config.supported_protocols() {
            let initiate_connection_response = self
                .call(RequestToSv2Client::SetupConnectionTrigger(protocol, flags))
                .await
                .map_err(|_| Sv2ClientServiceError::FailedToInitiateConnection { protocol })?;
            match initiate_connection_response {
                ResponseFromSv2Client::Ok => {
                    debug!("Connection established with {:?}", protocol);
                }
                _ => {
                    return Err(Sv2ClientServiceError::FailedToInitiateConnection { protocol });
                }
            }

            let mut this = self.clone();
            tokio::spawn(async move {
                if let Err(e) = this.listen_for_messages_via_tcp(protocol).await {
                    error!("Error listening for messages: {:?}", e);
                }
            });

            let mut this = self.clone();
            if let Some(_sibling_server_service_io) = &mut this.sibling_server_service_io {
                tokio::spawn(async move {
                    if let Err(e) = this.listen_for_requests_via_sibling_server_service().await {
                        error!("Error listening for requests: {:?}", e);
                    }
                });
            }
        }

        self.ready()
            .await
            .map_err(|_| Sv2ClientServiceError::ServiceNotReady)?;

        Ok(())
    }

    /// Shuts down the client service
    pub async fn shutdown(&mut self) {
        debug!("Initiating shutdown of Sv2ClientService");

        // Send shutdown signal to all tasks
        if let Err(e) = self.shutdown_tx.send(()) {
            error!("Failed to send shutdown signal: {}", e);
        }

        {
            let mut mining_guard = self.mining_tcp_client.write().await;
            if let Some(client) = &*mining_guard {
                client.shutdown();
            }
            *mining_guard = None;
        }

        {
            let mut job_declaration_guard = self.job_declaration_tcp_client.write().await;
            if let Some(client) = &*job_declaration_guard {
                client.shutdown();
            }
            *job_declaration_guard = None;
        }

        {
            let mut template_distribution_guard =
                self.template_distribution_tcp_client.write().await;
            if let Some(client) = &*template_distribution_guard {
                client.shutdown();
            }
            *template_distribution_guard = None;
        }

        debug!("Sv2ClientService shutdown complete");
    }

    async fn initiate_connection(
        &mut self,
        protocol: Protocol,
        supported_flags: u32,
    ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
        // Establish TCP connection if not already connected
        let tcp_client = match protocol {
            Protocol::MiningProtocol => {
                if self.mining_tcp_client.read().await.is_none() {
                    let config = self.config.mining_config.as_ref().ok_or({
                        RequestToSv2ClientError::UnsupportedProtocol {
                            protocol: Protocol::MiningProtocol,
                        }
                    })?;
                    let tcp_client = Sv2EncryptedTcpClient::new(config.server_addr, config.auth_pk)
                        .await
                        .ok_or_else(|| {
                            RequestToSv2ClientError::ConnectionError(
                                "Failed to create TCP client".to_string(),
                            )
                        })?;
                    self.mining_tcp_client
                        .write()
                        .await
                        .replace(tcp_client.clone());
                    tcp_client
                } else {
                    self.mining_tcp_client
                        .read()
                        .await
                        .as_ref()
                        .expect("mining_tcp_client should be Some")
                        .clone()
                }
            }
            Protocol::JobDeclarationProtocol => {
                if self.job_declaration_tcp_client.read().await.is_none() {
                    let config = self.config.job_declaration_config.as_ref().ok_or({
                        RequestToSv2ClientError::UnsupportedProtocol {
                            protocol: Protocol::JobDeclarationProtocol,
                        }
                    })?;
                    let tcp_client = Sv2EncryptedTcpClient::new(config.server_addr, config.auth_pk)
                        .await
                        .ok_or_else(|| {
                            RequestToSv2ClientError::ConnectionError(
                                "Failed to create TCP client".to_string(),
                            )
                        })?;
                    self.job_declaration_tcp_client
                        .write()
                        .await
                        .replace(tcp_client.clone());
                    tcp_client
                } else {
                    self.job_declaration_tcp_client
                        .read()
                        .await
                        .as_ref()
                        .expect("job_declaration_tcp_client should be Some")
                        .clone()
                }
            }
            Protocol::TemplateDistributionProtocol => {
                if self.template_distribution_tcp_client.read().await.is_none() {
                    let config = self.config.template_distribution_config.as_ref().ok_or(
                        RequestToSv2ClientError::UnsupportedProtocol {
                            protocol: Protocol::TemplateDistributionProtocol,
                        },
                    )?;
                    let tcp_client = Sv2EncryptedTcpClient::new(config.server_addr, config.auth_pk)
                        .await
                        .ok_or_else(|| {
                            RequestToSv2ClientError::ConnectionError(
                                "Failed to create TCP client".to_string(),
                            )
                        })?;
                    self.template_distribution_tcp_client
                        .write()
                        .await
                        .replace(tcp_client.clone());
                    tcp_client
                } else {
                    self.template_distribution_tcp_client
                        .read()
                        .await
                        .as_ref()
                        .expect("template_distribution_tcp_client should be Some")
                        .clone()
                }
            }
        };

        let endpoint_host = self.config.endpoint_host.clone().unwrap_or_default();
        let endpoint_port = self.config.endpoint_port.unwrap_or_default();
        let vendor = self.config.vendor.clone().unwrap_or_default();
        let hardware_version = self.config.hardware_version.clone().unwrap_or_default();
        let firmware = self.config.firmware.clone().unwrap_or_default();
        let device_id = self.config.device_id.clone().unwrap_or_default();

        // Helper inline function to convert string to STR0_255
        let to_str0_255 = |s: String, field: &str| {
            s.clone().into_bytes().try_into().map_err(|_| {
                RequestToSv2ClientError::StringConversionError(format!(
                    "Failed to convert {} '{}' to fixed-size array",
                    field, s
                ))
            })
        };

        let setup_connection = SetupConnection {
            protocol,
            min_version: self.config.min_supported_version,
            max_version: self.config.max_supported_version,
            flags: supported_flags,
            endpoint_host: to_str0_255(endpoint_host, "endpoint_host")?,
            endpoint_port,
            vendor: to_str0_255(vendor, "vendor")?,
            hardware_version: to_str0_255(hardware_version, "hardware_version")?,
            firmware: to_str0_255(firmware, "firmware")?,
            device_id: to_str0_255(device_id, "device_id")?,
        };

        // Send the setup connection message using the io field
        tcp_client
            .io
            .send_message(setup_connection.into(), MESSAGE_TYPE_SETUP_CONNECTION)
            .await?;

        // wait for the server to respond with a SetupConnectionSuccess or SetupConnectionError
        // and return the appropriate response
        let (message, _) = tcp_client.io.recv_message().await?;
        match message {
            AnyMessage::Common(CommonMessages::SetupConnectionSuccess(
                setup_connection_success,
            )) => {
                let server_used_version = setup_connection_success.used_version;
                let server_used_flags = setup_connection_success.flags;

                debug!(
                    "SetupConnectionSuccess received: version: {}, flags: {}",
                    server_used_version, server_used_flags
                );
                Ok(ResponseFromSv2Client::Ok)
            }
            AnyMessage::Common(CommonMessages::SetupConnectionError(setup_connection_error)) => {
                let error_code =
                    String::from_utf8_lossy(setup_connection_error.error_code.inner_as_ref())
                        .to_string();
                Err(RequestToSv2ClientError::SetupConnectionError(error_code))
            }
            _ => Err(RequestToSv2ClientError::ConnectionError(
                "Unexpected message type in response to SetupConnection".to_string(),
            )),
        }
    }

    // Listens for messages from the server and triggers Service Requests
    async fn listen_for_messages_via_tcp(
        &mut self,
        protocol: Protocol,
    ) -> Result<(), RequestToSv2ClientError> {
        if !self.is_connected(protocol).await {
            return Err(RequestToSv2ClientError::IsNotConnected);
        }

        let tcp_client: Sv2EncryptedTcpClient = match protocol {
            Protocol::MiningProtocol => match self.mining_tcp_client.read().await.as_ref() {
                Some(client) => client.clone(),
                None => return Err(RequestToSv2ClientError::IsNotConnected),
            },
            Protocol::JobDeclarationProtocol => {
                match self.job_declaration_tcp_client.read().await.as_ref() {
                    Some(client) => client.clone(),
                    None => return Err(RequestToSv2ClientError::IsNotConnected),
                }
            }
            Protocol::TemplateDistributionProtocol => {
                match self.template_distribution_tcp_client.read().await.as_ref() {
                    Some(client) => client.clone(),
                    None => return Err(RequestToSv2ClientError::IsNotConnected),
                }
            }
        };

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    debug!("Message listener received shutdown signal");
                    break;
                }
                message_result = tcp_client.io.recv_message() => {
                    match message_result {
                        Ok((message, _)) => {
                            if let Err(e) = self.call(RequestToSv2Client::Message(message)).await {
                                // this is a protection from attacks where a server sends a message that it knows the client cannot handle
                                // we simply log the error and ignore the message, without shutting down the client
                                error!("Error handling message: {:?}, message will be ignored", e);
                            }
                        }
                        Err(_) => {
                            debug!("Message listener channel closed");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // Listens for requests from the sibling server service and triggers Service Requests
    async fn listen_for_requests_via_sibling_server_service(
        &mut self,
    ) -> Result<(), RequestToSv2ClientError> {
        let sibling_server_service_io = self
            .sibling_server_service_io
            .as_ref()
            .ok_or(RequestToSv2ClientError::NoSiblingServerServiceIo)?;

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    debug!("Sibling server service request listener received shutdown signal");
                    break;
                }
                result = sibling_server_service_io.recv() => {
                    match result {
                        Ok(req) => {
                            debug!("Received request from sibling server service");

                            let mut service = self.clone();
                            if let Err(e) = service.call(*req).await {
                                error!("Error handling request from sibling server service: {:?}", e);
                            }
                        }
                        Err(e) => {
                            error!("Failed to receive request from sibling server service: {:?}", e);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Checks if the handler for the given protocol is a null handler
    fn has_null_handler(protocol: Protocol) -> bool {
        match protocol {
            Protocol::MiningProtocol => true, // Currently always true since mining handler is not implemented yet
            Protocol::JobDeclarationProtocol => true, // Currently always true since job declaration handler is not implemented yet
            Protocol::TemplateDistributionProtocol => {
                std::any::TypeId::of::<T>()
                    == std::any::TypeId::of::<NullSv2TemplateDistributionClientHandler>()
            }
        }
    }
}

impl<M, T> Service<RequestToSv2Client<'static>> for Sv2ClientService<M, T>
where
    M: Sv2MiningClientHandler + Clone + Send + Sync + 'static,
    T: Sv2TemplateDistributionClientHandler + Clone + Send + Sync + 'static,
{
    type Response = ResponseFromSv2Client<'static>;
    type Error = RequestToSv2ClientError;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    /// Polls readiness of each subprotocol handler.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mining_poll_ready = match Self::has_null_handler(Protocol::MiningProtocol) {
            true => Poll::Ready(Ok(())),
            false => self.mining_handler.poll_ready(cx),
        };

        // let job_declaration_poll_ready = match Self::has_null_handler(Protocol::JobDeclarationProtocol) {
        //     true => Poll::Ready(Ok(())),
        //     false => self.job_declaration_handler.poll_ready(cx),
        // };

        let job_declaration_poll_ready = Poll::Ready(Ok(()));

        let template_distribution_poll_ready =
            match Self::has_null_handler(Protocol::TemplateDistributionProtocol) {
                true => Poll::Ready(Ok(())),
                false => self.template_distribution_handler.poll_ready(cx),
            };

        // let template_distribution_poll_ready = Poll::Ready(Ok(()));

        // Combine the poll results - if any handler is not ready, return NotReady
        match (
            mining_poll_ready,
            job_declaration_poll_ready,
            template_distribution_poll_ready,
        ) {
            (Poll::Ready(Ok(())), Poll::Ready(Ok(())), Poll::Ready(Ok(()))) => Poll::Ready(Ok(())),
            (Poll::Ready(Err(e)), _, _) => Poll::Ready(Err(e)),
            (_, Poll::Ready(Err(e)), _) => Poll::Ready(Err(e)),
            (_, _, Poll::Ready(Err(e))) => Poll::Ready(Err(e)),
            _ => Poll::Pending,
        }
    }

    fn call(&mut self, request: RequestToSv2Client<'static>) -> Self::Future {
        // https://docs.rs/tower/latest/tower/trait.Service.html#be-careful-when-cloning-inner-services
        let clone = self.clone();
        let mut this = std::mem::replace(self, clone);

        Box::pin(async move {
            let response = match request {
                RequestToSv2Client::SetupConnectionTrigger(protocol, flags) => match protocol {
                    Protocol::MiningProtocol => {
                        debug!(
                                "Sv2ClientService received a trigger request for initiating a connection under the Mining protocol"
                            );
                        if this.config.mining_config.is_none() {
                            return Err(RequestToSv2ClientError::UnsupportedProtocol {
                                protocol: Protocol::MiningProtocol,
                            });
                        }
                        this.initiate_connection(protocol, flags).await
                    }
                    Protocol::JobDeclarationProtocol => {
                        debug!(
                                "Sv2ClientService received a trigger request for initiating a connection under the Job Declaration protocol"
                            );
                        if this.config.job_declaration_config.is_none() {
                            return Err(RequestToSv2ClientError::UnsupportedProtocol {
                                protocol: Protocol::JobDeclarationProtocol,
                            });
                        }
                        this.initiate_connection(protocol, 0).await
                    }
                    Protocol::TemplateDistributionProtocol => {
                        debug!(
                                "Sv2ClientService received a trigger request for initiating a connection under the Template Distribution protocol"
                            );
                        if this.config.template_distribution_config.is_none() {
                            return Err(RequestToSv2ClientError::UnsupportedProtocol {
                                protocol: Protocol::TemplateDistributionProtocol,
                            });
                        }
                        this.initiate_connection(protocol, 0).await
                    }
                },
                RequestToSv2Client::Message(sv2_message) => {
                    match sv2_message {
                        AnyMessage::Common(common) => match common {
                            CommonMessages::SetupConnection(_) => {
                                // a client should never receive a SetupConnection request
                                error!("Sv2ClientService received a SetupConnection request");
                                Err(RequestToSv2ClientError::UnsupportedMessage)
                            }
                            CommonMessages::SetupConnectionSuccess(_) => {
                                // the only situation where a client should receive a SetupConnectionSuccess is inside initiate_connection
                                error!("Sv2ClientService received a SetupConnectionSuccess request outside of initiate_connection");
                                Err(RequestToSv2ClientError::BadRouting)
                            }
                            CommonMessages::SetupConnectionError(_) => {
                                // the only situation where a client should receive a SetupConnectionError is inside initiate_connection
                                error!("Sv2ClientService received a SetupConnectionError request outside of initiate_connection");
                                Err(RequestToSv2ClientError::BadRouting)
                            }
                            CommonMessages::ChannelEndpointChanged(_channel_endpoint_changed) => {
                                todo!()
                            }
                            CommonMessages::Reconnect(_) => {
                                todo!()
                            }
                        },
                        AnyMessage::TemplateDistribution(template_distribution) => {
                            // check if the template_distribution_handler is supported
                            if std::any::TypeId::of::<T>()
                                == std::any::TypeId::of::<NullSv2TemplateDistributionClientHandler>(
                                )
                            {
                                error!("Sv2ClientService received a TemplateDistribution message, but no template distribution handler is configured");
                                return Err(RequestToSv2ClientError::UnsupportedProtocol {
                                    protocol: Protocol::TemplateDistributionProtocol,
                                });
                            }

                            match template_distribution {
                                TemplateDistribution::CoinbaseOutputConstraints(_) => {
                                    // a client should never receive a CoinbaseOutputConstraints message
                                    error!("Sv2ClientService received a CoinbaseOutputConstraints message");
                                    Err(RequestToSv2ClientError::UnsupportedMessage)
                                }
                                TemplateDistribution::SubmitSolution(_) => {
                                    // a client should never receive a SubmitSolution request
                                    error!("Sv2ClientService received a SubmitSolution message");
                                    Err(RequestToSv2ClientError::UnsupportedMessage)
                                }
                                TemplateDistribution::RequestTransactionData(_) => {
                                    // a client should never receive a RequestTransactionData request
                                    error!("Sv2ClientService received a RequestTransactionData message");
                                    Err(RequestToSv2ClientError::UnsupportedMessage)
                                }
                                TemplateDistribution::NewTemplate(message) => {
                                    debug!("Sv2ClientService received a NewTemplate message");
                                    this.template_distribution_handler
                                        .handle_new_template(message)
                                        .await
                                }
                                TemplateDistribution::RequestTransactionDataError(message) => {
                                    debug!("Sv2ClientService received a RequestTransactionDataError message");
                                    this.template_distribution_handler
                                        .handle_request_transaction_data_error(message)
                                        .await
                                }
                                TemplateDistribution::RequestTransactionDataSuccess(message) => {
                                    debug!("Sv2ClientService received a RequestTransactionDataSuccess message");
                                    this.template_distribution_handler
                                        .handle_request_transaction_data_success(message)
                                        .await
                                }
                                TemplateDistribution::SetNewPrevHash(message) => {
                                    debug!("Sv2ClientService received a SetNewPrevHash message");
                                    this.template_distribution_handler
                                        .handle_set_new_prev_hash(message)
                                        .await
                                }
                            }
                        }
                        AnyMessage::Mining(mining) => match mining {
                            Mining::OpenStandardMiningChannel(_) => {
                                // a client should never receive a OpenStandardMiningChannel message
                                error!(
                                    "Sv2ClientService received a OpenStandardMiningChannel message"
                                );
                                Err(RequestToSv2ClientError::UnsupportedMessage)
                            }
                            Mining::OpenExtendedMiningChannel(_) => {
                                // a client should never receive a OpenExtendedMiningChannel message
                                error!(
                                    "Sv2ClientService received a OpenExtendedMiningChannel message"
                                );
                                Err(RequestToSv2ClientError::UnsupportedMessage)
                            }
                            Mining::UpdateChannel(_) => {
                                // a client should never receive a UpdateChannel message
                                error!("Sv2ClientService received a UpdateChannel message");
                                Err(RequestToSv2ClientError::UnsupportedMessage)
                            }
                            Mining::SubmitSharesStandard(_) => {
                                // a client should never receive a SubmitSharesStandard message
                                error!("Sv2ClientService received a SubmitSharesStandard message");
                                Err(RequestToSv2ClientError::UnsupportedMessage)
                            }
                            Mining::SubmitSharesExtended(_) => {
                                // a client should never receive a SubmitSharesExtended message
                                error!("Sv2ClientService received a SubmitSharesExtended message");
                                Err(RequestToSv2ClientError::UnsupportedMessage)
                            }
                            Mining::SetCustomMiningJob(_) => {
                                // a client should never receive a SetCustomMiningJob message
                                error!("Sv2ClientService received a SetCustomMiningJob message");
                                Err(RequestToSv2ClientError::UnsupportedMessage)
                            }
                            Mining::OpenStandardMiningChannelSuccess(success) => {
                                debug!("Sv2ClientService received a OpenStandardMiningChannelSuccess message");
                                this.mining_handler
                                    .handle_open_standard_mining_channel_success(success)
                                    .await
                            }
                            Mining::OpenExtendedMiningChannelSuccess(success) => {
                                debug!("Sv2ClientService received a OpenExtendedMiningChannelSuccess message");
                                this.mining_handler
                                    .handle_open_extended_mining_channel_success(success)
                                    .await
                            }
                            Mining::OpenMiningChannelError(error) => {
                                debug!(
                                    "Sv2ClientService received a OpenMiningChannelError message"
                                );
                                this.mining_handler
                                    .handle_open_mining_channel_error(error)
                                    .await
                            }
                            Mining::UpdateChannelError(error) => {
                                debug!("Sv2ClientService received a UpdateChannelError message");
                                this.mining_handler.handle_update_channel_error(error).await
                            }
                            Mining::CloseChannel(close_channel) => {
                                debug!("Sv2ClientService received a CloseChannel message");
                                this.mining_handler
                                    .handle_close_channel(close_channel)
                                    .await
                            }
                            Mining::SetExtranoncePrefix(set_extranonce_prefix) => {
                                debug!("Sv2ClientService received a SetExtranoncePrefix message");
                                this.mining_handler
                                    .handle_set_extranonce_prefix(set_extranonce_prefix)
                                    .await
                            }
                            Mining::SubmitSharesSuccess(success) => {
                                debug!("Sv2ClientService received a SubmitSharesSuccess message");
                                this.mining_handler
                                    .handle_submit_shares_success(success)
                                    .await
                            }
                            Mining::SubmitSharesError(error) => {
                                debug!("Sv2ClientService received a SubmitSharesError message");
                                this.mining_handler.handle_submit_shares_error(error).await
                            }
                            Mining::NewMiningJob(new_mining_job) => {
                                debug!("Sv2ClientService received a NewMiningJob message");
                                this.mining_handler
                                    .handle_new_mining_job(new_mining_job)
                                    .await
                            }
                            Mining::NewExtendedMiningJob(new_extended_mining_job) => {
                                debug!("Sv2ClientService received a NewExtendedMiningJob message");
                                this.mining_handler
                                    .handle_new_extended_mining_job(new_extended_mining_job)
                                    .await
                            }
                            Mining::SetNewPrevHash(set_new_prev_hash) => {
                                debug!("Sv2ClientService received a SetNewPrevHash message");
                                this.mining_handler
                                    .handle_set_new_prev_hash(set_new_prev_hash)
                                    .await
                            }
                            Mining::SetCustomMiningJobSuccess(success) => {
                                debug!(
                                    "Sv2ClientService received a SetCustomMiningJobSuccess message"
                                );
                                this.mining_handler
                                    .handle_set_custom_mining_job_success(success)
                                    .await
                            }
                            Mining::SetCustomMiningJobError(error) => {
                                debug!(
                                    "Sv2ClientService received a SetCustomMiningJobError message"
                                );
                                this.mining_handler
                                    .handle_set_custom_mining_job_error(error)
                                    .await
                            }
                            Mining::SetTarget(set_target) => {
                                debug!("Sv2ClientService received a SetTarget message");
                                this.mining_handler.handle_set_target(set_target).await
                            }
                            Mining::SetGroupChannel(set_group_channel) => {
                                debug!("Sv2ClientService received a SetGroupChannel message");
                                this.mining_handler
                                    .handle_set_group_channel(set_group_channel)
                                    .await
                            }
                        },
                        _ => todo!(),
                    }
                }
                RequestToSv2Client::MiningTrigger(request) => {
                    if this.config.mining_config.is_none()
                        || std::any::TypeId::of::<M>()
                            == std::any::TypeId::of::<NullSv2MiningClientHandler>()
                    {
                        return Err(RequestToSv2ClientError::UnsupportedProtocol {
                            protocol: Protocol::MiningProtocol,
                        });
                    }
                    if !this.is_connected(Protocol::MiningProtocol).await {
                        return Err(RequestToSv2ClientError::IsNotConnected);
                    }
                    match request {
                        RequestToSv2MiningClientService::OpenStandardMiningChannel(
                            request_id,
                            user_identity,
                            nominal_hash_rate,
                            max_target,
                        ) => {
                            debug!("Sv2ClientService received a trigger request for sending OpenStandardMiningChannel");
                            let tcp_client = this
                                .mining_tcp_client
                                .read()
                                .await
                                .as_ref()
                                .expect("mining_tcp_client should be Some")
                                .clone();
                            let open_standard_mining_channel = AnyMessage::Mining(
                                Mining::OpenStandardMiningChannel(OpenStandardMiningChannel {
                                    request_id: request_id.into(),
                                    user_identity: user_identity
                                        .clone()
                                        .into_bytes()
                                        .try_into()
                                        .map_err(|_| {
                                            RequestToSv2ClientError::StringConversionError(format!(
                                            "Failed to convert user_identity '{}' to fixed-size array",
                                            user_identity
                                        ))
                                        })?,
                                    nominal_hash_rate,
                                    max_target: max_target.try_into().map_err(|_| {
                                        RequestToSv2ClientError::U256ConversionError(
                                            "Failed to convert max_target to fixed-size array".to_string(),
                                        )
                                    })?,
                                }),
                            );

                            let result = tcp_client
                                .io
                                .send_message(
                                    open_standard_mining_channel,
                                    MESSAGE_TYPE_OPEN_STANDARD_MINING_CHANNEL,
                                )
                                .await;
                            match result {
                                Ok(_) => {
                                    debug!("Successfully sent OpenStandardMiningChannel");
                                    Ok(ResponseFromSv2Client::Ok)
                                }
                                Err(e) => Err(e.into()),
                            }
                        }
                        RequestToSv2MiningClientService::OpenExtendedMiningChannel(
                            request_id,
                            user_identity,
                            nominal_hash_rate,
                            max_target,
                            min_rollable_extranonce_size,
                        ) => {
                            debug!("Sv2ClientService received a trigger request for sending OpenExtendedMiningChannel");
                            let tcp_client = this
                                .mining_tcp_client
                                .read()
                                .await
                                .as_ref()
                                .expect("mining_tcp_client should be Some")
                                .clone();

                            let open_extended_mining_channel = AnyMessage::Mining(
                                Mining::OpenExtendedMiningChannel(OpenExtendedMiningChannel {
                                    request_id,
                                    user_identity: user_identity
                                        .clone()
                                        .into_bytes()
                                        .try_into()
                                        .map_err(|_| {
                                            RequestToSv2ClientError::StringConversionError(format!(
                                            "Failed to convert user_identity '{}' to fixed-size array",
                                            user_identity,
                                        ))
                                        })?,
                                    nominal_hash_rate,
                                    max_target: max_target.try_into().map_err(|_| {
                                        RequestToSv2ClientError::U256ConversionError(
                                            "Failed to convert max_target to fixed-size array".to_string(),
                                        )
                                    })?,
                                    min_extranonce_size: min_rollable_extranonce_size,
                                }),
                            );

                            let result = tcp_client
                                .io
                                .send_message(
                                    open_extended_mining_channel,
                                    MESSAGE_TYPE_OPEN_EXTENDED_MINING_CHANNEL,
                                )
                                .await;
                            match result {
                                Ok(_) => {
                                    debug!("Successfully sent OpenExtendedMiningChannel");
                                    Ok(ResponseFromSv2Client::Ok)
                                }
                                Err(e) => Err(e.into()),
                            }
                        }
                    }
                }
                RequestToSv2Client::TemplateDistributionTrigger(request) => {
                    // check if this service is configured for template distribution
                    if this.config.template_distribution_config.is_none()
                        || std::any::TypeId::of::<T>()
                            == std::any::TypeId::of::<NullSv2TemplateDistributionClientHandler>()
                    {
                        return Err(RequestToSv2ClientError::UnsupportedProtocol {
                            protocol: Protocol::TemplateDistributionProtocol,
                        });
                    }
                    if !this
                        .is_connected(Protocol::TemplateDistributionProtocol)
                        .await
                    {
                        return Err(RequestToSv2ClientError::IsNotConnected);
                    }
                    match request {
                        RequestToSv2TemplateDistributionClientService::SetCoinbaseOutputConstraints(
                            max_additional_size,
                            max_additional_sigops,
                        ) => {
                            debug!("Sv2ClientService received a trigger request for sending CoinbaseOutputConstraints");
                            let tcp_client = this
                                .template_distribution_tcp_client
                                .read()
                                .await
                                .as_ref()
                                .expect("template_distribution_tcp_client should be Some")
                                .clone();
                            let coinbase_output_constraints = AnyMessage::TemplateDistribution(
                                TemplateDistribution::CoinbaseOutputConstraints(
                                    CoinbaseOutputConstraints {
                                        coinbase_output_max_additional_size: max_additional_size,
                                        coinbase_output_max_additional_sigops: max_additional_sigops,
                                    },
                                ),
                            );
                            let result = tcp_client
                                .io
                                .send_message(
                                    coinbase_output_constraints,
                                    MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS,
                                )
                                .await;
                            match result {
                                Ok(_) => {
                                    debug!("Successfully set CoinbaseOutputConstraints");
                                    this.config
                                        .template_distribution_config
                                        .as_mut()
                                        .expect("template_distribution_config should be Some")
                                        .coinbase_output_constraints =
                                        (max_additional_size, max_additional_sigops);
                                    Ok(ResponseFromSv2Client::Ok)
                                }
                                Err(e) => Err(e.into()),
                            }
                        }
                        RequestToSv2TemplateDistributionClientService::TransactionDataNeeded(
                            _template_id,
                        ) => {
                            debug!("Sv2ClientService received a trigger request for sending RequestTransactionData");
                            todo!()
                        }
                        RequestToSv2TemplateDistributionClientService::SubmitSolution(
                            submit_solution,
                        ) => {
                            debug!("Sv2ClientService received a trigger request for sending SubmitSolution");
                            if !this
                                .is_connected(Protocol::TemplateDistributionProtocol)
                                .await
                            {
                                return Err(RequestToSv2ClientError::IsNotConnected);
                            }

                            let tcp_client = this
                                .template_distribution_tcp_client
                                .read()
                                .await
                                .as_ref()
                                .expect("template_distribution_tcp_client should be Some")
                                .clone();
                            let submit_solution = AnyMessage::TemplateDistribution(
                                TemplateDistribution::SubmitSolution(submit_solution),
                            );
                            let result = tcp_client
                                .io
                                .send_message(submit_solution, MESSAGE_TYPE_SUBMIT_SOLUTION)
                                .await;
                            match result {
                                Ok(_) => {
                                    debug!("Successfully sent SubmitSolution");
                                    Ok(ResponseFromSv2Client::Ok)
                                }
                                Err(e) => Err(e.into()),
                            }
                        }
                    }
                }
                RequestToSv2Client::SendRequestToSiblingServerService(req) => {
                    debug!("Sv2ClientService received a SendRequestToSiblingServerService request");
                    match this.sibling_server_service_io {
                        Some(ref io) => {
                            io.send(*req.clone()).map_err(|_| {
                                RequestToSv2ClientError::FailedToSendRequestToSiblingServerService
                            })?;
                            Ok(ResponseFromSv2Client::Ok)
                        }
                        None => {
                            error!("No sibling server service on Sv2ClientService");
                            Err(RequestToSv2ClientError::NoSiblingServerServiceIo)
                        }
                    }
                }
            };

            // allows for recursive chaining of requests
            if let Ok(ResponseFromSv2Client::TriggerNewRequest(request)) = response {
                this.call(*request).await
            } else {
                response
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::client::service::config::Sv2ClientServiceConfig;
    use crate::client::service::config::Sv2ClientServiceMiningConfig;
    use crate::client::service::config::Sv2ClientServiceTemplateDistributionConfig;
    use crate::client::service::request::RequestToSv2Client;
    use crate::client::service::response::ResponseFromSv2Client;
    use crate::client::service::subprotocols::mining::handler::NullSv2MiningClientHandler;
    use crate::client::service::subprotocols::mining::handler::Sv2MiningClientHandler;
    use crate::client::service::subprotocols::template_distribution::handler::NullSv2TemplateDistributionClientHandler;
    use crate::client::service::subprotocols::template_distribution::handler::Sv2TemplateDistributionClientHandler;
    use crate::client::service::subprotocols::template_distribution::request::RequestToSv2TemplateDistributionClientService;
    use crate::client::service::RequestToSv2ClientError;
    use crate::client::service::Sv2ClientService;
    use crate::server::service::config::Sv2ServerServiceConfig;
    use crate::server::service::config::Sv2ServerServiceMiningConfig;
    use crate::server::service::config::Sv2ServerTcpConfig;
    use crate::server::service::request::RequestToSv2Server;
    use crate::server::service::request::RequestToSv2ServerError;
    use crate::server::service::response::ResponseFromSv2Server;
    use crate::server::service::subprotocols::mining::handler::Sv2MiningServerHandler;
    use crate::server::service::subprotocols::mining::request::RequestToSv2MiningServer;
    use crate::server::service::Sv2ServerService;
    use integration_tests_sv2::interceptor::MessageDirection;
    use integration_tests_sv2::start_sniffer;
    use integration_tests_sv2::start_template_provider;
    use key_utils::Secp256k1PublicKey;
    use key_utils::Secp256k1SecretKey;
    use roles_logic_sv2::common_messages_sv2::Protocol;
    use roles_logic_sv2::mining_sv2::OpenExtendedMiningChannel;
    use roles_logic_sv2::mining_sv2::OpenStandardMiningChannel;
    use roles_logic_sv2::mining_sv2::SetCustomMiningJob;
    use roles_logic_sv2::mining_sv2::SubmitSharesExtended;
    use roles_logic_sv2::mining_sv2::SubmitSharesStandard;
    use roles_logic_sv2::mining_sv2::UpdateChannel;
    use roles_logic_sv2::mining_sv2::{
        CloseChannel, NewExtendedMiningJob, NewMiningJob, OpenExtendedMiningChannelSuccess,
        OpenMiningChannelError, OpenStandardMiningChannelSuccess, SetCustomMiningJobError,
        SetCustomMiningJobSuccess, SetExtranoncePrefix, SetGroupChannel,
        SetNewPrevHash as SetNewPrevHashMining, SetTarget, SubmitSharesError, SubmitSharesSuccess,
        UpdateChannelError,
    };
    use roles_logic_sv2::template_distribution_sv2::MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS;
    use roles_logic_sv2::template_distribution_sv2::{
        NewTemplate, RequestTransactionDataError, RequestTransactionDataSuccess, SetNewPrevHash,
    };
    use std::net::IpAddr;
    use std::net::Ipv4Addr;
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::task::{Context, Poll};
    use tokio::sync::mpsc;
    use tower::{Service, ServiceExt};

    // A dummy mining handler that is not null, but not actually handling anything
    #[derive(Debug, Clone, Default)]
    struct DummyMiningClientHandler;

    impl Sv2MiningClientHandler for DummyMiningClientHandler {
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), RequestToSv2ClientError>> {
            Poll::Ready(Ok(()))
        }

        async fn handle_open_standard_mining_channel_success(
            &self,
            _open_standard_mining_channel_success: OpenStandardMiningChannelSuccess<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_open_extended_mining_channel_success(
            &self,
            _open_extended_mining_channel_success: OpenExtendedMiningChannelSuccess<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_open_mining_channel_error(
            &self,
            _open_mining_channel_error: OpenMiningChannelError<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_update_channel_error(
            &self,
            _update_channel_error: UpdateChannelError<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_close_channel(
            &self,
            _close_channel: CloseChannel<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_set_extranonce_prefix(
            &self,
            _set_extranonce_prefix: SetExtranoncePrefix<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_submit_shares_success(
            &self,
            _submit_shares_success: SubmitSharesSuccess,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_submit_shares_error(
            &self,
            _submit_shares_error: SubmitSharesError<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_new_mining_job(
            &self,
            _new_mining_job: NewMiningJob<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_new_extended_mining_job(
            &self,
            _new_extended_mining_job: NewExtendedMiningJob<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_set_new_prev_hash(
            &self,
            _set_new_prev_hash: SetNewPrevHashMining<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_set_custom_mining_job_success(
            &self,
            _set_custom_mining_job_success: SetCustomMiningJobSuccess,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_set_custom_mining_job_error(
            &self,
            _set_custom_mining_job_error: SetCustomMiningJobError<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_set_target(
            &self,
            _set_target: SetTarget<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_set_group_channel(
            &self,
            _set_group_channel: SetGroupChannel<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }
    }

    // A dummy template distribution handler that is not null, but not actually handling anything
    #[derive(Debug, Clone, Default)]
    struct DummyTemplateDistributionClientHandler;

    impl Sv2TemplateDistributionClientHandler for DummyTemplateDistributionClientHandler {
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), RequestToSv2ClientError>> {
            Poll::Ready(Ok(()))
        }

        async fn handle_new_template(
            &self,
            _template: NewTemplate<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_set_new_prev_hash(
            &self,
            _prev_hash: SetNewPrevHash<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_request_transaction_data_success(
            &self,
            _transaction_data: RequestTransactionDataSuccess<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_request_transaction_data_error(
            &self,
            _error: RequestTransactionDataError<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }
    }

    // A template distribution handler that is sending requests using the Sibling IO feature
    #[derive(Debug, Clone, Default)]
    struct SiblingIoTemplateDistributionClientHandler;

    impl Sv2TemplateDistributionClientHandler for SiblingIoTemplateDistributionClientHandler {
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), RequestToSv2ClientError>> {
            Poll::Ready(Ok(()))
        }

        async fn handle_new_template(
            &self,
            template: NewTemplate<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            let response = ResponseFromSv2Client::TriggerNewRequest(Box::new(
                RequestToSv2Client::SendRequestToSiblingServerService(Box::new(
                    RequestToSv2Server::MiningTrigger(RequestToSv2MiningServer::NewTemplate(
                        template.into_static(),
                    )),
                )),
            ));
            Ok(response)
        }

        async fn handle_set_new_prev_hash(
            &self,
            prev_hash: SetNewPrevHash<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            let response = ResponseFromSv2Client::TriggerNewRequest(Box::new(
                RequestToSv2Client::SendRequestToSiblingServerService(Box::new(
                    RequestToSv2Server::MiningTrigger(RequestToSv2MiningServer::SetNewPrevHash(
                        prev_hash.into_static(),
                    )),
                )),
            ));
            Ok(response)
        }

        async fn handle_request_transaction_data_success(
            &self,
            _transaction_data: RequestTransactionDataSuccess<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_request_transaction_data_error(
            &self,
            _error: RequestTransactionDataError<'_>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }
    }

    // A Dummy Mining Server Handler that is not null, but not actually handling anything
    #[derive(Debug, Clone, Default)]
    struct DummyMiningServerHandler;
    impl Sv2MiningServerHandler for DummyMiningServerHandler {
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), RequestToSv2ServerError>> {
            Poll::Ready(Ok(()))
        }

        async fn on_new_template(
            &self,
            _m: NewTemplate<'static>,
        ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
            Ok(ResponseFromSv2Server::Ok)
        }

        async fn on_set_new_prev_hash(
            &self,
            _m: SetNewPrevHash<'static>,
        ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
            Ok(ResponseFromSv2Server::Ok)
        }

        fn setup_connection_success_flags(&self) -> u32 {
            0
        }

        async fn add_client(&mut self, _client_id: u32, _flags: u32) {}

        async fn remove_client(&mut self, _client_id: u32) {}

        async fn remove_all_clients(&mut self) {}

        async fn handle_open_standard_mining_channel(
            &self,
            _client_id: u32,
            _m: OpenStandardMiningChannel<'static>,
        ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
            Ok(ResponseFromSv2Server::Ok)
        }

        async fn handle_open_extended_mining_channel(
            &self,
            _client_id: u32,
            _m: OpenExtendedMiningChannel<'static>,
        ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
            Ok(ResponseFromSv2Server::Ok)
        }

        async fn handle_update_channel(
            &self,
            _client_id: u32,
            _m: UpdateChannel<'static>,
        ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
            Ok(ResponseFromSv2Server::Ok)
        }

        async fn handle_close_channel(
            &self,
            _client_id: u32,
            _m: CloseChannel<'static>,
        ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
            Ok(ResponseFromSv2Server::Ok)
        }

        async fn handle_submit_shares_standard(
            &self,
            _client_id: u32,
            _m: SubmitSharesStandard,
        ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
            Ok(ResponseFromSv2Server::Ok)
        }

        async fn handle_submit_shares_extended(
            &self,
            _client_id: u32,
            _m: SubmitSharesExtended<'static>,
        ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
            Ok(ResponseFromSv2Server::Ok)
        }

        async fn handle_set_custom_mining_job(
            &self,
            _client_id: u32,
            _m: SetCustomMiningJob<'static>,
        ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
            Ok(ResponseFromSv2Server::Ok)
        }
    }

    #[tokio::test]
    async fn sv2_client_service_initiate_connection_success() {
        // start a TemplateProvider
        let (_tp, tp_addr) = integration_tests_sv2::start_template_provider(None);

        let template_distribution_config = Sv2ClientServiceTemplateDistributionConfig {
            coinbase_output_constraints: (1, 1),
            server_addr: tp_addr,
            auth_pk: None,
            setup_connection_flags: 0,
        };

        let sv2_client_service_config = Sv2ClientServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            endpoint_host: Some("localhost".to_string()),
            endpoint_port: Some(8080),
            vendor: Some("test".to_string()),
            hardware_version: Some("test".to_string()),
            firmware: Some("test".to_string()),
            device_id: Some("test".to_string()),
            mining_config: None,
            job_declaration_config: None,
            template_distribution_config: Some(template_distribution_config.clone()),
        };

        let template_distribution_handler = DummyTemplateDistributionClientHandler;

        let mut sv2_client_service = Sv2ClientService::new(
            sv2_client_service_config,
            NullSv2MiningClientHandler,
            template_distribution_handler,
        )
        .unwrap();

        assert!(
            !sv2_client_service
                .is_connected(Protocol::TemplateDistributionProtocol)
                .await
        );

        let initiate_connection_request = RequestToSv2Client::SetupConnectionTrigger(
            Protocol::TemplateDistributionProtocol,
            template_distribution_config.setup_connection_flags,
        );

        sv2_client_service.ready().await.unwrap();
        let initiate_connection_response = sv2_client_service
            .call(initiate_connection_request)
            .await
            .unwrap();

        // Get the service ready again after the call
        sv2_client_service.ready().await.unwrap();

        // Verify the response matches what we expect
        assert!(matches!(
            initiate_connection_response,
            ResponseFromSv2Client::Ok
        ));

        // Check connection state on the updated service instance
        assert!(
            sv2_client_service
                .is_connected(Protocol::TemplateDistributionProtocol)
                .await
        );
    }

    #[tokio::test]
    async fn sv2_client_service_initiate_connection_error() {
        // start a TemplateProvider
        let (_tp, tp_addr) = integration_tests_sv2::start_template_provider(None);

        let sv2_client_service_config = Sv2ClientServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            endpoint_host: Some("localhost".to_string()),
            endpoint_port: Some(8080),
            vendor: Some("test".to_string()),
            hardware_version: Some("test".to_string()),
            firmware: Some("test".to_string()),
            device_id: Some("test".to_string()),
            mining_config: Some(Sv2ClientServiceMiningConfig {
                server_addr: tp_addr,
                auth_pk: None,
                setup_connection_flags: 0,
            }),
            job_declaration_config: None,
            template_distribution_config: None,
        };

        let mining_handler = DummyMiningClientHandler;

        let mut sv2_client_service = Sv2ClientService::new(
            sv2_client_service_config,
            mining_handler,
            NullSv2TemplateDistributionClientHandler,
        )
        .unwrap();

        assert!(
            !sv2_client_service
                .is_connected(Protocol::TemplateDistributionProtocol)
                .await
        );

        let sv2_client_service = sv2_client_service.ready().await.unwrap();

        let initiate_connection_request =
            RequestToSv2Client::SetupConnectionTrigger(Protocol::TemplateDistributionProtocol, 0);
        let _initiate_connection_result =
            sv2_client_service.call(initiate_connection_request).await;

        // Verify we get a SetupConnectionError and check its contents
        // match initiate_connection_result {
        //     Err(RequestToSv2ClientError::SetupConnectionError(error_code)) => {
        //         // The error should indicate that the protocol is unsupported
        //         assert!(error_code.contains("unsupported-protocol"),
        //             "Expected error about unsupported protocol, got: {}", error_code);
        //     }
        //     other => panic!("Expected SetupConnectionError, got: {:?}", other),
        // }

        // the assertion above is currently impossible, until the following is fixed:
        // https://github.com/Sjors/bitcoin/issues/84

        // Check connection state on the updated service instance - should still be disconnected
        assert!(
            !sv2_client_service
                .is_connected(Protocol::TemplateDistributionProtocol)
                .await
        );
    }

    #[tokio::test]
    async fn sv2_client_service_bad_config() {
        let config = Sv2ClientServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            endpoint_host: Some("localhost".to_string()),
            endpoint_port: Some(8080),
            vendor: Some("test".to_string()),
            hardware_version: Some("test".to_string()),
            firmware: Some("test".to_string()),
            device_id: Some("test".to_string()),
            mining_config: None,
            job_declaration_config: None,
            template_distribution_config: None,
        };

        // add a dummy template distribution handler to (which is not null)
        let template_distribution_handler = DummyTemplateDistributionClientHandler;

        let result = Sv2ClientService::new(
            config,
            NullSv2MiningClientHandler,
            template_distribution_handler.clone(),
        );

        // we expect an error, because the template distribution config is None
        assert!(result.is_err());

        let template_distribution_config = Sv2ClientServiceTemplateDistributionConfig {
            coinbase_output_constraints: (1, 1),
            server_addr: std::net::SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                8080,
            ),
            auth_pk: None,
            setup_connection_flags: 0,
        };

        // --------------------------------------------------------------------------------------------

        let config = Sv2ClientServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            endpoint_host: Some("localhost".to_string()),
            endpoint_port: Some(8080),
            vendor: Some("test".to_string()),
            hardware_version: Some("test".to_string()),
            firmware: Some("test".to_string()),
            device_id: Some("test".to_string()),
            mining_config: None,
            job_declaration_config: None,
            template_distribution_config: Some(template_distribution_config), // we are signaling that we support template distribution
        };

        // but now we are using a null template distribution handler, which is also not allowed
        let template_distribution_handler = NullSv2TemplateDistributionClientHandler;

        let result = Sv2ClientService::new(
            config,
            NullSv2MiningClientHandler,
            template_distribution_handler,
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn sv2_client_service_shutdown_when_not_connected() {
        let (_tp, tp_addr) = integration_tests_sv2::start_template_provider(None);

        let template_distribution_config = Sv2ClientServiceTemplateDistributionConfig {
            coinbase_output_constraints: (1, 1),
            server_addr: tp_addr,
            auth_pk: None,
            setup_connection_flags: 0,
        };

        let sv2_client_service_config = Sv2ClientServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            endpoint_host: Some("localhost".to_string()),
            endpoint_port: Some(8080),
            vendor: Some("test".to_string()),
            hardware_version: Some("test".to_string()),
            firmware: Some("test".to_string()),
            device_id: Some("test".to_string()),
            mining_config: None,
            job_declaration_config: None,
            template_distribution_config: Some(template_distribution_config),
        };

        let template_distribution_handler = DummyTemplateDistributionClientHandler;

        let mut sv2_client_service = Sv2ClientService::new(
            sv2_client_service_config,
            NullSv2MiningClientHandler,
            template_distribution_handler,
        )
        .unwrap();

        // Verify initial state
        assert!(
            !sv2_client_service
                .is_connected(Protocol::TemplateDistributionProtocol)
                .await
        );

        // Shutdown should work even when not connected
        sv2_client_service.shutdown().await;

        // Verify final state
        assert!(
            !sv2_client_service
                .is_connected(Protocol::TemplateDistributionProtocol)
                .await
        );
    }

    #[tokio::test]
    async fn sv2_client_service_shutdown_when_connected() {
        let (_tp, tp_addr) = integration_tests_sv2::start_template_provider(None);

        let mining_handler = NullSv2MiningClientHandler;

        let template_distribution_config = Sv2ClientServiceTemplateDistributionConfig {
            coinbase_output_constraints: (1, 1),
            server_addr: tp_addr,
            auth_pk: None,
            setup_connection_flags: 0,
        };

        let sv2_client_service_config = Sv2ClientServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            endpoint_host: Some("localhost".to_string()),
            endpoint_port: Some(8080),
            vendor: Some("test".to_string()),
            hardware_version: Some("test".to_string()),
            firmware: Some("test".to_string()),
            device_id: Some("test".to_string()),
            mining_config: None,
            job_declaration_config: None,
            template_distribution_config: Some(template_distribution_config.clone()),
        };

        let template_distribution_handler = DummyTemplateDistributionClientHandler;

        let mut sv2_client_service = Sv2ClientService::new(
            sv2_client_service_config,
            mining_handler,
            template_distribution_handler,
        )
        .unwrap();

        // Connect to the server
        sv2_client_service.ready().await.unwrap();
        let response = sv2_client_service
            .call(RequestToSv2Client::SetupConnectionTrigger(
                Protocol::TemplateDistributionProtocol,
                template_distribution_config.setup_connection_flags,
            ))
            .await
            .unwrap();
        assert!(matches!(response, ResponseFromSv2Client::Ok));

        // Start message listener in background
        let mut sv2_client_service_clone = sv2_client_service.clone();
        let (tx, mut rx) = mpsc::channel(1);
        tokio::spawn(async move {
            let result = sv2_client_service_clone
                .listen_for_messages_via_tcp(Protocol::TemplateDistributionProtocol)
                .await;
            tx.send(()).await.unwrap();
            result
        });

        // Verify connected state
        assert!(
            sv2_client_service
                .is_connected(Protocol::TemplateDistributionProtocol)
                .await
        );

        // Shutdown the client
        sv2_client_service.shutdown().await;

        // Wait for listener to exit
        rx.recv().await.unwrap();

        // Verify final state
        assert!(
            !sv2_client_service
                .is_connected(Protocol::TemplateDistributionProtocol)
                .await
        );
    }

    #[tokio::test]
    async fn test_sibling_io() {
        let mut server_config = Sv2ServerServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            inactivity_limit: 3600,
            tcp_config: Sv2ServerTcpConfig {
                listen_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3333),
                pub_key: Secp256k1PublicKey::from_str(
                    "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72",
                )
                .unwrap(),
                priv_key: Secp256k1SecretKey::from_str(
                    "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n",
                )
                .unwrap(),
                cert_validity: 3600,
            },
            mining_config: Some(Sv2ServerServiceMiningConfig {
                supported_flags: 0b0101,
            }),
            job_declaration_config: None,
            template_distribution_config: None,
        };

        let mut client_config = Sv2ClientServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            endpoint_host: None,
            endpoint_port: None,
            vendor: None,
            hardware_version: None,
            device_id: None,
            firmware: None,
            mining_config: None,
            job_declaration_config: None,
            template_distribution_config: Some(Sv2ClientServiceTemplateDistributionConfig {
                server_addr: SocketAddr::from_str("127.0.0.1:8442").unwrap(),
                auth_pk: None,
                coinbase_output_constraints: (1, 1),
                setup_connection_flags: 0,
            }),
        };

        // Start a Template Provider that simulates a Bitcoin node with SV2 support.
        let (_tp, tp_address) = start_template_provider(None);

        // Start a sniffer to intercept messages between the client and the Template Provider.
        let (tp_sniffer, tp_sniffer_addr) = start_sniffer("", tp_address, false, vec![]);

        // Update the client configuration to use the sniffer's address.
        if let Some(ref mut tdc) = client_config.template_distribution_config {
            tdc.server_addr = tp_sniffer_addr;
        }

        server_config.tcp_config.listen_address = SocketAddr::from_str("127.0.0.1:0").unwrap();

        // Allow some time for the sniffer to initialize.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Clone the template_distribution_config for later use to avoid moving it.
        let template_distribution_config = client_config
            .template_distribution_config
            .as_ref()
            .unwrap()
            .clone();

        // Initialize the handlers for TemplateDistribution and MiningServer.
        let tdc_handler = SiblingIoTemplateDistributionClientHandler;
        let mining_server_handler = DummyMiningServerHandler;

        // Create the Sv2ServerService and its sibling IO for communication with the client.
        let (
            mut server_service,
            sibling_server_io, // SiblingIO is created here.
        ) = Sv2ServerService::new_with_sibling_io(server_config, mining_server_handler).unwrap();

        // Create the Sv2ClientService and connect it to the server using the sibling IO.

        let mut client_service = Sv2ClientService::new_with_sibling_io(
            client_config.clone(),
            NullSv2MiningClientHandler,
            tdc_handler,
            sibling_server_io,
        )
        .unwrap();

        // Start the server and client services.
        server_service.start().await.unwrap();
        client_service.start().await.unwrap();

        // Trigger the Template Provider to set coinbase output constraints.
        client_service
            .call(RequestToSv2Client::TemplateDistributionTrigger(
                RequestToSv2TemplateDistributionClientService::SetCoinbaseOutputConstraints(
                    template_distribution_config.coinbase_output_constraints.0,
                    template_distribution_config.coinbase_output_constraints.1,
                ),
            ))
            .await
            .unwrap();

        // Wait for the sniffer to detect the coinbase output constraints message.
        tp_sniffer
            .wait_for_message_type(
                MessageDirection::ToUpstream,
                MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS,
            )
            .await;

        // Create a dummy NewTemplate message to simulate a new mining template.
        let new_template = NewTemplate {
            template_id: 0,
            future_template: false,
            version: 0,
            coinbase_tx_version: 0,
            coinbase_prefix: binary_codec_sv2::B0255::Owned(hex::decode("00").unwrap().to_vec()),
            coinbase_tx_input_sequence: 0,
            coinbase_tx_value_remaining: 0,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs: binary_codec_sv2::B064K::Owned(
                hex::decode("00").unwrap().to_vec(),
            ),
            coinbase_tx_locktime: 0,
            merkle_path: binary_codec_sv2::Seq0255::new(Vec::new()).unwrap(),
        };

        // Send the NewTemplate message to the sibling server and verify the response.
        let new_template_response = client_service
            .call(RequestToSv2Client::SendRequestToSiblingServerService(
                Box::new(RequestToSv2Server::MiningTrigger(
                    RequestToSv2MiningServer::NewTemplate(new_template),
                )),
            ))
            .await;

        // Assert that the response was sent to the sibling server.
        assert!(matches!(
            new_template_response.as_ref().unwrap(),
            ResponseFromSv2Client::Ok
        ));

        // Create a dummy SetNewPrevHash message to simulate a new previous hash.
        let new_prev_hash = SetNewPrevHash {
            template_id: 0,
            prev_hash: binary_codec_sv2::U256::Owned(hex::decode("00").unwrap().to_vec()),
            header_timestamp: 0,
            n_bits: 0,
            target: binary_codec_sv2::U256::Owned(hex::decode("00").unwrap().to_vec()),
        };

        // Send the SetNewPrevHash message to the sibling server and verify the response.
        let new_prev_hash_response = client_service
            .call(RequestToSv2Client::SendRequestToSiblingServerService(
                Box::new(RequestToSv2Server::MiningTrigger(
                    RequestToSv2MiningServer::SetNewPrevHash(new_prev_hash),
                )),
            ))
            .await
            .unwrap();

        // Assert that the response from the MiningServerHandler is received.
        assert!(matches!(new_prev_hash_response, ResponseFromSv2Client::Ok));

        // Shutdown the server and client services gracefully.
        server_service.shutdown().await;
        client_service.shutdown().await;
    }

    #[tokio::test]
    async fn test_sibling_io_with_wrong_constructor() {
        let mut server_config = Sv2ServerServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            inactivity_limit: 3600,
            tcp_config: Sv2ServerTcpConfig {
                listen_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3333),
                pub_key: Secp256k1PublicKey::from_str(
                    "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72",
                )
                .unwrap(),
                priv_key: Secp256k1SecretKey::from_str(
                    "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n",
                )
                .unwrap(),
                cert_validity: 3600,
            },
            mining_config: Some(Sv2ServerServiceMiningConfig {
                supported_flags: 0b0101,
            }),
            job_declaration_config: None,
            template_distribution_config: None,
        };

        let mut client_config = Sv2ClientServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            endpoint_host: None,
            endpoint_port: None,
            vendor: None,
            hardware_version: None,
            device_id: None,
            firmware: None,
            mining_config: None,
            job_declaration_config: None,
            template_distribution_config: Some(Sv2ClientServiceTemplateDistributionConfig {
                server_addr: SocketAddr::from_str("127.0.0.1:8442").unwrap(),
                auth_pk: None,
                coinbase_output_constraints: (1, 1),
                setup_connection_flags: 0,
            }),
        };

        // Start a Template Provider that simulates a Bitcoin node with SV2 support.
        let (_tp, tp_address) = start_template_provider(None);

        // Update the client configuration to use the sniffer's address.
        client_config
            .template_distribution_config
            .as_mut()
            .unwrap()
            .server_addr = tp_address;

        server_config.tcp_config.listen_address = SocketAddr::from_str("127.0.0.1:0").unwrap();

        // Allow some time for the sniffer to initialize.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Initialize the handlers for TemplateDistribution and MiningServer.
        let tdc_handler = SiblingIoTemplateDistributionClientHandler;
        let mining_handler = DummyMiningServerHandler;

        // Create the Sv2ServerService with a wrong constructor.
        let mut server_service = Sv2ServerService::new(server_config, mining_handler).unwrap();
        // Create the Sv2ClientService with a wrong constructor.
        let mut client_service =
            Sv2ClientService::new(client_config, NullSv2MiningClientHandler, tdc_handler).unwrap();

        // Start the server and client services.
        server_service.start().await.unwrap();
        client_service.start().await.unwrap();

        // Create a dummy NewTemplate message to simulate a new mining template.
        let new_template = NewTemplate {
            template_id: 0,
            future_template: false,
            version: 0,
            coinbase_tx_version: 0,
            coinbase_prefix: binary_codec_sv2::B0255::Owned(hex::decode("00").unwrap().to_vec()),
            coinbase_tx_input_sequence: 0,
            coinbase_tx_value_remaining: 0,
            coinbase_tx_outputs_count: 0,
            coinbase_tx_outputs: binary_codec_sv2::B064K::Owned(
                hex::decode("00").unwrap().to_vec(),
            ),
            coinbase_tx_locktime: 0,
            merkle_path: binary_codec_sv2::Seq0255::new(Vec::new()).unwrap(),
        };

        // Send the NewTemplate message to the sibling server and verify the response.
        let new_template_response = client_service
            .call(RequestToSv2Client::SendRequestToSiblingServerService(
                Box::new(RequestToSv2Server::MiningTrigger(
                    RequestToSv2MiningServer::NewTemplate(new_template),
                )),
            ))
            .await;

        // Assert that the error is of type RequestToSv2ClientError::NoSiblingServerServiceIo.
        assert!(matches!(
            new_template_response,
            Err(RequestToSv2ClientError::NoSiblingServerServiceIo)
        ));

        // Shutdown the server and client services gracefully.
        server_service.shutdown().await;
        client_service.shutdown().await;
    }

    #[tokio::test]
    async fn sv2_client_service_submit_solution() {
        use crate::client::service::request::RequestToSv2Client;
        use crate::client::service::response::ResponseFromSv2Client;
        use crate::client::service::subprotocols::template_distribution::request::RequestToSv2TemplateDistributionClientService;
        use roles_logic_sv2::common_messages_sv2::Protocol;
        use roles_logic_sv2::template_distribution_sv2::SubmitSolution;

        // Start a TemplateProvider
        let (_tp, tp_addr) = integration_tests_sv2::start_template_provider(None);

        let template_distribution_config = Sv2ClientServiceTemplateDistributionConfig {
            coinbase_output_constraints: (1, 1),
            server_addr: tp_addr,
            auth_pk: None,
            setup_connection_flags: 0,
        };

        let sv2_client_service_config = Sv2ClientServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            endpoint_host: Some("localhost".to_string()),
            endpoint_port: Some(8080),
            vendor: Some("test".to_string()),
            hardware_version: Some("test".to_string()),
            firmware: Some("test".to_string()),
            device_id: Some("test".to_string()),
            mining_config: None,
            job_declaration_config: None,
            template_distribution_config: Some(template_distribution_config.clone()),
        };

        let template_distribution_handler = DummyTemplateDistributionClientHandler;

        let mut sv2_client_service = Sv2ClientService::new(
            sv2_client_service_config,
            NullSv2MiningClientHandler,
            template_distribution_handler,
        )
        .unwrap();

        // Connect to the server
        sv2_client_service.ready().await.unwrap();
        let initiate_connection_request = RequestToSv2Client::SetupConnectionTrigger(
            Protocol::TemplateDistributionProtocol,
            template_distribution_config.setup_connection_flags,
        );
        let response = sv2_client_service
            .call(initiate_connection_request)
            .await
            .unwrap();
        assert!(matches!(response, ResponseFromSv2Client::Ok));

        // Construct a dummy SubmitSolution message
        let submit_solution = SubmitSolution {
            template_id: 0,
            version: 0,
            header_timestamp: 0,
            header_nonce: 0,
            coinbase_tx: binary_codec_sv2::B064K::Owned(vec![]),
        };

        let submit_solution_request = RequestToSv2Client::TemplateDistributionTrigger(
            RequestToSv2TemplateDistributionClientService::SubmitSolution(submit_solution),
        );

        let response = sv2_client_service.call(submit_solution_request).await;
        assert!(matches!(response, Ok(ResponseFromSv2Client::Ok)));
    }
}
