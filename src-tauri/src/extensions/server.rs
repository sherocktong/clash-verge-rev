//! Fork-owned CLI control server.
//!
//! Serves the `clash-verge-cli` command routes on the fixed loopback port
//! 127.0.0.1:33331 instead of merging them into the upstream embedded
//! server, so `utils/server.rs` stays byte-identical to upstream across
//! syncs. The CLI assumes that fixed port, which only the upstream server
//! used before it moved to an ephemeral port; this fork-owned listener
//! restores the contract.

use std::net::{IpAddr, Ipv4Addr};
use std::future::pending;

use anyhow::{Context as _, Result};
use clash_verge_logging::{Type, logging};
use tokio::net::TcpListener;
use warp::Filter;

use crate::{
    config::{Config, IVerge},
    feat,
    process::AsyncHandler,
};

const CLI_PORT: u16 = 33331;

/// Start the fork control server and register fork i18n overlays.
pub fn init() {
    clash_verge_fork_i18n::register();

    AsyncHandler::spawn(move || async move {
        if let Err(err) = serve().await {
            logging!(error, Type::Core, "CLI control server stopped: {err}");
        }
    });
}

async fn serve() -> Result<()> {
    let listener = TcpListener::bind((IpAddr::from(Ipv4Addr::LOCALHOST), CLI_PORT))
        .await
        .context("failed to bind CLI control port")?;

    logging!(
        info,
        Type::Core,
        "CLI control server listening on 127.0.0.1:{CLI_PORT}"
    );
    warp::serve(cli_routes())
        .incoming(listener)
        .graceful(async { pending::<()>().await })
        .run()
        .await;
    Ok(())
}

/// Warp filter with all CLI command routes.
fn cli_routes() -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    // Reload verge config from file (called by CLI)
    let reload_verge = warp::path!("commands" / "reload" / "verge").and_then(|| async {
        logging!(info, Type::Window, "CLI requested verge config reload");
        AsyncHandler::spawn(|| async move {
            // Reload verge config from file
            let verge = Config::verge().await;
            let new_config = IVerge::new().await;
            verge.edit_draft(|d| *d = new_config);
            verge.apply();
            logging!(info, Type::Config, "Verge config reloaded from CLI");
        });
        Ok::<_, warp::Rejection>(warp::reply::with_status::<std::string::String>(
            "ok".to_string(),
            warp::http::StatusCode::OK,
        ))
    });

    // Toggle TUN mode (POST)
    let toggle_tun = warp::post()
        .and(warp::path!("commands" / "toggle" / "tun"))
        .and_then(|| async move {
            logging!(info, Type::Window, "CLI requested TUN toggle");
            let enabled = feat::toggle_tun_mode(None).await;
            logging!(info, Type::Window, "TUN toggled to: {}", enabled);
            let result = serde_json::json!({ "ok": true, "enabled": enabled });
            Ok::<_, warp::Rejection>(warp::reply::json(&result))
        });

    // Enable TUN mode (POST)
    let enable_tun = warp::post()
        .and(warp::path!("commands" / "enable" / "tun"))
        .and_then(|| async move {
            logging!(info, Type::Window, "CLI requested TUN enable");
            let result = match feat::patch_verge(
                &IVerge {
                    enable_tun_mode: Some(true),
                    ..IVerge::default()
                },
                false,
            )
            .await
            {
                Ok(_) => {
                    logging!(info, Type::Window, "TUN enabled successfully");
                    serde_json::json!({ "ok": true })
                }
                Err(e) => {
                    logging!(error, Type::Window, "Failed to enable TUN: {}", e);
                    serde_json::json!({ "ok": false, "error": e.to_string() })
                }
            };
            Ok::<_, warp::Rejection>(warp::reply::json(&result))
        });

    // Disable TUN mode (POST)
    let disable_tun = warp::post()
        .and(warp::path!("commands" / "disable" / "tun"))
        .and_then(|| async move {
            logging!(info, Type::Window, "CLI requested TUN disable");
            let result = match feat::patch_verge(
                &IVerge {
                    enable_tun_mode: Some(false),
                    ..IVerge::default()
                },
                false,
            )
            .await
            {
                Ok(_) => {
                    logging!(info, Type::Window, "TUN disabled successfully");
                    serde_json::json!({ "ok": true })
                }
                Err(e) => {
                    logging!(error, Type::Window, "Failed to disable TUN: {}", e);
                    serde_json::json!({ "ok": false, "error": e.to_string() })
                }
            };
            Ok::<_, warp::Rejection>(warp::reply::json(&result))
        });

    // Toggle system proxy (POST)
    let toggle_sysproxy = warp::post()
        .and(warp::path!("commands" / "toggle" / "sysproxy"))
        .and_then(|| async move {
            logging!(info, Type::Window, "CLI requested system proxy toggle");
            // Get current state and toggle
            let verge = Config::verge().await;
            let current = verge.latest_arc().enable_system_proxy.unwrap_or(false);
            let enable = !current;

            let result = match feat::patch_verge(
                &IVerge {
                    enable_system_proxy: Some(enable),
                    ..IVerge::default()
                },
                false,
            )
            .await
            {
                Ok(_) => {
                    logging!(info, Type::Window, "System proxy toggled to: {}", enable);
                    serde_json::json!({ "ok": true, "enabled": enable })
                }
                Err(e) => {
                    logging!(error, Type::Window, "Failed to toggle system proxy: {}", e);
                    serde_json::json!({ "ok": false, "error": e.to_string() })
                }
            };
            Ok::<_, warp::Rejection>(warp::reply::json(&result))
        });

    // Enable system proxy (POST)
    let enable_sysproxy = warp::post()
        .and(warp::path!("commands" / "enable" / "sysproxy"))
        .and_then(|| async move {
            logging!(info, Type::Window, "CLI requested system proxy enable");
            let result = match feat::patch_verge(
                &IVerge {
                    enable_system_proxy: Some(true),
                    ..IVerge::default()
                },
                false,
            )
            .await
            {
                Ok(_) => {
                    logging!(info, Type::Window, "System proxy enabled successfully");
                    serde_json::json!({ "ok": true })
                }
                Err(e) => {
                    logging!(error, Type::Window, "Failed to enable system proxy: {}", e);
                    serde_json::json!({ "ok": false, "error": e.to_string() })
                }
            };
            Ok::<_, warp::Rejection>(warp::reply::json(&result))
        });

    // Disable system proxy (POST)
    let disable_sysproxy = warp::post()
        .and(warp::path!("commands" / "disable" / "sysproxy"))
        .and_then(|| async move {
            logging!(info, Type::Window, "CLI requested system proxy disable");
            let result = match feat::patch_verge(
                &IVerge {
                    enable_system_proxy: Some(false),
                    ..IVerge::default()
                },
                false,
            )
            .await
            {
                Ok(_) => {
                    logging!(info, Type::Window, "System proxy disabled successfully");
                    serde_json::json!({ "ok": true })
                }
                Err(e) => {
                    logging!(error, Type::Window, "Failed to disable system proxy: {}", e);
                    serde_json::json!({ "ok": false, "error": e.to_string() })
                }
            };
            Ok::<_, warp::Rejection>(warp::reply::json(&result))
        });

    // Get current status (GET)
    let status = warp::get()
        .and(warp::path!("commands" / "status"))
        .and_then(|| async move {
            let verge = Config::verge().await.latest_arc();
            let result = serde_json::json!({
                "ok": true,
                "enable_tun_mode": verge.enable_tun_mode.unwrap_or(false),
                "enable_system_proxy": verge.enable_system_proxy.unwrap_or(false),
                "verge_mixed_port": verge.verge_mixed_port.unwrap_or(7897),
                "proxy_auto_config": verge.proxy_auto_config.unwrap_or(false),
            });
            Ok::<_, warp::Rejection>(warp::reply::json(&result))
        });

    reload_verge
        .or(toggle_tun)
        .or(enable_tun)
        .or(disable_tun)
        .or(toggle_sysproxy)
        .or(enable_sysproxy)
        .or(disable_sysproxy)
        .or(status)
}
