//! Fork-specific extensions to clash-verge-rev.
//!
//! All extension logic lives in this module so that upstream files stay
//! byte-identical to upstream wherever possible. Upstream files contain
//! only two one-line hooks into this module:
//! - `lib.rs`: `mod extensions;`
//! - `utils/resolve/mod.rs`: `crate::extensions::init();`

mod proxy_connectivity;
mod server;

use clash_verge_logging::{Type, logging};

/// Initialize all fork extensions. Called once during async setup.
pub fn init() {
    logging!(info, Type::Setup, "Initializing extensions...");
    server::init();
    proxy_connectivity::ProxyConnectivityChecker::start();
    logging!(info, Type::Setup, "Extensions initialized");
}
