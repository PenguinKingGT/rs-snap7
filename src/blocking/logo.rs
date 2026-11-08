//! Blocking LOGO! address adapter using the async implementation.
use super::{Client, runtime};
use crate::{ClientConfig, Result};
use std::net::SocketAddr;

/// LOGO! VM access on the shared blocking runtime.
#[derive(Debug, Clone)]
pub struct Logo {
    client: Client,
}
impl Logo {
    /// Connect with explicit LOGO TSAPs.
    pub fn connect(endpoint: SocketAddr, local_tsap: u16, remote_tsap: u16) -> Result<Self> {
        let mut config = ClientConfig::new(endpoint, 0, 0)?;
        config.transport.local_tsap = local_tsap;
        config.transport.remote_tsap = remote_tsap.to_be_bytes().to_vec();
        Self::connect_with_config(config)
    }
    /// Connect with caller-provided configuration.
    pub fn connect_with_config(config: ClientConfig) -> Result<Self> {
        Ok(Self::from_client(Client::connect(config)?))
    }
    /// Share an existing blocking client session.
    pub fn from_client(client: Client) -> Self {
        Self { client }
    }
    /// Access the underlying client.
    pub fn client(&self) -> &Client {
        &self.client
    }
    /// Read a VM value using the checked asynchronous address adapter.
    pub fn read(&self, address: &str) -> Result<i64> {
        runtime::run(crate::Logo::from_client(self.client.inner.clone()).read(address))
    }
    /// Write a VM value using the checked asynchronous address adapter.
    pub fn write(&self, address: &str, value: i64) -> Result<()> {
        runtime::run(crate::Logo::from_client(self.client.inner.clone()).write(address, value))
    }
}
