//! An idiomatic Rust implementation of classic Siemens S7 communication.
//!
//! See `docs/migration.md` for implemented capabilities and compatibility limits.

pub mod error;
pub mod types;

pub use error::{Error, Result};
pub use types::{Address, Area, BlockType, CpuState, Parameter, ServerArea, WordLen};
