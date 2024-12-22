use roles_logic_sv2::common_messages_sv2::Protocol;
use std::fmt;

/// Errors that can occur when working with the Sv2ServerService.
#[derive(Debug)]
pub enum Sv2ServerServiceError {
    /// Occurs when a protocol is configured as supported but the corresponding handler is null.
    NullHandlerForSupportedProtocol {
        /// The protocol that was configured as supported but has a null handler.
        protocol: Protocol,
    },
    /// Occurs when a protocol is not configured as supported but a non-null handler is provided.
    NonNullHandlerForUnsupportedProtocol {
        /// The protocol that was not configured as supported but has a non-null handler.
        protocol: Protocol,
    },
    /// Occurs when a protocol is configured as supported but no config is provided.
    MissingConfigForSupportedProtocol {
        /// The protocol that was configured as supported but has no config.
        protocol: Protocol,
    },
    /// Occurs when the TCP server fails to start.
    TcpServerError,
    /// Other errors that might occur in the future.
    Other(String),
}

impl fmt::Display for Sv2ServerServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Sv2ServerServiceError::NullHandlerForSupportedProtocol { protocol } => {
                write!(
                    f,
                    "Protocol {:?} is supported but a null handler was provided",
                    protocol
                )
            }
            Sv2ServerServiceError::NonNullHandlerForUnsupportedProtocol { protocol } => {
                write!(
                    f,
                    "Protocol {:?} is not supported but a non-null handler was provided",
                    protocol
                )
            }
            Sv2ServerServiceError::MissingConfigForSupportedProtocol { protocol } => {
                write!(
                    f,
                    "Protocol {:?} is supported but no config was provided",
                    protocol
                )
            }
            Sv2ServerServiceError::Other(msg) => write!(f, "{}", msg),
            Sv2ServerServiceError::TcpServerError => write!(f, "TCP server failed to start"),
        }
    }
}

impl std::error::Error for Sv2ServerServiceError {}
