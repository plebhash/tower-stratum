use key_utils::Secp256k1PublicKey;
use key_utils::Secp256k1SecretKey;
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Clone, Deserialize)]
pub struct MyMiningServerConfig {
    pub listening_port: u16,
    pub pub_key: Secp256k1PublicKey,
    pub priv_key: Secp256k1SecretKey,
    pub cert_validity: u64,
    pub inactivity_limit: u64,
}

impl MyMiningServerConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
    }
}
