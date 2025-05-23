use crate::client::service::config::Sv2ClientServiceConfig;
use crate::client::service::error::Sv2ClientServiceError;
use crate::client::service::request::{RequestToSv2Client, RequestToSv2ClientError};
use crate::client::service::response::ResponseFromSv2Client;
use crate::client::service::sibling::Sv2SiblingServerServiceIo;
use crate::client::service::subprotocols::template_distribution::handler::NullSv2TemplateDistributionClientHandler;
use crate::client::service::subprotocols::template_distribution::handler::Sv2TemplateDistributionClientHandler;
use crate::client::service::subprotocols::template_distribution::request::RequestToSv2TemplateDistributionClientService;
use crate::client::service::subprotocols::template_distribution::response::ResponseToTemplateDistributionTrigger;
use crate::client::tcp::Sv2TcpClient;

use const_sv2::MESSAGE_TYPE_COINBASE_OUTPUT_CONSTRAINTS;
use const_sv2::MESSAGE_TYPE_SETUP_CONNECTION;
use roles_logic_sv2::common_messages_sv2::{Protocol, SetupConnection};
use roles_logic_sv2::parsers::{AnyMessage, CommonMessages, TemplateDistribution};
use roles_logic_sv2::template_distribution_sv2::CoinbaseOutputConstraints;
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
pub struct Sv2ClientService<T>
// todo: add J and T generic parameters
where
    T: Sv2TemplateDistributionClientHandler + Clone + Send + Sync + 'static,
{
    config: Sv2ClientServiceConfig,
    mining_tcp_client: Arc<RwLock<Option<Sv2TcpClient>>>,
    job_declaration_tcp_client: Arc<RwLock<Option<Sv2TcpClient>>>,
    template_distribution_tcp_client: Arc<RwLock<Option<Sv2TcpClient>>>,
    // todo: add mining_handler: M,
    // todo: add job_declaration_handler: J,
    template_distribution_handler: T,
    shutdown_tx: broadcast::Sender<()>,
    sibling_server_service_io: Option<Sv2SiblingServerServiceIo>,
}

impl<T> Sv2ClientService<T>
where
    T: Sv2TemplateDistributionClientHandler + Clone + Send + Sync + 'static,
{
    /// Creates a new [`Sv2ClientService`]
    ///
    /// No sibling server service is required.
    pub fn new(
        config: Sv2ClientServiceConfig,
        template_distribution_handler: T,
        // todo: add mining_handler: M,
        // todo: add job_declaration_handler: J,
    ) -> Result<Self, Sv2ClientServiceError> {
        let sv2_client_service = Self::_new(config, template_distribution_handler, None)?;
        Ok(sv2_client_service)
    }

    /// Creates a new [`Sv2ClientService`] with a sibling server service.
    ///
    /// The sibling server service is used to send and receive requests to a sibling [`crate::server::service::Sv2ServerService`] that pairs with this client.    
    ///
    /// Before calling this, you need to create a [`Sv2SiblingServerServiceIo`] using [`crate::server::service::Sv2ServerService::new_with_sibling_io`].
    pub fn new_with_sibling_io(
        config: Sv2ClientServiceConfig,
        template_distribution_handler: T,
        sibling_server_service_io: Sv2SiblingServerServiceIo,
    ) -> Result<Self, Sv2ClientServiceError> {
        let sv2_client_service = Self::_new(
            config,
            template_distribution_handler,
            Some(sibling_server_service_io),
        )?;
        Ok(sv2_client_service)
    }

    // internal constructor
    fn _new(
        config: Sv2ClientServiceConfig,
        // todo: add mining_handler: M,
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
        // Check if template_distribution_handler is NullSv2TemplateDistributionClientHandler
        let is_null_template_distribution_handler = std::any::TypeId::of::<T>()
            == std::any::TypeId::of::<NullSv2TemplateDistributionClientHandler>();

        // Check if template_distribution_handler is compatible with the supported protocols
        if config
            .supported_protocols()
            .contains(&Protocol::TemplateDistributionProtocol)
        {
            if is_null_template_distribution_handler {
                return Err(Sv2ClientServiceError::NullHandlerForSupportedProtocol {
                    protocol: Protocol::TemplateDistributionProtocol,
                });
            }
        } else {
            if !is_null_template_distribution_handler {
                return Err(
                    Sv2ClientServiceError::NonNullHandlerForUnsupportedProtocol {
                        protocol: Protocol::TemplateDistributionProtocol,
                    },
                );
            }
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

        for protocol in self.config.supported_protocols() {
            let initiate_connection_response = self
                .call(RequestToSv2Client::SetupConnectionTrigger(protocol))
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
                    let config = self.config.mining_config.as_ref().ok_or_else(|| {
                        RequestToSv2ClientError::UnsupportedProtocol {
                            protocol: Protocol::MiningProtocol,
                        }
                    })?;
                    let tcp_client = Sv2TcpClient::new(
                        config.server_addr,
                        self.config.encrypted,
                        config.auth_pk,
                    )
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
                    let config = self.config.job_declaration_config.as_ref().ok_or_else(|| {
                        RequestToSv2ClientError::UnsupportedProtocol {
                            protocol: Protocol::JobDeclarationProtocol,
                        }
                    })?;
                    let tcp_client = Sv2TcpClient::new(
                        config.server_addr,
                        self.config.encrypted,
                        config.auth_pk,
                    )
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
                    let config = self
                        .config
                        .template_distribution_config
                        .as_ref()
                        .ok_or_else(|| RequestToSv2ClientError::UnsupportedProtocol {
                            protocol: Protocol::TemplateDistributionProtocol,
                        })?;
                    let tcp_client = Sv2TcpClient::new(
                        config.server_addr,
                        self.config.encrypted,
                        config.auth_pk,
                    )
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
            protocol: protocol,
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
            .io()
            .send_message(setup_connection.into(), MESSAGE_TYPE_SETUP_CONNECTION)
            .await?;

        // wait for the server to respond with a SetupConnectionSuccess or SetupConnectionError
        // and return the appropriate response
        let (message, _) = tcp_client.io().recv_message().await?;
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

        let tcp_client: Sv2TcpClient = match protocol {
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
                message_result = tcp_client.io().recv_message() => {
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

impl<T> Service<RequestToSv2Client<'static>> for Sv2ClientService<T>
where
    T: Sv2TemplateDistributionClientHandler + Clone + Send + Sync + 'static,
{
    type Response = ResponseFromSv2Client<'static>;
    type Error = RequestToSv2ClientError;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    /// Polls readiness of each subprotocol handler.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // let mining_poll_ready = match Self::has_null_handler(Protocol::MiningProtocol) {
        //     true => Poll::Ready(Ok(())),
        //     false => self.mining_handler.poll_ready(cx),
        // };

        let mining_poll_ready = Poll::Ready(Ok(()));

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
                RequestToSv2Client::SetupConnectionTrigger(protocol) => match protocol {
                    Protocol::MiningProtocol => {
                        debug!(
                                "Sv2ClientService received a trigger request for initiating a connection under the Mining protocol"
                            );
                        if this.config.mining_config.is_none() {
                            return Err(RequestToSv2ClientError::UnsupportedProtocol {
                                protocol: Protocol::MiningProtocol,
                            });
                        }
                        this.initiate_connection(protocol, 0).await
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
                        _ => todo!(),
                    }
                }
                RequestToSv2Client::TemplateDistributionTrigger(request) => match request {
                    RequestToSv2TemplateDistributionClientService::SetCoinbaseOutputConstraints(
                        max_additional_size,
                        max_additional_sigops,
                    ) => {
                        debug!("Sv2ClientService received a trigger request for sending CoinbaseOutputConstraints");

                        // check if this service is configured for template distribution
                        if this.config.template_distribution_config.is_none()
                            || std::any::TypeId::of::<T>()
                                == std::any::TypeId::of::<NullSv2TemplateDistributionClientHandler>(
                                )
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
                            .io()
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
                                Ok(ResponseFromSv2Client::ResponseToTemplateDistributionTrigger(ResponseToTemplateDistributionTrigger::SuccessfullySetCoinbaseOutputConstraints))
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
                },
                RequestToSv2Client::SendRequestToSiblingServerService(req) => {
                    debug!("Sv2ClientService received a SendRequestToSiblingServerService request");
                    match this.sibling_server_service_io {
                        Some(ref io) => {
                            io.send(*req.clone()).map_err(|_| {
                                RequestToSv2ClientError::FailedToSendRequestToSiblingServerService
                            })?;
                            Ok(ResponseFromSv2Client::SentRequestToSiblingServerService(
                                *req,
                            ))
                        }
                        None => {
                            error!("No sibling server service on Sv2ClientService");
                            Err(RequestToSv2ClientError::NoSiblingServerServiceIo)
                        }
                    }
                }
            };

            // allows for recursive chaining of requests
            let response = if let Ok(ResponseFromSv2Client::TriggerNewRequest(request)) = response {
                this.call(request).await
            } else {
                response
            };

            response
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
    use crate::client::service::subprotocols::template_distribution::handler::NullSv2TemplateDistributionClientHandler;
    use crate::client::service::subprotocols::template_distribution::handler::Sv2TemplateDistributionClientHandler;
    use crate::client::service::RequestToSv2ClientError;
    use crate::client::service::Sv2ClientService;
    use roles_logic_sv2::common_messages_sv2::Protocol;
    use roles_logic_sv2::template_distribution_sv2::{
        NewTemplate, RequestTransactionDataError, RequestTransactionDataSuccess, SetNewPrevHash,
    };
    use std::task::{Context, Poll};
    use tokio::sync::mpsc;
    use tower::{Service, ServiceExt};

    // A dummy template distribution handler that is not null, but not actually handling anything
    #[derive(Debug, Clone, Default)]
    struct DummyTemplateDistributionHandler;

    impl Sv2TemplateDistributionClientHandler for DummyTemplateDistributionHandler {
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), RequestToSv2ClientError>> {
            Poll::Ready(Ok(()))
        }

        async fn handle_new_template(
            &self,
            _template: NewTemplate<'static>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_set_new_prev_hash(
            &self,
            _prev_hash: SetNewPrevHash<'static>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_request_transaction_data_success(
            &self,
            _transaction_data: RequestTransactionDataSuccess<'static>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
        }

        async fn handle_request_transaction_data_error(
            &self,
            _error: RequestTransactionDataError<'static>,
        ) -> Result<ResponseFromSv2Client<'static>, RequestToSv2ClientError> {
            Ok(ResponseFromSv2Client::Ok)
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
            encrypted: true,
        };

        let template_distribution_handler = DummyTemplateDistributionHandler;

        let mut sv2_client_service =
            Sv2ClientService::new(sv2_client_service_config, template_distribution_handler)
                .unwrap();

        assert!(
            !sv2_client_service
                .is_connected(Protocol::TemplateDistributionProtocol)
                .await
        );

        let initiate_connection_request =
            RequestToSv2Client::SetupConnectionTrigger(Protocol::TemplateDistributionProtocol);

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
            }),
            job_declaration_config: None,
            template_distribution_config: None,
            encrypted: true,
        };

        let template_distribution_handler = NullSv2TemplateDistributionClientHandler;

        let mut sv2_client_service =
            Sv2ClientService::new(sv2_client_service_config, template_distribution_handler)
                .unwrap();

        assert!(
            !sv2_client_service
                .is_connected(Protocol::TemplateDistributionProtocol)
                .await
        );

        let sv2_client_service = sv2_client_service.ready().await.unwrap();

        let initiate_connection_request =
            RequestToSv2Client::SetupConnectionTrigger(Protocol::TemplateDistributionProtocol);
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
            encrypted: true,
        };

        // add a dummy template distribution handler to (which is not null)
        let template_distribution_handler = DummyTemplateDistributionHandler;

        let result = Sv2ClientService::new(config, template_distribution_handler.clone());

        // we expect an error, because the template distribution config is None
        assert!(result.is_err());

        let template_distribution_config = Sv2ClientServiceTemplateDistributionConfig {
            coinbase_output_constraints: (1, 1),
            server_addr: std::net::SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                8080,
            ),
            auth_pk: None,
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
            encrypted: true,
        };

        // but now we are using a null template distribution handler, which is also not allowed
        let template_distribution_handler = NullSv2TemplateDistributionClientHandler;

        let result = Sv2ClientService::<NullSv2TemplateDistributionClientHandler>::new(
            config,
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
            encrypted: true,
        };

        let template_distribution_handler = DummyTemplateDistributionHandler;

        let mut sv2_client_service =
            Sv2ClientService::new(sv2_client_service_config, template_distribution_handler)
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

        let template_distribution_config = Sv2ClientServiceTemplateDistributionConfig {
            coinbase_output_constraints: (1, 1),
            server_addr: tp_addr,
            auth_pk: None,
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
            encrypted: true,
        };

        let template_distribution_handler = DummyTemplateDistributionHandler;

        let mut sv2_client_service =
            Sv2ClientService::new(sv2_client_service_config, template_distribution_handler)
                .unwrap();

        // Connect to the server
        sv2_client_service.ready().await.unwrap();
        let response = sv2_client_service
            .call(RequestToSv2Client::SetupConnectionTrigger(
                Protocol::TemplateDistributionProtocol,
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
}
