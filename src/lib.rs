#[cfg(feature = "cli")]
pub mod cli;

#[cfg(feature = "mcp")]
pub mod server;

#[cfg(feature = "mcp")]
pub mod tools;

#[cfg(feature = "mcp")]
pub mod resources;

#[cfg(feature = "mcp")]
pub mod logs;

pub mod utils;
pub mod embed;

pub use embed::Embedder;