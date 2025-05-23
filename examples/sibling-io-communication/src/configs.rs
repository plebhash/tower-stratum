use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
};

use tower_stratum::{
    client::service::config::{Sv2ClientServiceConfig, Sv2ClientServiceTemplateDistributionConfig},
    key_utils::{Secp256k1PublicKey, Secp256k1SecretKey},
    server::service::config::{
        Sv2ServerServiceConfig, Sv2ServerServiceMiningConfig, Sv2ServerTcpConfig,
    },
};

pub struct MyConfig {
    pub server_config: Sv2ServerServiceConfig,
    pub client_config: Sv2ClientServiceConfig,
}

impl MyConfig {
    pub fn new() -> Self {
        let server_config = Sv2ServerServiceConfig {
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
                encrypted: true
            },
            mining_config: Some(Sv2ServerServiceMiningConfig {
                supported_flags: 0b0101,
            }),
            job_declaration_config: None,
            template_distribution_config: None,
        };

        let client_config = Sv2ClientServiceConfig {
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
            }),
            encrypted: true,
        };
        Self {
            server_config,
            client_config,
        }
    }
}
