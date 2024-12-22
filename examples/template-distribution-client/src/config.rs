use key_utils::Secp256k1PublicKey;
use serde::Deserialize;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;

#[derive(Deserialize)]
pub struct MyTemplateDistributionClientConfig {
    pub server_addr: SocketAddr,
    pub auth_pk: Option<Secp256k1PublicKey>,
    pub coinbase_output_max_additional_size: u32,
    pub coinbase_output_max_additional_sigops: u16,
}

impl MyTemplateDistributionClientConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
    }
}
