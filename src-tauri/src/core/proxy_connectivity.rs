use crate::{
    config::Config,
    core::handle,
    process::AsyncHandler,
    utils::notification::{NotificationEvent, notify_event},
};
use clash_verge_logging::{Type, logging};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{Duration, sleep};

/// Interval between connectivity checks (10 minutes)
const CHECK_INTERVAL: Duration = Duration::from_secs(600);
/// Timeout for each connectivity test
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
/// Default test URL
const DEFAULT_TEST_URL: &str = "http://www.gstatic.com/generate_204";

static RUNNING: AtomicBool = AtomicBool::new(false);

pub struct ProxyConnectivityChecker;

impl ProxyConnectivityChecker {
    /// Start the background proxy connectivity checker.
    pub fn start() {
        if RUNNING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        AsyncHandler::spawn(move || async move {
            logging!(info, Type::System, "Proxy connectivity checker started");

            loop {
                if handle::Handle::global().is_exiting() {
                    break;
                }

                sleep(CHECK_INTERVAL).await;

                if handle::Handle::global().is_exiting() {
                    break;
                }

                Self::check_once().await;
            }

            RUNNING.store(false, Ordering::SeqCst);
            logging!(info, Type::System, "Proxy connectivity checker stopped");
        });
    }

    async fn check_once() {
        let verge = Config::verge().await.latest_arc();
        let proxy_enabled = verge.enable_system_proxy.unwrap_or(false) || verge.enable_tun_mode.unwrap_or(false);

        if !proxy_enabled {
            return;
        }

        let mixed_port = match verge.verge_mixed_port {
            Some(p) => p,
            None => Config::clash().await.data_arc().get_mixed_port(),
        };

        let proxy_url = format!("http://127.0.0.1:{mixed_port}");
        let result = tokio::time::timeout(CHECK_TIMEOUT, async {
            let client = reqwest::Client::builder()
                .proxy(reqwest::Proxy::all(&proxy_url).map_err(|e| e.to_string())?)
                .timeout(CHECK_TIMEOUT)
                .build()
                .map_err(|e| e.to_string())?;

            let response = client
                .get(DEFAULT_TEST_URL)
                .send()
                .await
                .map_err(|e| format!("Request failed: {e}"))?;

            if !response.status().is_success() {
                return Err(format!("HTTP {}", response.status()));
            }

            Ok::<(), String>(())
        })
        .await;

        match result {
            Ok(Ok(())) => {
                logging!(debug, Type::System, "Proxy connectivity check passed");
            }
            Ok(Err(e)) => {
                logging!(warn, Type::System, "Proxy connectivity check failed: {e}");
                notify_event(NotificationEvent::ProxyConnectivityFailed).await;
            }
            Err(_) => {
                logging!(warn, Type::System, "Proxy connectivity check timed out");
                notify_event(NotificationEvent::ProxyConnectivityFailed).await;
            }
        }
    }
}
