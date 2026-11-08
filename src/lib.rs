//! An idiomatic Rust implementation of classic Siemens S7 communication.
//!
//! See `docs/migration.md` for implemented capabilities and compatibility limits.

//! # Asynchronous access
//!
//! ```no_run
//! use rs_snap7::{Client, ClientConfig};
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() -> rs_snap7::Result<()> {
//! let endpoint = "192.168.1.10:102".parse().expect("valid PLC endpoint");
//! let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?).await?;
//! let _value = client.db_read_int(1, 0).await?;
//! client.disconnect().await?;
//! # Ok(())
//! # }
//! ```

pub mod blocks;
pub mod client;
pub mod codec;
mod config;
pub mod db;
pub mod error;
pub mod logo;
pub mod observability;
mod operations;
mod planner;
pub mod policy;
pub mod pool;
pub mod protocol;
pub mod szl;
pub mod tags;
pub mod transport;
pub mod types;

pub use client::Client;
pub use config::ClientConfig;
pub use error::{Error, Result};
pub use logo::Logo;
pub use types::{Address, Area, BlockType, CpuState, Parameter, ServerArea, WordLen};

pub use pool::ClientPool;

#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "partner")]
pub mod partner;

#[cfg(feature = "ppi")]
pub mod ppi;

#[cfg(feature = "discovery")]
pub mod discovery;

#[cfg(feature = "demo")]
pub mod demo;

#[cfg(feature = "blocking")]
pub mod blocking;
