use binary_sv2::Str0255;
use roles_logic_sv2::common_messages_sv2::Protocol;

/// Represents a successfully established Sv2 connection after `SetupConnection` Sv2 message is received and handled.
#[derive(Debug, Clone)]
pub struct Sv2ConnectionClient {
    pub protocol: Protocol,
    pub min_version: u16,
    pub max_version: u16,
    pub flags: u32,
    pub endpoint_host: Str0255<'static>,
    pub endpoint_port: u16,
    pub vendor: Str0255<'static>,
    pub hardware_version: Str0255<'static>,
    pub firmware: Str0255<'static>,
    pub device_id: Str0255<'static>,
}
