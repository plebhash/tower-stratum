use key_utils::{Secp256k1PublicKey, Secp256k1SecretKey};
use roles_logic_sv2::common_messages_sv2::Protocol;
use std::net::SocketAddr;

/// Config parameters for a [`crate::server::service::Sv2ServerService`]
/// Support for the Mining, Job Declaration and Template Distribution subprotocols is indicated by the presence of the corresponding config fields.
#[derive(Clone, Debug)]
pub struct Sv2ServerServiceConfig {
    /// The minimum protocol version this service is willing to speak.
    pub min_supported_version: u16,
    /// The maximum protocol version this service is willing to speak.
    pub max_supported_version: u16,
    /// Time limit that a connection is allowed to remain inactive before being dropped from memory (in seconds).
    pub inactivity_limit: u64,
    /// The configuration for the TCP server.
    pub tcp_config: Sv2ServerTcpConfig,
    pub mining_config: Option<Sv2ServerServiceMiningConfig>,
    pub job_declaration_config: Option<Sv2ServerServiceJobDeclarationConfig>,
    pub template_distribution_config: Option<Sv2ServerServiceTemplateDistributionConfig>,
}

impl Sv2ServerServiceConfig {
    /// Returns the list of supported protocols based on the presence of config fields.
    pub fn supported_protocols(&self) -> Vec<Protocol> {
        let mut protocols = Vec::new();

        if self.mining_config.is_some() {
            protocols.push(Protocol::MiningProtocol);
        }

        if self.job_declaration_config.is_some() {
            protocols.push(Protocol::JobDeclarationProtocol);
        }

        if self.template_distribution_config.is_some() {
            protocols.push(Protocol::TemplateDistributionProtocol);
        }

        protocols
    }
}

#[derive(Clone, Debug)]
pub struct Sv2ServerTcpConfig {
    /// The address that the server will listen on.
    pub listen_address: SocketAddr,
    /// The public key of the server.
    pub pub_key: Secp256k1PublicKey,
    /// The private key of the server.
    pub priv_key: Secp256k1SecretKey,
    /// The validity of the server's certificate.
    pub cert_validity: u64,
    /// TCP connection will be encrypted if this is set to true.
    pub encrypted: bool,
}

/// Config parameters for the Mining subprotocol of a [`crate::server::service::Sv2ServerService`]
#[derive(Clone, Debug)]
pub struct Sv2ServerServiceMiningConfig {
    /// Bitflags indicating the protocol features this service supports.
    pub supported_flags: u32,
}

/// Config parameters for the Job Declaration subprotocol of a [`crate::server::service::Sv2ServerService`]
#[derive(Clone, Debug)]
pub struct Sv2ServerServiceJobDeclarationConfig {
    /// Bitflags indicating the protocol features this service supports.
    pub supported_flags: u32,
}

/// Config parameters for the Template Distribution subprotocol of a [`crate::server::service::Sv2ServerService`]   
#[derive(Clone, Debug)]
pub struct Sv2ServerServiceTemplateDistributionConfig {
    /// Bitflags indicating the protocol features this service supports.
    pub supported_flags: u32,
}
