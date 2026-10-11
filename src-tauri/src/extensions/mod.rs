//! Fork-specific extensions to clash-verge-rev.
//!
//! All extension logic lives in this module so that upstream files stay
//! byte-identical to upstream wherever possible. Upstream files contain
//! only three one-line hooks into this module:
//! - `lib.rs`: `mod extensions;`
//! - `utils/server.rs`: `.or(crate::extensions::cli_routes())`
//! - `utils/resolve/mod.rs`: `crate::extensions::init();`

mod cli_routes;
mod proxy_connectivity;

pub use cli_routes::cli_routes;

use clash_verge_logging::{Type, logging};

/// Initialize all fork extensions. Called once during async setup.
pub fn init() {
    logging!(info, Type::Setup, "Initializing extensions...");
    proxy_connectivity::ProxyConnectivityChecker::start();
    logging!(info, Type::Setup, "Extensions initialized");
}
