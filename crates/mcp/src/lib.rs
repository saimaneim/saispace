pub mod config;
mod instructions;
pub mod server;
pub mod tools;

pub use config::Config;
pub use server::SaiSpaceServer;

/// Package version plus the short commit it was built from.
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("SAI_COMMIT"), ")");
