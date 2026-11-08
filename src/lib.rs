//! An idiomatic Rust implementation of classic Siemens S7 communication.
//!
//! See `docs/migration.md` for implemented capabilities and compatibility limits.

pub mod client;
pub mod codec;
pub mod error;
mod operations;
mod planner;
pub mod protocol;
mod session;
pub mod szl;
pub mod transport;
pub mod types;

pub use client::{Client, ClientConfig};
pub use error::{Error, Result};
pub use types::{Address, Area, BlockType, CpuState, Parameter, ServerArea, WordLen};
