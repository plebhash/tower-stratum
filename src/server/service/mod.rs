use crate::client::service::sibling::Sv2SiblingServerServiceIo;
use crate::server::service::client::{Sv2MessagesToClient, Sv2ServerServiceClient};
use crate::server::service::config::Sv2ServerServiceConfig;
use crate::server::service::connection::Sv2ConnectionClient;
use crate::server::service::error::Sv2ServerServiceError;
use crate::server::service::request::{
    RequestToSv2Server, RequestToSv2ServerError, Sv2MessageToServer,
};
use crate::server::service::response::ResponseFromSv2Server;
use crate::server::service::sibling::Sv2SiblingClientServiceIo;
use crate::server::service::subprotocols::mining::handler::NullSv2MiningServerHandler;
use crate::server::service::subprotocols::mining::handler::Sv2MiningServerHandler;
use crate::server::service::subprotocols::mining::trigger::MiningServerTrigger;
use crate::server::tcp::encrypted::start_encrypted_tcp_server;
use crate::server::ClientIdGenerator;
use dashmap::DashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use stratum_common::roles_logic_sv2::common_messages_sv2::{
    Protocol, SetupConnection, SetupConnectionError, SetupConnectionSuccess,
};
use stratum_common::roles_logic_sv2::parsers::{AnyMessage, CommonMessages, Mining};
use tokio_util::sync::CancellationToken;
use tower::{Service, ServiceExt};
use tracing::{debug, error};

pub mod client;
pub mod config;
pub mod connection;
pub mod error;
pub mod request;
pub mod response;
pub mod sibling;
pub mod subprotocols;

/// A [`tower::Service`] implementer that provides:
/// - TCP server functionality
/// - Client connection management
/// - Optional handlers for Mining, Job Declaration, and Template Distribution Sv2 subprotocols
/// - Ability to listen for messages from the client and trigger Service Requests
///
/// Inactive clients have their connections killed and are removed from memory after some predefined time (configurable via [`config::Sv2ServerServiceConfig`]).
///
/// The `M` generic parameter is the handler for the Mining subprotocol.
/// If the service does not support mining subprotocol, `M` should be set to [`NullSv2MiningServerHandler`].
///
/// The `J` generic parameter is the handler for the Job Declaration subprotocol.
/// If the service does not support job declaration subprotocol, `J` should be set to [`NullSv2JobDeclarationServerHandler`].
///
/// The `T` generic parameter is the handler for the Template Distribution subprotocol.
/// If the service does not support template distribution subprotocol, `T` should be set to [`NullSv2TemplateDistributionServerHandler`].
#[derive(Debug, Clone)]
pub struct Sv2ServerService<M>
// todo: add J and T generic parameters
where
    M: Sv2MiningServerHandler + Clone + Send + Sync + 'static,
{
    config: Sv2ServerServiceConfig,
    clients: Arc<DashMap<u32, Arc<Sv2ServerServiceClient>>>,
    client_id_generator: ClientIdGenerator,
    mining_handler: M,
    // todo: job_declaration_handler: J,
    // todo: template_distribution_handler: T,
    sibling_client_service_io: Option<Sv2SiblingClientServiceIo>,
    cancellation_token: CancellationToken,
}

impl<M> Sv2ServerService<M>
where
    M: Sv2MiningServerHandler + Clone + Send + Sync + 'static,
{
    /// Creates a new [`Sv2ServerService`]
    ///
    /// No sibling client service is required.
    pub fn new(
        config: Sv2ServerServiceConfig,
        mining_handler: M,
        // todo: job_declaration_handler: J,
        // todo: template_distribution_handler: T,
        cancellation_token: CancellationToken,
    ) -> Result<Self, Sv2ServerServiceError> {
        let sv2_server_service = Self::_new(config, mining_handler, None, cancellation_token)?;
        Ok(sv2_server_service)
    }

    /// Creates a new [`Sv2ServerService`] plus a new [`Sv2SiblingServerServiceIo`].
    ///
    /// The [`Sv2SiblingClientServiceIo`] can be used as input to [`crate::client::service::Sv2ClientService::new_from_sibling_io`] to create a sibling client service that pairs with this server.
    pub fn new_with_sibling_io(
        config: Sv2ServerServiceConfig,
        mining_handler: M,
        cancellation_token: CancellationToken,
    ) -> Result<(Self, Sv2SiblingServerServiceIo), Sv2ServerServiceError> {
        let (sibling_client_service_io, sibling_server_service_io) =
            Sv2SiblingClientServiceIo::new();
        let sv2_server_service = Self::_new(
            config,
            mining_handler,
            Some(sibling_client_service_io),
            cancellation_token,
        )?;
        Ok((sv2_server_service, sibling_server_service_io))
    }

    // internal constructor
    fn _new(
        config: Sv2ServerServiceConfig,
        mining_handler: M,
        // todo: job_declaration_handler: J,
        // todo: template_distribution_handler: T,
        sibling_client_service_io: Option<Sv2SiblingClientServiceIo>,
        cancellation_token: CancellationToken,
    ) -> Result<Self, Sv2ServerServiceError> {
        Self::validate_protocol_handlers(&config)?;

        let sv2_server_service = Sv2ServerService {
            config: config.clone(),
            clients: Arc::new(DashMap::new()),
            client_id_generator: ClientIdGenerator::new(),
            mining_handler,
            sibling_client_service_io,
            cancellation_token,
        };

        Ok(sv2_server_service)
    }

    pub async fn start(&mut self) -> Result<(), Sv2ServerServiceError> {
        // Create a channel for new client connections
        let (new_client_tx, mut new_client_rx) = tokio::sync::mpsc::channel(32);

        let cancellation_token = self.cancellation_token.clone();

        start_encrypted_tcp_server(
            self.config.tcp_config.listen_address,
            self.config.tcp_config.pub_key,
            self.config.tcp_config.priv_key,
            self.config.tcp_config.cert_validity,
            new_client_tx,
            cancellation_token.clone(),
        )
        .await
        .map_err(|_e| Sv2ServerServiceError::TcpServerError)?;

        let clients = self.clients.clone();
        let inactivity_limit = self.config.inactivity_limit;
        let mut this = self.clone();

        // spawn a task to monitor for inactive connections and clean up the DashMap
        tokio::spawn(async move {
            let cancellation_token = cancellation_token;
            loop {
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        debug!("Inactive connection monitor task cancelled");
                        this.remove_all_clients().await;
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                        let mut clients_to_remove = Vec::new();
                        // Identify inactive clients
                        for entry in clients.iter() {
                            let client_id = *entry.key();
                            let client = entry.value();
                            if client.is_inactive(inactivity_limit) {
                                clients_to_remove.push(client_id);
                            }
                        }

                        if !clients_to_remove.is_empty() {
                            for client_id in clients_to_remove {
                                this.remove_client(client_id).await;
                            }
                        }
                    }
                }
            }
            debug!("Inactive connection monitor task ended");
        });

        let service = self.clone();
        let cancellation_token = self.cancellation_token.clone();

        // Spawn a task to handle new client connections
        let clients = self.clients.clone();
        let mut client_id_generator = self.client_id_generator.clone();
        tokio::spawn(async move {
            let cancellation_token = cancellation_token;
            loop {
                tokio::select! {
                    _ = cancellation_token.cancelled() => {
                        debug!("New client connection handler task cancelled");
                        break;
                    }
                    Some(io) = new_client_rx.recv() => {
                        let client = Sv2ServerServiceClient::new(io.clone());
                        let client_id = client_id_generator.next();
                        clients.insert(client_id, Arc::new(client));
                        debug!("added new client with id: {}", client_id);

                        // Spawn a task to handle incoming messages from this client
                        let mut service = service.clone();
                        let cancellation_token = cancellation_token.clone();
                        tokio::spawn(async move {
                            let cancellation_token = cancellation_token;
                            loop {
                                tokio::select! {
                                    _ = cancellation_token.cancelled() => {
                                        debug!("Client {} message handler task cancelled", client_id);
                                        break;
                                    }
                                    message_result = io.recv_message() => {
                                        match message_result {
                                            Ok(message) => {
                                                let request = RequestToSv2Server::IncomingMessage(Sv2MessageToServer {
                                                    message,
                                                    client_id: Some(client_id),
                                                });

                                                // Call the service with the request
                                                if let Err(e) = service.call(request).await {
                                                    // this is a protection from attacks where a client sends a message that the server cannot handle
                                                    // we simply log the error and ignore the message, without shutting down the task
                                                    error!(
                                                        "Error handling message from client_id {}: {:?}, message will be ignored",
                                                        client_id,
                                                        e
                                                    );
                                                }
                                            }
                                            Err(_) => {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            debug!("Client {} message handler task ended", client_id);
                            service.remove_client(client_id).await;
                        });
                    }
                }
            }
        });

        let cancellation_token = self.cancellation_token.clone();
        let mut service = self.clone();

        // spawn a task to route requests from the sibling client service
        if let Some(sibling_io) = service.sibling_client_service_io.clone() {
            tokio::spawn(async move {
                let cancellation_token = cancellation_token;

                loop {
                    tokio::select! {
                        _ = cancellation_token.cancelled() => {
                            debug!("Sibling client service request monitor task cancelled");
                            break;
                        }
                        result = sibling_io.recv() => {
                            match result {
                                Ok(req) => {
                                    debug!("Received request from sibling client service");

                                    // Check if service is ready before processing the request
                                    if let Err(e) = service.ready().await.map_err(|_| Sv2ServerServiceError::ServiceNotReady) {
                                        error!("Service not ready, skipping request: {:?}", e);
                                        continue;
                                    }

                                    // Call the service with the request
                                    if let Err(e) = service.call(*req).await {
                                        error!(
                                            "Error handling request from sibling client service: {:?}",
                                            e
                                        );
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to receive request from sibling client service: {:?}", e);
                                }
                            }
                        }
                    }
                }
                debug!("Sibling client service request monitor task ended");
                sibling_io.shutdown();
            });
        }

        if !Self::has_null_handler(Protocol::MiningProtocol) {
            self.ready()
                .await
                .map_err(|_| Sv2ServerServiceError::ServiceNotReady)?;
            match self
                .call(RequestToSv2Server::MiningTrigger(
                    MiningServerTrigger::Start,
                ))
                .await
            {
                Ok(_) => {
                    debug!("Mining handler started");
                }
                Err(e) => {
                    error!("Failed to start mining handler: {:?}", e);
                    return Err(Sv2ServerServiceError::FailedToStartMiningHandler);
                }
            }
        }

        // todo: start job declaration handler
        // todo: start template distribution handler

        debug!("Sv2ServerService started");

        // wait for cancellation token to be cancelled
        self.cancellation_token.cancelled().await;

        Ok(())
    }

    async fn remove_client(&mut self, client_id: u32) {
        if !Self::has_null_handler(Protocol::MiningProtocol) {
            self.mining_handler.remove_client(client_id).await;
        }

        // todo: remove client from other subprotocols

        if let Some((_, client)) = self.clients.remove(&client_id) {
            client.io.shutdown();
        }
    }

    async fn remove_all_clients(&mut self) {
        let client_entries: Vec<_> = self
            .clients
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect();

        for (client_id, client) in client_entries {
            client.io.shutdown();

            if !Self::has_null_handler(Protocol::MiningProtocol) {
                self.mining_handler.remove_client(client_id).await;
            }

            // todo: remove client from other subprotocols
        }

        self.clients.clear();
    }

    fn has_null_handler(protocol: Protocol) -> bool {
        match protocol {
            Protocol::MiningProtocol => {
                std::any::TypeId::of::<M>() == std::any::TypeId::of::<NullSv2MiningServerHandler>()
            }
            // todo: add checks for job_declaration_handler and template_distribution_handler
            _ => false,
        }
    }

    // Validates that the protocol handlers are consistent with the supported protocols.
    // Returns an error if:
    // - A protocol is configured as supported but the corresponding handler is null.
    // - A protocol is not configured as supported but a non-null handler is provided.
    fn validate_protocol_handlers(
        config: &Sv2ServerServiceConfig,
    ) -> Result<(), Sv2ServerServiceError> {
        // Check if mining_handler is NullSv2MiningServerHandler
        let is_null_mining_handler = Self::has_null_handler(Protocol::MiningProtocol);

        // Check if mining_handler is compatible with the supported protocols
        if config
            .supported_protocols()
            .contains(&Protocol::MiningProtocol)
        {
            if is_null_mining_handler {
                return Err(Sv2ServerServiceError::NullHandlerForSupportedProtocol {
                    protocol: Protocol::MiningProtocol,
                });
            }
        } else if !is_null_mining_handler {
            return Err(
                Sv2ServerServiceError::NonNullHandlerForUnsupportedProtocol {
                    protocol: Protocol::MiningProtocol,
                },
            );
        }

        // todo: add checks for job_declaration_handler and template_distribution_handler

        Ok(())
    }

    /// Returns `Some` if there is an active [`client::Sv2ServerServiceClient`] on the requested index, `None` otherwise.
    pub fn get_client(&self, client_id: u32) -> Option<Arc<Sv2ServerServiceClient>> {
        self.clients.get(&client_id).map(|entry| entry.clone())
    }

    /// Returns how many [`client::Sv2ServerServiceClient`] are active.
    pub fn get_client_count(&self) -> usize {
        self.clients.len()
    }

    /// Updates the last message time for a given client
    pub fn update_client_message_time(&self, client_id: u32) -> bool {
        if let Some(client_entry) = self.clients.get(&client_id) {
            client_entry.update_last_message_time();
            true
        } else {
            false
        }
    }

    /// The core logic for handling a [`SetupConnection`] request:
    /// 1) Check that the requested subprotocol is supported.
    /// 2) Negotiate an overlapping version.
    /// 3) Check that requested flags are supported (else return which flags are unsupported).
    /// 4) If success, populate the client's connection details
    /// 5) Return either [`SetupConnectionSuccess`] or [`SetupConnectionError`].
    pub async fn handle_setup_connection(
        &mut self,
        req: SetupConnection<'static>,
        client_id: u32,
    ) -> Result<ResponseFromSv2Server<'static>, RequestToSv2ServerError> {
        debug!(
            "Sv2ServerService received a SetupConnection request: {:?}",
            req
        );

        // 1) Check subprotocol
        if !self.config.supported_protocols().contains(&req.protocol) {
            let setup_connection_error = SetupConnectionError {
                flags: 0,
                error_code: "unsupported-protocol"
                    .to_string()
                    .into_bytes()
                    .try_into()
                    .expect("failed to encode string"),
            };

            let response = ResponseFromSv2Server::TriggerNewRequest(Box::new(
                RequestToSv2Server::SendMessagesToClient(Box::new(Sv2MessagesToClient {
                    client_id,
                    messages: vec![setup_connection_error.into()],
                })),
            ));
            return Ok(response);
        }

        // 2) Check version support
        if req.max_version < self.config.min_supported_version
            || req.min_version > self.config.max_supported_version
        {
            let setup_connection_error = SetupConnectionError {
                flags: 0,
                error_code: "protocol-version-mismatch"
                    .to_string()
                    .into_bytes()
                    .try_into()
                    .expect("failed to encode string"),
            };

            let response = ResponseFromSv2Server::TriggerNewRequest(Box::new(
                RequestToSv2Server::SendMessagesToClient(Box::new(Sv2MessagesToClient {
                    client_id,
                    messages: vec![setup_connection_error.into()],
                })),
            ));
            return Ok(response);
        }

        // Choose an actual version to use.
        let used_version = std::cmp::min(req.max_version, self.config.max_supported_version);

        // 3) Flags check
        let supported_flags = match req.protocol {
            Protocol::MiningProtocol => {
                self.config
                    .mining_config
                    .as_ref()
                    .expect("Mining config must be Some")
                    .supported_flags
            }
            Protocol::JobDeclarationProtocol => {
                self.config
                    .job_declaration_config
                    .as_ref()
                    .expect("Job declaration config must be Some")
                    .supported_flags
            }
            Protocol::TemplateDistributionProtocol => {
                self.config
                    .template_distribution_config
                    .as_ref()
                    .expect("Template distribution config must be Some")
                    .supported_flags
            }
        };
        let unsupported_flags = req.flags & !supported_flags;
        if unsupported_flags != 0 {
            let setup_connection_error = SetupConnectionError {
                flags: unsupported_flags,
                error_code: "unsupported-feature-flags"
                    .to_string()
                    .into_bytes()
                    .try_into()
                    .expect("failed to encode string"),
            };

            let response = ResponseFromSv2Server::TriggerNewRequest(Box::new(
                RequestToSv2Server::SendMessagesToClient(Box::new(Sv2MessagesToClient {
                    client_id,
                    messages: vec![setup_connection_error.into()],
                })),
            ));

            if !Self::has_null_handler(Protocol::MiningProtocol) {
                self.mining_handler.add_client(client_id, req.flags).await;
            }

            return Ok(response);
        }

        // 4) Create connection details and update client
        let connection = Sv2ConnectionClient {
            protocol: req.protocol,
            min_version: req.min_version,
            max_version: req.max_version,
            flags: req.flags,
            endpoint_host: req.endpoint_host,
            endpoint_port: req.endpoint_port,
            vendor: req.vendor,
            hardware_version: req.hardware_version,
            firmware: req.firmware,
            device_id: req.device_id,
        };

        if let Some(client_entry) = self.clients.get(&client_id) {
            *client_entry.connection.write().await = Some(connection);
        } else {
            return Err(RequestToSv2ServerError::IdNotFound);
        }

        let setup_connection_success_flags = match req.protocol {
            Protocol::MiningProtocol => {
                self.mining_handler.add_client(client_id, req.flags).await;
                self.mining_handler.setup_connection_success_flags()
            }
            Protocol::JobDeclarationProtocol => {
                // todo
                0
            }
            Protocol::TemplateDistributionProtocol => {
                // todo
                0
            }
        };

        // 5) Return SetupConnectionSuccess
        let setup_connection_success = SetupConnectionSuccess {
            used_version,
            flags: setup_connection_success_flags,
        };

        let response = ResponseFromSv2Server::TriggerNewRequest(Box::new(
            RequestToSv2Server::SendMessagesToClient(Box::new(Sv2MessagesToClient {
                client_id,
                messages: vec![setup_connection_success.into()],
            })),
        ));

        Ok(response)
    }

    /// Add a client to the service (for testing purposes)
    #[cfg(test)]
    pub fn add_client(&mut self, client_id: u32, client: Sv2ServerServiceClient) {
        self.clients.insert(client_id, Arc::new(client));
    }
}

impl<M> Service<RequestToSv2Server<'static>> for Sv2ServerService<M>
where
    M: Sv2MiningServerHandler + Clone + Send + Sync + 'static,
{
    type Response = ResponseFromSv2Server<'static>;
    type Error = RequestToSv2ServerError;
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

        // let template_distribution_poll_ready = match Self::has_null_handler(Protocol::TemplateDistributionProtocol) {
        //     true => Poll::Ready(Ok(())),
        //     false => self.template_distribution_handler.poll_ready(cx),
        // };
        let template_distribution_poll_ready = Poll::Ready(Ok(()));

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

    fn call(&mut self, req: RequestToSv2Server<'static>) -> Self::Future {
        // https://docs.rs/tower/latest/tower/trait.Service.html#be-careful-when-cloning-inner-services
        let clone = self.clone();
        let mut this = std::mem::replace(self, clone);

        Box::pin(async move {
            // Extract client_id if available and update message time
            if let RequestToSv2Server::IncomingMessage(sv2_message) = &req {
                if let Some(client_id) = sv2_message.client_id {
                    this.update_client_message_time(client_id);
                }
            }

            let req_clone = req.clone();
            let response = match req_clone {
                RequestToSv2Server::IncomingMessage(sv2_message) => {
                    match sv2_message.message {
                        AnyMessage::Common(common) => {
                            match common {
                                CommonMessages::SetupConnection(setup_connection) => {
                                    // SetupConnection must be the first message from the client
                                    // therefore client_id must be Some since it was assigned when
                                    // the client connected to the TCP server
                                    if let Some(client_id) = sv2_message.client_id {
                                        this.handle_setup_connection(setup_connection, client_id)
                                            .await
                                    } else {
                                        Err(RequestToSv2ServerError::IdMustBeSome)
                                    }
                                }
                                _ => Err(RequestToSv2ServerError::UnsupportedMessage),
                            }
                        }
                        // Mining protocol messages
                        AnyMessage::Mining(message) => {
                            // Check if mining protocol is supported before routing to mining handler
                            if Self::has_null_handler(Protocol::MiningProtocol) {
                                return Err(RequestToSv2ServerError::UnsupportedProtocol {
                                    protocol: Protocol::MiningProtocol,
                                });
                            }

                            match message {
                                Mining::OpenStandardMiningChannel(open_standard_mining_channel) => {
                                    debug!("Sv2ServerService received a OpenStandardMiningChannel message");
                                    this.mining_handler
                                        .handle_open_standard_mining_channel(
                                            sv2_message.client_id.expect("client_id must be Some"),
                                            open_standard_mining_channel,
                                        )
                                        .await
                                }
                                Mining::OpenExtendedMiningChannel(open_extended_mining_channel) => {
                                    debug!("Sv2ServerService received a OpenExtendedMiningChannel message");
                                    this.mining_handler
                                        .handle_open_extended_mining_channel(
                                            sv2_message.client_id.expect("client_id must be Some"),
                                            open_extended_mining_channel,
                                        )
                                        .await
                                }
                                Mining::UpdateChannel(update_channel) => {
                                    debug!("Sv2ServerService received a UpdateChannel message");
                                    this.mining_handler
                                        .handle_update_channel(
                                            sv2_message.client_id.expect("client_id must be Some"),
                                            update_channel,
                                        )
                                        .await
                                }
                                Mining::SubmitSharesStandard(submit_shares_standard) => {
                                    debug!(
                                        "Sv2ServerService received a SubmitSharesStandard message"
                                    );
                                    this.mining_handler
                                        .handle_submit_shares_standard(
                                            sv2_message.client_id.expect("client_id must be Some"),
                                            submit_shares_standard,
                                        )
                                        .await
                                }
                                Mining::SubmitSharesExtended(submit_shares_extended) => {
                                    debug!(
                                        "Sv2ServerService received a SubmitSharesExtended message"
                                    );
                                    this.mining_handler
                                        .handle_submit_shares_extended(
                                            sv2_message.client_id.expect("client_id must be Some"),
                                            submit_shares_extended,
                                        )
                                        .await
                                }
                                Mining::SetCustomMiningJob(set_custom_mining_job) => {
                                    debug!(
                                        "Sv2ServerService received a SetCustomMiningJob message"
                                    );
                                    this.mining_handler
                                        .handle_set_custom_mining_job(
                                            sv2_message.client_id.expect("client_id must be Some"),
                                            set_custom_mining_job,
                                        )
                                        .await
                                }
                                Mining::CloseChannel(close_channel) => {
                                    debug!("Sv2ServerService received a CloseChannel message");
                                    this.mining_handler
                                        .handle_close_channel(
                                            sv2_message.client_id.expect("client_id must be Some"),
                                            close_channel,
                                        )
                                        .await
                                }
                                Mining::NewExtendedMiningJob(_) => {
                                    error!(
                                        "Sv2ServerService received a NewExtendedMiningJob message"
                                    );
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::NewMiningJob(_) => {
                                    error!("Sv2ServerService received a NewMiningJob message");
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::SetNewPrevHash(_) => {
                                    error!("Sv2ServerService received a SetNewPrevHash message");
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::OpenExtendedMiningChannelSuccess(_) => {
                                    error!("Sv2ServerService received a OpenExtendedMiningChannelSuccess message");
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::OpenMiningChannelError(_) => {
                                    error!("Sv2ServerService received a OpenMiningChannelError message");
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::OpenStandardMiningChannelSuccess(_) => {
                                    error!("Sv2ServerService received a OpenStandardMiningChannelSuccess message");
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::SetCustomMiningJobError(_) => {
                                    error!("Sv2ServerService received a SetCustomMiningJobError message");
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::SetCustomMiningJobSuccess(_) => {
                                    error!("Sv2ServerService received a SetCustomMiningJobSuccess message");
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::SetExtranoncePrefix(_) => {
                                    error!(
                                        "Sv2ServerService received a SetExtranoncePrefix message"
                                    );
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::SetGroupChannel(_) => {
                                    error!("Sv2ServerService received a SetGroupChannel message");
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::SetTarget(_) => {
                                    error!("Sv2ServerService received a SetTarget message");
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::SubmitSharesError(_) => {
                                    error!("Sv2ServerService received a SubmitSharesError message");
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::SubmitSharesSuccess(_) => {
                                    error!(
                                        "Sv2ServerService received a SubmitSharesSuccess message"
                                    );
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                                Mining::UpdateChannelError(_) => {
                                    error!(
                                        "Sv2ServerService received a UpdateChannelError message"
                                    );
                                    Err(RequestToSv2ServerError::UnsupportedMessage)
                                }
                            }
                        }
                        // JobDeclaration
                        // TemplateDistribution
                        _ => {
                            todo!()
                        }
                    }
                }
                RequestToSv2Server::MiningTrigger(req) => match req {
                    MiningServerTrigger::Start => {
                        debug!("Sv2ServerService received a MiningServerTrigger::Start request");
                        this.mining_handler.start().await
                    }
                    MiningServerTrigger::NewTemplate(new_template) => {
                        debug!(
                            "Sv2ServerService received a MiningServerTrigger::NewTemplate request"
                        );
                        this.mining_handler.on_new_template(new_template).await
                    }
                    MiningServerTrigger::SetNewPrevHash(set_new_prev_hash) => {
                        debug!("Sv2ServerService received a MiningServerTrigger::SetNewPrevHash request");
                        this.mining_handler
                            .on_set_new_prev_hash(set_new_prev_hash)
                            .await
                    }
                },
                RequestToSv2Server::SendRequestToSiblingClientService(req) => {
                    debug!(
                        "Sv2ServerService received a SendExternalRequestToClientService request"
                    );
                    match this.sibling_client_service_io {
                        Some(ref io) => {
                            io.send(*req.clone()).map_err(|_| {
                                RequestToSv2ServerError::FailedToSendRequestToSiblingClientService
                            })?;
                            Ok(ResponseFromSv2Server::Ok)
                        }
                        None => {
                            error!("No sibling client service on Sv2ServerService");
                            Err(RequestToSv2ServerError::NoSiblingClientService)
                        }
                    }
                }
                RequestToSv2Server::SendMessagesToClient(sv2_messages_to_client) => {
                    debug!("Sv2ServerService received a SendMessagesToClient request");

                    let client_id = sv2_messages_to_client.client_id;

                    // Get the client's IO from the map
                    let io = if let Some(client) = this.clients.get(&client_id) {
                        client.io.clone()
                    } else {
                        error!("Client not found in Sv2ServerService");
                        return Err(RequestToSv2ServerError::FailedToSendResponseToClient);
                    };

                    let messages = sv2_messages_to_client.messages;

                    for message in messages {
                        match io.send_message(message).await {
                            Ok(_) => {
                                continue;
                            }
                            Err(_) => {
                                return Err(RequestToSv2ServerError::FailedToSendResponseToClient)
                            }
                        }
                    }

                    return Ok(ResponseFromSv2Server::Ok);
                }
                RequestToSv2Server::SendMessagesToClients(sv2_messages_to_clients) => {
                    debug!("Sv2ServerService received a SendMessagesToClients request");

                    // iterate over each client and send the messages to them
                    for sv2_messages_to_client in sv2_messages_to_clients.as_ref() {
                        let client_id = sv2_messages_to_client.client_id;

                        // Get the client's IO from the map
                        let io = if let Some(client) = this.clients.get(&client_id) {
                            client.io.clone()
                        } else {
                            error!("Client not found in Sv2ServerService");
                            return Err(RequestToSv2ServerError::FailedToSendResponseToClient);
                        };

                        for message in sv2_messages_to_client.messages.clone() {
                            match io.send_message(message).await {
                                Ok(_) => {
                                    continue;
                                }
                                Err(_) => {
                                    return Err(
                                        RequestToSv2ServerError::FailedToSendResponseToClient,
                                    )
                                }
                            }
                        }
                    }

                    return Ok(ResponseFromSv2Server::Ok);
                }
                RequestToSv2Server::MultipleRequests(reqs) => {
                    debug!(
                        "Sv2ServerService received a MultipleRequests request: {:?}",
                        reqs
                    );

                    for req in reqs.as_ref() {
                        if let Err(e) = this.call(req.clone()).await {
                            error!(
                                "RequestToSv2Server::MultipleRequests {:?} generated an error {:?}",
                                req, e
                            );
                            return Err(e);
                        }
                    }
                    Ok(ResponseFromSv2Server::Ok)
                }
            };

            // allow for recursive chaining of requests
            if let Ok(ResponseFromSv2Server::TriggerNewRequest(req)) = response {
                this.call(*req).await
            } else {
                response
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::client::tcp::encrypted::Sv2EncryptedTcpClient;
    use crate::server::service::config::Sv2ServerServiceJobDeclarationConfig;
    use crate::server::service::config::Sv2ServerServiceMiningConfig;
    use crate::server::service::config::Sv2ServerTcpConfig;
    use crate::server::service::Sv2ServerService;
    use crate::server::service::{
        error::Sv2ServerServiceError, subprotocols::mining::handler::NullSv2MiningServerHandler,
        Sv2ServerServiceConfig,
    };
    use crate::Sv2MessageFrame;
    use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
    use stratum_common::roles_logic_sv2;
    use stratum_common::roles_logic_sv2::common_messages_sv2::{Protocol, SetupConnection};
    use stratum_common::roles_logic_sv2::parsers::{AnyMessage, CommonMessages};
    use tokio_util::sync::CancellationToken;

    fn get_available_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    #[tokio::test]
    async fn sv2_server_ok() {
        let server_port = get_available_port();
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), server_port);
        let pub_key = Secp256k1PublicKey::try_from(
            "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
        )
        .expect("failed");
        let priv_key = Secp256k1SecretKey::try_from(
            "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
        )
        .expect("failed");

        let tcp_config = Sv2ServerTcpConfig {
            listen_address: server_addr,
            pub_key,
            priv_key,
            cert_validity: 3600,
        };

        let job_declaration_config = Sv2ServerServiceJobDeclarationConfig {
            supported_flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
        };

        let sv2_server_config = Sv2ServerServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            inactivity_limit: 1,
            tcp_config,
            mining_config: None,
            job_declaration_config: Some(job_declaration_config),
            template_distribution_config: None,
        };

        let mining_handler = NullSv2MiningServerHandler;

        let cancellation_token = CancellationToken::new();

        let sv2_server_service =
            Sv2ServerService::new(sv2_server_config, mining_handler, cancellation_token).unwrap();

        // Spawn the server start in a background task
        let mut sv2_server_service_clone = sv2_server_service.clone();
        tokio::spawn(async move {
            sv2_server_service_clone.start().await.unwrap();
        });

        // Wait for server to be ready
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Create a TCP client to establish a connection
        let client = Sv2EncryptedTcpClient::new(server_addr, None).await.unwrap();

        let setup_connection_ok = SetupConnection {
            protocol: Protocol::JobDeclarationProtocol,
            min_version: 2,
            max_version: 2,
            flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
            endpoint_host: "".to_string().try_into().unwrap(),
            endpoint_port: 0,
            vendor: "".to_string().try_into().unwrap(),
            hardware_version: "".to_string().try_into().unwrap(),
            firmware: "".to_string().try_into().unwrap(),
            device_id: "".to_string().try_into().unwrap(),
        };

        // Send SetupConnection message through the client
        client
            .io
            .send_message(setup_connection_ok.clone().into())
            .await
            .unwrap();

        // Wait for and verify server's response
        let response = client.io.rx.recv().await.unwrap();
        match response {
            Sv2MessageFrame::Sv2(mut frame) => {
                let header = frame.get_header().unwrap();
                assert_eq!(
                    header.msg_type(),
                    roles_logic_sv2::common_messages_sv2::MESSAGE_TYPE_SETUP_CONNECTION_SUCCESS
                );
                let mut payload = frame.payload().to_vec();
                let message: Result<AnyMessage<'_>, _> =
                    (header.msg_type(), payload.as_mut_slice()).try_into();
                if let Ok(AnyMessage::Common(CommonMessages::SetupConnectionSuccess(success))) =
                    message
                {
                    assert_eq!(success.used_version, 2);
                    assert_eq!(success.flags, 0);
                } else {
                    panic!("expected SetupConnectionSuccess message");
                }
            }
            _ => panic!("expected Sv2Frame"),
        }

        // Verify that the client was added to the service
        assert_eq!(sv2_server_service.get_client_count(), 1);

        // Verify that the client's connection details were set correctly
        let client = sv2_server_service.get_client(1).unwrap();
        let connection = client.connection.read().await.clone().unwrap();
        assert_eq!(connection.protocol, setup_connection_ok.protocol);
        assert_eq!(connection.min_version, setup_connection_ok.min_version);
        assert_eq!(connection.max_version, setup_connection_ok.max_version);
        assert_eq!(connection.flags, setup_connection_ok.flags);
        assert_eq!(connection.endpoint_host, setup_connection_ok.endpoint_host);
        assert_eq!(connection.endpoint_port, setup_connection_ok.endpoint_port);
        assert_eq!(connection.vendor, setup_connection_ok.vendor);
        assert_eq!(
            connection.hardware_version,
            setup_connection_ok.hardware_version
        );
        assert_eq!(connection.firmware, setup_connection_ok.firmware);
        assert_eq!(connection.device_id, setup_connection_ok.device_id);

        // sleep to trigger removal of connection due to inactivity (limit is 1s)
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        assert_eq!(sv2_server_service.get_client_count(), 0);
    }

    #[tokio::test]
    async fn sv2_server_ok_with_multiple_clients() {
        let server_port = get_available_port();
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), server_port);

        let pub_key = Secp256k1PublicKey::try_from(
            "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
        )
        .expect("failed");
        let priv_key = Secp256k1SecretKey::try_from(
            "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
        )
        .expect("failed");

        let tcp_config = Sv2ServerTcpConfig {
            listen_address: server_addr,
            pub_key,
            priv_key,
            cert_validity: 3600,
        };

        let job_declaration_config = Sv2ServerServiceJobDeclarationConfig {
            supported_flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
        };

        let sv2_server_config = Sv2ServerServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            inactivity_limit: 1,
            tcp_config,
            job_declaration_config: Some(job_declaration_config),
            mining_config: None,
            template_distribution_config: None,
        };

        let mining_handler = NullSv2MiningServerHandler;

        let cancellation_token = CancellationToken::new();

        let sv2_server_service =
            Sv2ServerService::new(sv2_server_config, mining_handler, cancellation_token).unwrap();

        // Spawn the server start in a background task
        let mut sv2_server_service_clone = sv2_server_service.clone();
        tokio::spawn(async move {
            sv2_server_service_clone.start().await.unwrap();
        });

        // Wait for server to be ready
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Create a TCP client to establish a connection
        let client_1 = Sv2EncryptedTcpClient::new(server_addr, None).await.unwrap();
        let client_2 = Sv2EncryptedTcpClient::new(server_addr, None).await.unwrap();

        let setup_connection_ok = SetupConnection {
            protocol: Protocol::JobDeclarationProtocol,
            min_version: 2,
            max_version: 2,
            flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
            endpoint_host: "".to_string().try_into().unwrap(),
            endpoint_port: 0,
            vendor: "".to_string().try_into().unwrap(),
            hardware_version: "".to_string().try_into().unwrap(),
            firmware: "".to_string().try_into().unwrap(),
            device_id: "".to_string().try_into().unwrap(),
        };

        // Send SetupConnection message through the first client
        client_1
            .io
            .send_message(setup_connection_ok.clone().into())
            .await
            .unwrap();

        // Send SetupConnection message through the second client
        client_2
            .io
            .send_message(setup_connection_ok.clone().into())
            .await
            .unwrap();

        // Receive the responses
        let _response_1 = client_1.io.rx.recv().await.unwrap();
        let _response_2 = client_2.io.rx.recv().await.unwrap();

        // Verify that the clients were added to the service
        assert_eq!(sv2_server_service.get_client_count(), 2);
    }

    #[tokio::test]
    async fn sv2_server_service_bad_protocol() {
        let server_port = {
            use std::net::TcpListener;
            TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port()
        };
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), server_port);
        let pub_key = Secp256k1PublicKey::try_from(
            "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
        )
        .expect("failed");
        let priv_key = Secp256k1SecretKey::try_from(
            "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
        )
        .expect("failed");

        let tcp_config = Sv2ServerTcpConfig {
            listen_address: server_addr,
            pub_key,
            priv_key,
            cert_validity: 3600,
        };

        let job_declaration_config = Sv2ServerServiceJobDeclarationConfig {
            supported_flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
        };

        let sv2_server_config = Sv2ServerServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            inactivity_limit: 1,
            tcp_config,
            job_declaration_config: Some(job_declaration_config),
            mining_config: None,
            template_distribution_config: None,
        };

        let mining_handler = NullSv2MiningServerHandler;

        let cancellation_token = CancellationToken::new();

        let sv2_server_service =
            Sv2ServerService::new(sv2_server_config, mining_handler, cancellation_token).unwrap();

        // Spawn the server start in a background task
        let mut sv2_server_service_clone = sv2_server_service.clone();
        tokio::spawn(async move {
            sv2_server_service_clone.start().await.unwrap();
        });

        // Wait for server to be ready
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Create a TCP client to establish a connection
        let client = Sv2EncryptedTcpClient::new(server_addr, None).await.unwrap();

        let setup_connection_bad_protocol = SetupConnection {
            protocol: Protocol::TemplateDistributionProtocol, // unsupported protocol
            min_version: 2,
            max_version: 2,
            flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
            endpoint_host: "".to_string().try_into().unwrap(),
            endpoint_port: 0,
            vendor: "".to_string().try_into().unwrap(),
            hardware_version: "".to_string().try_into().unwrap(),
            firmware: "".to_string().try_into().unwrap(),
            device_id: "".to_string().try_into().unwrap(),
        };

        // Send SetupConnection message through the client
        client
            .io
            .send_message(setup_connection_bad_protocol.clone().into())
            .await
            .unwrap();

        // Receive the response
        let response = client.io.rx.recv().await.unwrap();
        match response {
            Sv2MessageFrame::Sv2(mut frame) => {
                let header = frame.get_header().unwrap();
                assert_eq!(
                    header.msg_type(),
                    roles_logic_sv2::common_messages_sv2::MESSAGE_TYPE_SETUP_CONNECTION_ERROR
                );
                let mut payload = frame.payload().to_vec();
                let message: Result<AnyMessage<'_>, _> =
                    (header.msg_type(), payload.as_mut_slice()).try_into();
                if let Ok(AnyMessage::Common(CommonMessages::SetupConnectionError(error))) = message
                {
                    assert_eq!(error.error_code.as_ref(), b"unsupported-protocol");
                } else {
                    panic!("expected SetupConnectionError message");
                }
            }
            _ => panic!("expected Sv2Frame"),
        }

        // wait for the client to be removed from the service
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // verify that the client was removed from the service
        assert_eq!(sv2_server_service.get_client_count(), 0);
    }

    #[tokio::test]
    async fn sv2_server_service_version_mismatch() {
        let server_port = {
            use std::net::TcpListener;
            TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port()
        };
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), server_port);
        let pub_key = Secp256k1PublicKey::try_from(
            "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
        )
        .expect("failed");
        let priv_key = Secp256k1SecretKey::try_from(
            "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
        )
        .expect("failed");

        let tcp_config = Sv2ServerTcpConfig {
            listen_address: server_addr,
            pub_key,
            priv_key,
            cert_validity: 3600,
        };

        let job_declaration_config = Sv2ServerServiceJobDeclarationConfig {
            supported_flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
        };

        let sv2_server_config = Sv2ServerServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            inactivity_limit: 1,
            tcp_config,
            job_declaration_config: Some(job_declaration_config),
            mining_config: None,
            template_distribution_config: None,
        };

        let mining_handler = NullSv2MiningServerHandler;

        let cancellation_token = CancellationToken::new();

        let sv2_server_service =
            Sv2ServerService::new(sv2_server_config, mining_handler, cancellation_token).unwrap();

        // Spawn the server start in a background task
        let mut sv2_server_service_clone = sv2_server_service.clone();
        tokio::spawn(async move {
            sv2_server_service_clone.start().await.unwrap();
        });

        // Wait for server to be ready
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Test min version too high
        let client = Sv2EncryptedTcpClient::new(server_addr, None).await.unwrap();

        let setup_connection_bad_min_version = SetupConnection {
            protocol: Protocol::JobDeclarationProtocol,
            min_version: 3,
            max_version: 3,
            flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
            endpoint_host: "".to_string().try_into().unwrap(),
            endpoint_port: 0,
            vendor: "".to_string().try_into().unwrap(),
            hardware_version: "".to_string().try_into().unwrap(),
            firmware: "".to_string().try_into().unwrap(),
            device_id: "".to_string().try_into().unwrap(),
        };

        // Send SetupConnection message through the client
        client
            .io
            .send_message(setup_connection_bad_min_version.clone().into())
            .await
            .unwrap();

        // Receive the response
        let response = client.io.rx.recv().await.unwrap();
        match response {
            Sv2MessageFrame::Sv2(mut frame) => {
                let header = frame.get_header().unwrap();
                assert_eq!(
                    header.msg_type(),
                    roles_logic_sv2::common_messages_sv2::MESSAGE_TYPE_SETUP_CONNECTION_ERROR
                );
                let mut payload = frame.payload().to_vec();
                let message: Result<AnyMessage<'_>, _> =
                    (header.msg_type(), payload.as_mut_slice()).try_into();
                if let Ok(AnyMessage::Common(CommonMessages::SetupConnectionError(error))) = message
                {
                    assert_eq!(error.error_code.as_ref(), b"protocol-version-mismatch");
                } else {
                    panic!("expected SetupConnectionError message");
                }
            }
            _ => panic!("expected Sv2Frame"),
        }

        // Test max version too low
        let client = Sv2EncryptedTcpClient::new(server_addr, None).await.unwrap();

        let setup_connection_bad_max_version = SetupConnection {
            protocol: Protocol::JobDeclarationProtocol,
            min_version: 1,
            max_version: 1,
            flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
            endpoint_host: "".to_string().try_into().unwrap(),
            endpoint_port: 0,
            vendor: "".to_string().try_into().unwrap(),
            hardware_version: "".to_string().try_into().unwrap(),
            firmware: "".to_string().try_into().unwrap(),
            device_id: "".to_string().try_into().unwrap(),
        };

        // Send SetupConnection message through the client
        client
            .io
            .send_message(setup_connection_bad_max_version.clone().into())
            .await
            .unwrap();

        // Receive the response
        let response = client.io.rx.recv().await.unwrap();
        match response {
            Sv2MessageFrame::Sv2(mut frame) => {
                let header = frame.get_header().unwrap();
                assert_eq!(
                    header.msg_type(),
                    roles_logic_sv2::common_messages_sv2::MESSAGE_TYPE_SETUP_CONNECTION_ERROR
                );
                let mut payload = frame.payload().to_vec();
                let message: Result<AnyMessage<'_>, _> =
                    (header.msg_type(), payload.as_mut_slice()).try_into();
                if let Ok(AnyMessage::Common(CommonMessages::SetupConnectionError(error))) = message
                {
                    assert_eq!(error.error_code.as_ref(), b"protocol-version-mismatch");
                } else {
                    panic!("expected SetupConnectionError message");
                }
            }
            _ => panic!("expected Sv2Frame"),
        }

        // wait for the client to be removed from the service
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // verify that the client was removed from the service
        assert_eq!(sv2_server_service.get_client_count(), 0);
    }

    #[tokio::test]
    async fn sv2_server_service_unsupported_flags() {
        let server_port = {
            use std::net::TcpListener;
            TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port()
        };
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), server_port);
        let pub_key = Secp256k1PublicKey::try_from(
            "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
        )
        .expect("failed");
        let priv_key = Secp256k1SecretKey::try_from(
            "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
        )
        .expect("failed");

        let tcp_config = Sv2ServerTcpConfig {
            listen_address: server_addr,
            pub_key,
            priv_key,
            cert_validity: 3600,
        };

        let job_declaration_config = Sv2ServerServiceJobDeclarationConfig {
            supported_flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
        };

        let sv2_server_config = Sv2ServerServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            inactivity_limit: 1,
            tcp_config,
            job_declaration_config: Some(job_declaration_config),
            mining_config: None,
            template_distribution_config: None,
        };

        let mining_handler = NullSv2MiningServerHandler;

        let cancellation_token = CancellationToken::new();

        let sv2_server_service =
            Sv2ServerService::new(sv2_server_config, mining_handler, cancellation_token).unwrap();

        // Spawn the server start in a background task
        let mut sv2_server_service_clone = sv2_server_service.clone();
        tokio::spawn(async move {
            sv2_server_service_clone.start().await.unwrap();
        });

        // Wait for server to be ready
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Create a TCP client to establish a connection
        let client = Sv2EncryptedTcpClient::new(server_addr, None).await.unwrap();

        let setup_connection_unsupported_flags = SetupConnection {
            protocol: Protocol::JobDeclarationProtocol,
            min_version: 2,
            max_version: 2,
            flags: 0b0000_0000_0000_0000_0000_0000_0000_0001,
            endpoint_host: "".to_string().try_into().unwrap(),
            endpoint_port: 0,
            vendor: "".to_string().try_into().unwrap(),
            hardware_version: "".to_string().try_into().unwrap(),
            firmware: "".to_string().try_into().unwrap(),
            device_id: "".to_string().try_into().unwrap(),
        };

        // Send SetupConnection message through the client
        client
            .io
            .send_message(setup_connection_unsupported_flags.clone().into())
            .await
            .unwrap();

        // Receive the response
        let response = client.io.rx.recv().await.unwrap();
        match response {
            Sv2MessageFrame::Sv2(mut frame) => {
                let header = frame.get_header().unwrap();
                assert_eq!(
                    header.msg_type(),
                    roles_logic_sv2::common_messages_sv2::MESSAGE_TYPE_SETUP_CONNECTION_ERROR
                );
                let mut payload = frame.payload().to_vec();
                let message: Result<AnyMessage<'_>, _> =
                    (header.msg_type(), payload.as_mut_slice()).try_into();
                if let Ok(AnyMessage::Common(CommonMessages::SetupConnectionError(error))) = message
                {
                    assert_eq!(error.error_code.as_ref(), b"unsupported-feature-flags");
                } else {
                    panic!("expected SetupConnectionError message");
                }
            }
            _ => panic!("expected Sv2Frame"),
        }

        // wait for the client to be removed from the service
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // verify that the client was removed from the service
        assert_eq!(sv2_server_service.get_client_count(), 0);
    }

    #[test]
    fn sv2_server_service_null_handler_error() {
        let tcp_config = Sv2ServerTcpConfig {
            listen_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            pub_key: Secp256k1PublicKey::try_from(
                "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
            )
            .expect("failed"),
            priv_key: Secp256k1SecretKey::try_from(
                "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
            )
            .expect("failed"),
            cert_validity: 3600,
        };

        let mining_config = Sv2ServerServiceMiningConfig {
            supported_flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
        };

        let sv2_server_config = Sv2ServerServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            inactivity_limit: 1,
            tcp_config,
            mining_config: Some(mining_config),
            job_declaration_config: None,
            template_distribution_config: None,
        };

        // Create a null mining handler
        let mining_handler = NullSv2MiningServerHandler {};

        let cancellation_token = CancellationToken::new();

        // This should return an error because we're using a null handler for a supported protocol
        let result =
            super::Sv2ServerService::new(sv2_server_config, mining_handler, cancellation_token);

        assert!(result.is_err());

        if let Err(err) = result {
            match err {
                Sv2ServerServiceError::NullHandlerForSupportedProtocol { protocol } => {
                    assert_eq!(protocol, Protocol::MiningProtocol);
                }
                _ => panic!("Expected NullHandlerForSupportedProtocol error"),
            }
        } else {
            panic!("Expected error for null handler for supported protocol");
        }
    }

    #[tokio::test]
    async fn sv2_server_shutdown_with_no_clients() {
        let server_port = get_available_port();
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), server_port);
        let pub_key = Secp256k1PublicKey::try_from(
            "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
        )
        .expect("failed");
        let priv_key = Secp256k1SecretKey::try_from(
            "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
        )
        .expect("failed");

        let tcp_config = Sv2ServerTcpConfig {
            listen_address: server_addr,
            pub_key,
            priv_key,
            cert_validity: 3600,
        };

        let job_declaration_config = Sv2ServerServiceJobDeclarationConfig {
            supported_flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
        };

        let sv2_server_config = Sv2ServerServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            job_declaration_config: Some(job_declaration_config),
            mining_config: None,
            template_distribution_config: None,
            inactivity_limit: 1,
            tcp_config,
        };

        let mining_handler = NullSv2MiningServerHandler;

        let cancellation_token = CancellationToken::new();

        let sv2_server_service = Sv2ServerService::new(
            sv2_server_config,
            mining_handler,
            cancellation_token.clone(),
        )
        .unwrap();

        // Spawn the server start in a background task
        let mut sv2_server_service_clone = sv2_server_service.clone();
        tokio::spawn(async move {
            sv2_server_service_clone.start().await.unwrap();
        });

        // Wait for server to be ready
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify initial state
        assert_eq!(sv2_server_service.get_client_count(), 0);

        // Shutdown the server
        cancellation_token.cancel();

        // Verify final state
        assert_eq!(sv2_server_service.get_client_count(), 0);
    }

    #[tokio::test]
    async fn sv2_server_shutdown_with_active_clients() {
        let server_port = get_available_port();
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), server_port);
        let pub_key = Secp256k1PublicKey::try_from(
            "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72".to_string(),
        )
        .expect("failed");
        let priv_key = Secp256k1SecretKey::try_from(
            "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n".to_string(),
        )
        .expect("failed");

        let tcp_config = Sv2ServerTcpConfig {
            listen_address: server_addr,
            pub_key,
            priv_key,
            cert_validity: 3600,
        };

        let job_declaration_config = Sv2ServerServiceJobDeclarationConfig {
            supported_flags: 0b0000_0000_0000_0000_0000_0000_0000_0000,
        };

        let sv2_server_config = Sv2ServerServiceConfig {
            min_supported_version: 2,
            max_supported_version: 2,
            job_declaration_config: Some(job_declaration_config),
            mining_config: None,
            template_distribution_config: None,
            inactivity_limit: 10, // Set higher to prevent automatic cleanup
            tcp_config,
        };

        let mining_handler = NullSv2MiningServerHandler;

        let cancellation_token = CancellationToken::new();

        let sv2_server_service = Sv2ServerService::new(
            sv2_server_config,
            mining_handler,
            cancellation_token.clone(),
        )
        .unwrap();

        // Spawn the server start in a background task
        let mut sv2_server_service_clone = sv2_server_service.clone();
        tokio::spawn(async move {
            sv2_server_service_clone.start().await.unwrap();
        });

        // Wait for server to be ready
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Create and connect multiple clients
        let client1 = Sv2EncryptedTcpClient::new(server_addr, None).await.unwrap();
        let client2 = Sv2EncryptedTcpClient::new(server_addr, None).await.unwrap();

        let setup_connection = SetupConnection {
            protocol: Protocol::JobDeclarationProtocol,
            min_version: 2,
            max_version: 2,
            flags: 0,
            endpoint_host: "".to_string().try_into().unwrap(),
            endpoint_port: 0,
            vendor: "".to_string().try_into().unwrap(),
            hardware_version: "".to_string().try_into().unwrap(),
            firmware: "".to_string().try_into().unwrap(),
            device_id: "".to_string().try_into().unwrap(),
        };

        // Send SetupConnection messages
        client1
            .io
            .send_message(setup_connection.clone().into())
            .await
            .unwrap();
        client2
            .io
            .send_message(setup_connection.clone().into())
            .await
            .unwrap();

        // Verify both clients are connected
        let client_count = sv2_server_service.get_client_count();
        assert_eq!(client_count, 2);

        cancellation_token.cancel();

        // Try to receive messages from clients - should fail as connections are closed
        assert!(client1.io.recv_message().await.is_err());
        assert!(client2.io.recv_message().await.is_err());
    }
}
