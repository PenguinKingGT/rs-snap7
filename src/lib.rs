//! An idiomatic Rust implementation of classic Siemens S7 communication.
//!
//! See `docs/migration.md` for implemented capabilities and compatibility limits.

//! # Blocking access
//!
//! ```no_run
//! use rs_snap7::{Client, ClientConfig};
//! # fn main() -> rs_snap7::Result<()> {
//! let endpoint = "192.168.1.10:102".parse().expect("valid PLC endpoint");
//! let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?)?;
//! let _value = client.db_read_int(1, 0)?;
//! client.disconnect()?;
//! # Ok(())
//! # }
//! ```

pub mod blocks;
pub mod client;
pub mod codec;
pub mod error;
pub mod logo;
pub mod observability;
mod operations;
mod planner;
pub mod policy;
pub mod pool;
pub mod protocol;
mod session;
pub mod szl;
pub mod transport;
pub mod types;

pub use client::{Client, ClientConfig};
pub use error::{Error, Result};
pub use logo::Logo;
pub use types::{Address, Area, BlockType, CpuState, Parameter, ServerArea, WordLen};

#[cfg(feature = "async")]
pub mod async_client;
#[cfg(feature = "async")]
pub use async_client::AsyncClient;

#[cfg(feature = "async")]
pub use async_client::AsyncClientPool;
pub use pool::ClientPool;
