use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant as StdInstant};

use parking_lot::RwLock;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::{self, Instant as TokioInstant};

use crate::config::RouteConfig;
use crate::daemon::{ActiveNode, SharedNodes, NOT_CONNECTED};

pub fn extract_assets(assets_dir: &Path) -> std::io::Result<PathBuf> {
    let tor_bin_path = assets_dir.join(if cfg!(windows) { "tor.exe" } else { "tor" });
    let geoip_path   = assets_dir.join("geoip");
    let geoip6_path  = assets_dir.join("geoip6");

    std::fs::write(&tor_bin_path, crate::TOR_BINARY_DATA)?;
    std::fs::write(&geoip_path,   crate::GEOIP_DATA)?;
    std::fs::write(&geoip6_path,  crate::GEOIP6_DATA)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tor_bin_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tor_bin_path, perms)?;
    }

    Ok(tor_bin_path)
}

fn write_torrc(
    route: &RouteConfig,
    tor_data_root: &Path,
    geoip_path: &Path,
    geoip6_path: &Path,
) -> std::io::Result<(PathBuf, PathBuf)> {
    let instance_dir = tor_data_root.join(&route.name);
    std::fs::create_dir_all(&instance_dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&instance_dir, std::fs::Permissions::from_mode(0o700));
    }

    let bind_address       = route.bind_address.clone().unwrap_or_else(|| "0.0.0.0".to_string());
    let torrc_path         = instance_dir.join("torrc");
    let control_port_file  = instance_dir.join("control-port");
    let cookie_file        = instance_dir.join("control_auth_cookie");

    let mut torrc = String::new();
    torrc.push_str(&format!("SocksPort {}:{}\n", bind_address, route.input_port));
    torrc.push_str(&format!("DataDirectory {}\n", instance_dir.display()));
    torrc.push_str(&format!("GeoIPFile {}\n",     geoip_path.display()));
    torrc.push_str(&format!("GeoIPv6File {}\n",   geoip6_path.display()));
    torrc.push_str(&format!("ExitNodes {{{}}}\n", route.country_code.to_lowercase()));
    torrc.push_str("StrictNodes 1\n");
    torrc.push_str("ControlPort auto\n");
    torrc.push_str(&format!("ControlPortWriteToFile {}\n", control_port_file.display()));
    torrc.push_str("CookieAuthentication 1\n");
    torrc.push_str(&format!("CookieAuthFile {}\n", cookie_file.display()));
    torrc.push_str("Log notice stdout\n");
    torrc.push_str("AvoidDiskWrites 1\n");

    std::fs::write(&torrc_path, torrc)?;
    Ok((torrc_path, instance_dir))
}

async fn read_control_port(instance_dir: &Path) -> Option<String> {
    let content = tokio::fs::read_to_string(instance_dir.join("control-port")).await.ok()?;
    content.lines()
        .find(|l| l.starts_with("PORT="))
        .map(|l| l.trim_start_matches("PORT=").trim().to_string())
}

async fn read_auth_cookie(instance_dir: &Path) -> Option<Vec<u8>> {
    tokio::fs::read(instance_dir.join("control_auth_cookie")).await.ok()
}

async fn send_newnym(addr: &str, cookie: &[u8]) -> std::io::Result<()> {
    let mut stream = TcpStream::connect(addr).await?;
    let hex: String = cookie.iter().map(|b| format!("{:02x}", b)).collect();

    stream.write_all(format!("AUTHENTICATE {}\r\n", hex).as_bytes()).await?;
    let mut buf = [0u8; 512];
    let _ = stream.read(&mut buf).await?;

    stream.write_all(b"SIGNAL NEWNYM\r\n").await?;
    let _ = stream.read(&mut buf).await?;
    let _ = stream.write_all(b"QUIT\r\n").await;
    Ok(())
}

#[derive(serde::Deserialize)]
struct TorIpResponse {
    #[serde(rename = "IP")]
    ip: String,
}

/// Sends a test request through the route's SOCKS5 port.
/// Returns (latency, Option<tor_exit_ip>).
async fn measure_latency(proxy_url: &str) -> (Duration, Option<String>) {
    let proxy = match reqwest::Proxy::all(proxy_url) {
        Ok(p) => p,
        Err(_) => return (NOT_CONNECTED, None),
    };
    let client = match reqwest::Client::builder()
        .proxy(proxy)
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (NOT_CONNECTED, None),
    };

    let start = StdInstant::now();
    match client.get("https://check.torproject.org/api/ip").send().await {
        Ok(resp) if resp.status().is_success() => {
            let elapsed = start.elapsed();
            let ip = resp.json::<TorIpResponse>().await.ok().map(|r| r.ip);
            (elapsed, ip)
        }
        _ => (NOT_CONNECTED, None),
    }
}

/// Formats the current UTC time as ISO 8601 (without the chrono dependency).
fn now_iso() -> String {
    // SystemTime → seconds since epoch → hand-format
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple approach: emit as a Unix timestamp string; the panel's
    // `new Date(value).toLocaleTimeString()` will parse it correctly if we
    // pass milliseconds.
    format!("{}", secs * 1000)
}

pub fn spawn_route(
    route: RouteConfig,
    tor_bin: PathBuf,
    tor_data_root: PathBuf,
    geoip_path: PathBuf,
    geoip6_path: PathBuf,
    global_nodes: SharedNodes,
    db_path: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let (torrc_path, instance_dir) =
                match write_torrc(&route, &tor_data_root, &geoip_path, &geoip6_path) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("❌ [{}] Failed to write torrc: {}", route.name, e);
                        time::sleep(Duration::from_secs(30)).await;
                        continue;
                    }
                };

            let latency         = Arc::new(RwLock::new(NOT_CONNECTED));
            let tor_ip          = Arc::new(RwLock::new(None::<String>));
            let last_checked_at = Arc::new(RwLock::new(None::<String>));

            global_nodes.write().insert(
                route.name.clone(),
                Arc::new(ActiveNode {
                    latency: latency.clone(),
                    tor_ip: tor_ip.clone(),
                    last_checked_at: last_checked_at.clone(),
                }),
            );

            let mut cmd = Command::new(&tor_bin);
            cmd.arg("-f").arg(&torrc_path)
               .stdout(Stdio::piped())
               .stderr(Stdio::null())
               .kill_on_drop(true);

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("❌ [{}] Failed to spawn: {}", route.name, e);
                    global_nodes.write().remove(&route.name);
                    time::sleep(Duration::from_secs(30)).await;
                    continue;
                }
            };

            let stdout = child.stdout.take().expect("piped stdout");
            let (bootstrap_tx, bootstrap_rx) = tokio::sync::oneshot::channel::<bool>();
            let log_name = route.name.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                let mut bootstrap_tx = Some(bootstrap_tx);
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.contains("Bootstrapped 100%") {
                        if let Some(tx) = bootstrap_tx.take() { let _ = tx.send(true); }
                    }
                    if line.contains("[warn]") || line.contains("[err]") {
                        eprintln!("[{}] {}", log_name, line);
                    }
                }
                if let Some(tx) = bootstrap_tx.take() { let _ = tx.send(false); }
            });

            let bootstrapped = tokio::select! {
                r = bootstrap_rx => r.unwrap_or(false),
                _ = time::sleep(Duration::from_secs(120)) => false,
            };

            if !bootstrapped {
                eprintln!("⚠️ [{}] Bootstrap timeout, retrying...", route.name);
                let _ = child.kill().await;
                global_nodes.write().remove(&route.name);
                time::sleep(Duration::from_secs(15)).await;
                continue;
            }

            println!("✅ [{}] Circuit ready (exit: {})", route.name, route.country_code.to_uppercase());

            let control_addr = read_control_port(&instance_dir).await;
            let cookie       = read_auth_cookie(&instance_dir).await;

            let bind_address = route.bind_address.clone().unwrap_or_else(|| "0.0.0.0".to_string());
            let proxy_url    = format!("socks5h://{}:{}", bind_address, route.input_port);

            let test_every   = Duration::from_secs(route.test_interval_minutes.unwrap_or(15) * 60);
            let swap_after   = Duration::from_secs(route.swap_interval_hours.unwrap_or(24) * 3600);

            let mut test_ticker = time::interval(test_every);
            let mut next_swap   = TokioInstant::now() + swap_after;

            // First check immediately
            let (lat, ip) = measure_latency(&proxy_url).await;
            *latency.write()         = lat;
            *tor_ip.write()          = ip.clone();
            *last_checked_at.write() = Some(now_iso());

            let _ = crate::config::update_route_state_by_name(
                &db_path,
                &route.name,
                ip.as_deref(),
                last_checked_at.read().as_deref(),
            );

            let exit_status: Option<std::io::Result<std::process::ExitStatus>>;
            loop {
                tokio::select! {
                    status = child.wait() => { exit_status = Some(status); break; }
                    _ = test_ticker.tick() => {
                        let (lat, ip) = measure_latency(&proxy_url).await;
                        *latency.write()         = lat;
                        *tor_ip.write()          = ip.clone();
                        *last_checked_at.write() = Some(now_iso());
                    }
                    _ = time::sleep_until(next_swap) => {
                        if let (Some(addr), Some(ck)) = (&control_addr, &cookie) {
                            match send_newnym(addr, ck).await {
                                Ok(_)  => println!("🔄 [{}] New circuit (NEWNYM)", route.name),
                                Err(e) => eprintln!("⚠️ [{}] NEWNYM failed: {}", route.name, e),
                            }
                        }
                        if let Err(e) = crate::config::update_route_state_by_name(
                            &db_path,
                            &route.name,
                            tor_ip.read().as_deref(),
                            last_checked_at.read().as_deref(),
                        ) {
                            eprintln!("⚠️ [{}] failed to persist swap state: {}", route.name, e);
                        }
                        next_swap = TokioInstant::now() + swap_after;
                    }
                }
            }

            global_nodes.write().remove(&route.name);
            match exit_status {
                Some(Ok(s))  => eprintln!("⚠️ [{}] Exited ({}), restarting...", route.name, s),
                Some(Err(e)) => eprintln!("⚠️ [{}] Error ({}), restarting...", route.name, e),
                None => eprintln!("⚠️ [{}] Child ended unexpectedly, restarting...", route.name),
            }
            time::sleep(Duration::from_secs(5)).await;
        }
    })
}