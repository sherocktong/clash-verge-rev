use super::resolve;
use crate::{
    cmd::is_port_in_use,
    config::{Config, DEFAULT_PAC, IVerge},
    feat,
    module::lightweight,
    process::AsyncHandler,
    utils::window_manager::WindowManager,
};
use anyhow::{Result, bail};
use clash_verge_logging::{Type, logging, logging_error};
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use reqwest::ClientBuilder;
use smartstring::alias::String;
use std::time::Duration;
use tokio::sync::oneshot;
use warp::Filter as _;

#[derive(serde::Deserialize, Debug)]
struct QueryParam {
    param: String,
}

// 关闭 embedded server 的信号发送端
static SHUTDOWN_SENDER: OnceCell<Mutex<Option<oneshot::Sender<()>>>> = OnceCell::new();

/// check whether there is already exists
pub async fn check_singleton() -> Result<()> {
    let port = IVerge::get_singleton_port();
    if is_port_in_use(port) {
        let client = ClientBuilder::new().timeout(Duration::from_millis(500)).build()?;
        // 需要确保 Send
        #[allow(clippy::needless_collect)]
        let argvs: Vec<std::string::String> = std::env::args().collect();
        if argvs.len() > 1 {
            #[cfg(not(target_os = "macos"))]
            {
                let param = argvs[1].as_str();
                if param.starts_with("clash:") {
                    client
                        .get(format!("http://127.0.0.1:{port}/commands/scheme?param={param}"))
                        .send()
                        .await?;
                }
            }
        } else {
            client
                .get(format!("http://127.0.0.1:{port}/commands/visible"))
                .send()
                .await?;
        }
        logging!(error, Type::Window, "failed to setup singleton listen server");
        bail!("app exists");
    }
    Ok(())
}

/// The embed server only be used to implement singleton process
/// maybe it can be used as pac server later
pub fn embed_server() {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    #[allow(clippy::expect_used)]
    SHUTDOWN_SENDER
        .set(Mutex::new(Some(shutdown_tx)))
        .expect("failed to set shutdown signal for embedded server");
    let port = IVerge::get_singleton_port();

    let visible = warp::path!("commands" / "visible").and_then(|| async {
        logging!(info, Type::Window, "检测到从单例模式恢复应用窗口");
        if !lightweight::exit_lightweight_mode().await {
            WindowManager::show_main_window().await;
        } else {
            logging!(error, Type::Window, "轻量模式退出失败，无法恢复应用窗口");
        };
        Ok::<_, warp::Rejection>(warp::reply::with_status::<std::string::String>(
            "ok".to_string(),
            warp::http::StatusCode::OK,
        ))
    });

    let pac = warp::path!("commands" / "pac").and_then(|| async move {
        let verge_config = Config::verge().await;
        let clash_config = Config::clash().await;

        let verge_data = verge_config.data_arc();
        let clash_data = clash_config.data_arc();

        let pac_content = verge_data.pac_file_content.as_deref().unwrap_or(DEFAULT_PAC);

        let pac_port = verge_data
            .verge_mixed_port
            .unwrap_or_else(|| clash_data.get_mixed_port());
        let processed_content = pac_content.replace("%mixed-port%", &format!("{pac_port}"));
        Ok::<_, warp::Rejection>(
            warp::http::Response::builder()
                .header("Content-Type", "application/x-ns-proxy-autoconfig")
                .body(processed_content)
                .unwrap_or_default(),
        )
    });

    // Use map instead of and_then to avoid Send issues
    let scheme = warp::path!("commands" / "scheme")
        .and(warp::query::<QueryParam>())
        .and_then(|query: QueryParam| async move {
            AsyncHandler::spawn(|| async move {
                logging_error!(Type::Setup, resolve::resolve_scheme(&query.param).await);
            });
            Ok::<_, warp::Rejection>(warp::reply::with_status::<std::string::String>(
                "ok".to_string(),
                warp::http::StatusCode::OK,
            ))
        });

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

    let commands = visible
        .or(scheme)
        .or(pac)
        .or(reload_verge)
        .or(toggle_tun)
        .or(enable_tun)
        .or(disable_tun)
        .or(toggle_sysproxy)
        .or(enable_sysproxy)
        .or(disable_sysproxy)
        .or(status);

    AsyncHandler::spawn(move || async move {
        warp::serve(commands)
            .bind(([127, 0, 0, 1], port))
            .await
            .graceful(async {
                shutdown_rx.await.ok();
            })
            .run()
            .await;
    });
}

pub fn shutdown_embedded_server() {
    logging!(info, Type::Window, "shutting down embedded server");
    if let Some(sender) = SHUTDOWN_SENDER.get()
        && let Some(sender) = sender.lock().take()
    {
        sender.send(()).ok();
    }
}
