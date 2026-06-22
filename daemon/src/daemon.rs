use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::Duration;
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tokio::time;

use crate::api::start_web_server;
use crate::config::{Config, RouteConfig, init_db};

pub const NOT_CONNECTED: Duration = Duration::from_secs(3596400);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

pub struct ActiveNode {
    pub latency: Arc<RwLock<Duration>>,
    pub tor_ip: Arc<RwLock<Option<String>>>,
    pub last_checked_at: Arc<RwLock<Option<String>>>,
}

pub type SharedNodes = Arc<RwLock<HashMap<String, Arc<ActiveNode>>>>;

async fn stop_route(name: &str, handle: tokio::task::JoinHandle<()>) {
    handle.abort();
    if time::timeout(SHUTDOWN_TIMEOUT, handle).await.is_err() {
        eprintln!(
            "⚠️ [{}] Old process didn't confirm shutdown within {}s.",
            name, SHUTDOWN_TIMEOUT.as_secs()
        );
    }
}

pub async fn run_daemon(db_path: &str, api_bind: &str, web_dir: Option<String>) {
    let abs_db_path = match fs::canonicalize(db_path) {
        Ok(p) => p,
        Err(_) => PathBuf::from(db_path),
    };
    let abs_db_str = abs_db_path.to_str().unwrap_or(db_path).to_string();

    // Bootstrap DB schema
    if let Err(e) = init_db(&abs_db_str) {
        eprintln!("❌ Failed to init database: {}", e);
        process::exit(1);
    }

    let pid = process::id();
    let temp_dir = std::env::temp_dir();
    let assets_dir = temp_dir.join(format!("tor-router-assets-{}", pid));
    let tor_data_dir_base = temp_dir.join(format!("tor-router-data-{}", pid));

    fs::create_dir_all(&assets_dir).unwrap();
    fs::create_dir_all(&tor_data_dir_base).unwrap();

    let tor_bin_path = match crate::tor_process::extract_assets(&assets_dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ Failed to extract embedded Tor assets: {}", e);
            process::exit(1);
        }
    };
    let geoip_path = assets_dir.join("geoip");
    let geoip6_path = assets_dir.join("geoip6");

    let assets_clone = assets_dir.clone();
    let tor_clone = tor_data_dir_base.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        println!("🛑 Exit signal received! Cleaning up...");
        let _ = fs::remove_dir_all(assets_clone);
        let _ = fs::remove_dir_all(tor_clone);
        process::exit(0);
    });

    println!("✅ Daemon started (PID: {}). DB: {:?}", pid, abs_db_path);
    println!("💡 Tip: type '\x1b[36mtor-p\x1b[0m' in a new terminal to open the CLI.");
    println!("Press Ctrl+C to exit.");

    let (restart_tx, mut restart_rx) = mpsc::channel::<String>(32);
    let global_nodes: SharedNodes = Arc::new(RwLock::new(HashMap::new()));

    // Start combined web panel + API server
    let api_nodes = global_nodes.clone();
    let bind = api_bind.to_string();
    let tx = restart_tx.clone();
    let db_for_api = abs_db_str.clone();
    tokio::spawn(async move {
        start_web_server(bind, tx, api_nodes, db_for_api, web_dir).await;
    });

    let mut active_routes: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();
    let mut active_configs: HashMap<String, RouteConfig> = HashMap::new();

    let mut ticker = time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            route_name = restart_rx.recv() => {
                if let Some(name) = route_name {
                    if let Some(handle) = active_routes.remove(&name) {
                        global_nodes.write().remove(&name);
                        active_configs.remove(&name);
                        println!("🔄 [{}] Stopping old process...", name);
                        stop_route(&name, handle).await;
                        println!("✅ [{}] Stopped, will respawn on next cycle", name);
                    }
                }
            }
            _ = ticker.tick() => {
                if let Ok(config) = crate::config::load_from_db(&abs_db_str) {
                    reload_config(
                        config,
                        &mut active_routes,
                        &mut active_configs,
                        global_nodes.clone(),
                        &tor_bin_path,
                        &tor_data_dir_base,
                        &geoip_path,
                        &geoip6_path,
                        &abs_db_str,
                    ).await;
                }
            }
        }
    }
}

async fn reload_config(
    config: Config,
    active_handles: &mut HashMap<String, tokio::task::JoinHandle<()>>,
    active_configs: &mut HashMap<String, RouteConfig>,
    global_nodes: SharedNodes,
    tor_bin: &PathBuf,
    tor_data_root: &PathBuf,
    geoip_path: &PathBuf,
    geoip6_path: &PathBuf,
    db_path: &str,
) {
    let mut new_routes: HashMap<String, RouteConfig> = HashMap::new();
    for mut r in config.routes {
        if r.swap_interval_hours.unwrap_or(0) == 0 { r.swap_interval_hours = Some(24); }
        if r.test_interval_minutes.unwrap_or(0) < 1 { r.test_interval_minutes = Some(15); }
        new_routes.insert(r.name.clone(), r);
    }

    // Stop routes removed from DB or whose config changed
    let to_stop: Vec<String> = active_configs
        .iter()
        .filter(|(name, old)| match new_routes.get(*name) {
            None => true,
            Some(new) => *new != **old,
        })
        .map(|(n, _)| n.clone())
        .collect();

    let mut waiters = Vec::new();
    for name in &to_stop {
        if let Some(handle) = active_handles.remove(name) {
            global_nodes.write().remove(name);
            active_configs.remove(name);
            println!("🛑 [{}] Stopping...", name);
            let n = name.clone();
            waiters.push(tokio::spawn(async move {
                stop_route(&n, handle).await;
                println!("✅ [{}] Stopped", n);
            }));
        }
    }
    for w in waiters { let _ = w.await; }

    // Start new or changed routes
    for (name, route) in &new_routes {
        if !active_handles.contains_key(name) {
            println!("🚀 [{}] Starting -> exit country {}", name, route.country_code.to_uppercase());
            let handle = crate::tor_process::spawn_route(
                route.clone(),
                tor_bin.clone(),
                tor_data_root.clone(),
                geoip_path.clone(),
                geoip6_path.clone(),
                global_nodes.clone(),
                db_path.to_string(),
            );
            active_handles.insert(name.clone(), handle);
            active_configs.insert(name.clone(), route.clone());
        }
    }
}