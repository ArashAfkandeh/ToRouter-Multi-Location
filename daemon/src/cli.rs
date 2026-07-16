use reqwest;
use serde::Deserialize;
use std::io::{self, Write};
use std::process::Command;
use std::thread;
use std::time::Duration;

lazy_static::lazy_static! {
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::builder().timeout(Duration::from_secs(6)).build().unwrap();
}

#[derive(Deserialize, Debug)]
struct RouteStatus {
    name: String,
    #[serde(alias = "country_code")]
    country: String,
    bind_address: String,
    #[serde(alias = "input_port")]
    port: u16,
    latency: String,
    #[serde(default)]
    tor_ip: Option<String>,
    #[serde(default)]
    last_checked_at: Option<String>,
}

#[derive(Deserialize, Debug)]
struct RouteApiItem {
    id: String,
    name: String,
    bind_address: String,
    input_port: u16,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    country_code: String,
    swap_interval_minutes: u64,
    test_interval_minutes: u64,
    latency: String,
    #[serde(default)]
    tor_ip: Option<String>,
    #[serde(default)]
    last_checked_at: Option<String>,
}

async fn auto_login(api_url: &str) -> Option<String> {
    let exe_path = std::env::current_exe().unwrap_or_default();
    let dir = exe_path.parent().unwrap_or(std::path::Path::new("."));
    let db_path = dir.join("ToRouter.sqlite");
    
    if let Ok(settings) = crate::config::load_settings(db_path.to_str().unwrap_or("ToRouter.sqlite")) {
        let payload = serde_json::json!({
            "username": settings.admin_username,
            "password": settings.admin_password
        });
        
        let client = HTTP_CLIENT.clone();
        if let Ok(res) = client.post(&format!("{}/api/login", api_url)).json(&payload).send().await {
            if res.status().is_success() {
                if let Some(cookie) = res.headers().get(reqwest::header::SET_COOKIE) {
                    if let Ok(c) = cookie.to_str() {
                        if let Some(s) = c.split(';').find(|p| p.trim().starts_with("session=")) {
                            return Some(s.trim().to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

pub async fn run_cli(api_url_base: &str) {
    let mut api_url = api_url_base.trim_end_matches('/').to_string();
    let mut session_cookie: Option<String> = None;

    let exe_path = std::env::current_exe().unwrap_or_default();
    let dir = exe_path.parent().unwrap_or(std::path::Path::new("."));
    let db_path = dir.join("ToRouter.sqlite");
    if let Ok(settings) = crate::config::load_settings(db_path.to_str().unwrap_or("ToRouter.sqlite")) {
        let has_domain = !settings.domain.as_deref().unwrap_or("").trim().is_empty();
        let scheme = if settings.use_custom_cert || has_domain { "https" } else { "http" };
        let mut connect_addr = settings.web_bind_address.clone();
        if connect_addr == "0.0.0.0" {
            connect_addr = "127.0.0.1".to_string();
        }
        if has_domain {
            connect_addr = settings.domain.clone().unwrap();
        }
        let mut base_path = settings.web_base_path.trim().trim_end_matches('/').to_string();
        if !base_path.is_empty() && !base_path.starts_with('/') {
            base_path = format!("/{}", base_path);
        }
        api_url = format!("{}://{}:{}{}", scheme, connect_addr, settings.web_panel_port, base_path);
    }

    loop {
        if session_cookie.is_none() {
            session_cookie = auto_login(&api_url).await;
        }

        let srv_status = get_service_status().await;
        let is_active = srv_status.contains("Active") && !srv_status.contains("Inactive");

        clear_screen();
        println!("\x1b[1m\x1b[36m╔════════════════════════════════════════════════════════╗\x1b[0m");
        println!("\x1b[1m\x1b[36m║                  🚀 ToRouter CLI                       ║\x1b[0m");
        println!("\x1b[1m\x1b[36m╚════════════════════════════════════════════════════════╝\x1b[0m");
        println!("  Service Status: {}\n", srv_status);

        if is_active {
            println!("\x1b[1m\x1b[33m▶ ROUTING MANAGEMENT\x1b[0m");
            println!("  \x1b[36m1.\x1b[0m 📊 View Live Status");
            println!("  \x1b[36m2.\x1b[0m 🔄 Restart a Route");
            println!("  \x1b[36m3.\x1b[0m ⚙️ Restart ALL Routes");
            println!("  \x1b[36m4.\x1b[0m ➕ Create Route");
            println!("  \x1b[36m5.\x1b[0m ✏️ Edit Route");
            println!("  \x1b[36m6.\x1b[0m 🗑️ Delete Route\n");
            
            println!("\x1b[1m\x1b[33m▶ SETTINGS & INFO\x1b[0m");
            println!("  \x1b[36m7.\x1b[0m 🌐 Change Web/API Ports");
            println!("  \x1b[36m8.\x1b[0m 🔐 Update Settings & Credentials");
            println!("  \x1b[36m9.\x1b[0m ℹ️ View Panel Info & Credentials\n");
        } else {
            println!("\x1b[1m\x1b[31m▶ ROUTING MANAGEMENT & SETTINGS\x1b[0m");
            println!("  \x1b[90m[ API options are disabled while service is inactive ]\x1b[0m");
            println!("  \x1b[36m9.\x1b[0m ℹ️ View Panel Info & Credentials\n");
        }

        println!("\x1b[1m\x1b[33m▶ SYSTEM & SERVICE\x1b[0m");
        if !is_active {
            println!("  \x1b[36m10.\x1b[0m 🚀 Start Service");
        } else {
            println!("  \x1b[36m11.\x1b[0m 🛑 Stop Service");
            println!("  \x1b[36m12.\x1b[0m 🔄 Restart Service");
        }
        println!("  \x1b[36m13.\x1b[0m 🔄 Upgrade/Downgrade");
        println!("  \x1b[36m14.\x1b[0m 🗑️ Uninstall\n");

        println!("  \x1b[36m0.\x1b[0m ❌ Exit\n");
        print!("👉 Select an option: ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let choice = input.trim();
        
        if choice.is_empty() {
            println!("\n👋 Exiting CLI.");
            return;
        }

        match choice {
            "1" if is_active => {
                view_live_status_loop(&api_url, session_cookie.as_deref()).await;
                pause();
            }
            "2" if is_active => {
                restart_route(&api_url).await;
                pause();
            }
            "3" if is_active => {
                restart_all(&api_url).await;
                pause();
            }
            "4" if is_active => {
                create_route_cli(&api_url, session_cookie.as_deref()).await;
                pause();
            }
            "5" if is_active => {
                edit_route_cli(&api_url, session_cookie.as_deref()).await;
                pause();
            }
            "6" if is_active => {
                delete_route_cli(&api_url, session_cookie.as_deref()).await;
                pause();
            }
            "7" if is_active => {
                change_ports_with_session(&api_url, session_cookie.as_deref()).await;
                pause();
            }
            "8" if is_active => {
                update_admin_credentials(&api_url, session_cookie.as_deref()).await;
                session_cookie = None;
                pause();
            }
            "9" => {
                display_panel_info().await;
                pause();
            }
            "10" => {
                start_service().await;
                pause();
            }
            "11" => {
                stop_service().await;
                pause();
            }
            "12" => {
                restart_systemd_service().await;
                pause();
            }
            "13" => {
                update_downgrade().await;
                pause();
            }
            "14" => {
                if uninstall_app().await {
                    return; // exit the CLI
                }
                pause();
            }
            "0" => {
                println!("\n👋 Exiting CLI.");
                return;
            }
            _ => {
                println!("\x1b[31m⚠️ Invalid option or service inactive!\x1b[0m");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

fn clear_screen() {
    if cfg!(target_os = "windows") {
        let _ = Command::new("cmd").args(["/c", "cls"]).status();
    } else {
        let _ = Command::new("clear").status();
    }
}

fn pause() {
    print!("\n\x1b[35mPress ENTER to return to menu...\x1b[0m");
    io::stdout().flush().unwrap();
    let mut unused = String::new();
    let _ = io::stdin().read_line(&mut unused);
}

async fn display_panel_info() {
    clear_screen();
    println!("\x1b[1m\x1b[36m═══ ℹ️ Panel Info & Credentials ═══\x1b[0m\n");
    let exe_path = std::env::current_exe().unwrap_or_default();
    let dir = exe_path.parent().unwrap_or(std::path::Path::new("."));
    let db_path = dir.join("ToRouter.sqlite");
    
    if let Ok(settings) = crate::config::load_settings(db_path.to_str().unwrap_or("ToRouter.sqlite")) {
        let has_domain = !settings.domain.as_deref().unwrap_or("").trim().is_empty();
        let scheme = if settings.use_custom_cert || has_domain { "https" } else { "http" };
        let mut bind = settings.web_bind_address.clone();
        if bind == "0.0.0.0" {
            if let Ok(res) = HTTP_CLIENT.get("https://api.ipify.org").send().await {
                if let Ok(ip) = res.text().await {
                    let ip = ip.trim().to_string();
                    if !ip.is_empty() {
                        bind = ip;
                    } else {
                        bind = "127.0.0.1".to_string();
                    }
                } else {
                    bind = "127.0.0.1".to_string();
                }
            } else {
                bind = "127.0.0.1".to_string();
            }
        }
        let host = if has_domain { settings.domain.clone().unwrap() } else { bind };
        let port = settings.web_panel_port;
        let mut base_path = settings.web_base_path.trim().trim_end_matches('/').to_string();
        if !base_path.is_empty() && !base_path.starts_with('/') {
            base_path = format!("/{}", base_path);
        }
        if !base_path.is_empty() {
            base_path.push('/');
        }
        
        let url = if scheme == "https" && port == 443 {
            format!("{}://{}{}", scheme, host, base_path)
        } else if scheme == "http" && port == 80 {
            format!("{}://{}{}", scheme, host, base_path)
        } else {
            format!("{}://{}:{}{}", scheme, host, port, base_path)
        };
        
        println!("  \x1b[33mURL:\x1b[0m      \x1b[1m\x1b[32m{}\x1b[0m", url);
        println!("  \x1b[33mUsername:\x1b[0m \x1b[1m\x1b[32m{}\x1b[0m", settings.admin_username);
        println!("  \x1b[33mPassword:\x1b[0m \x1b[1m\x1b[32m{}\x1b[0m\n", settings.admin_password);
    } else {
        println!("\x1b[31mFailed to load settings from DB.\x1b[0m");
    }
}

async fn fetch_status(api_url: &str) -> Option<Vec<RouteStatus>> {
    let client = HTTP_CLIENT.clone();
    if let Ok(resp) = client.get(&format!("{}/status", api_url)).send().await {
        if let Ok(stats) = resp.json::<Vec<RouteStatus>>().await {
            return Some(stats);
        }
    }
    None
}

async fn fetch_status_with_session(api_url: &str, session: Option<&str>) -> Option<Vec<RouteStatus>> {
    if let Some(cookie) = session {
        let client = HTTP_CLIENT.clone();
        if let Ok(r) = client.get(&format!("{}/api/routes", api_url))
            .header(reqwest::header::COOKIE, cookie)
            .send().await
        {
            if let Ok(items) = r.json::<Vec<RouteApiItem>>().await {
                let mapped = items.into_iter().map(|it| RouteStatus {
                    name: it.name,
                    country: it.country_code,
                    bind_address: it.bind_address,
                    port: it.input_port,
                    latency: it.latency,
                    tor_ip: it.tor_ip,
                    last_checked_at: it.last_checked_at,
                }).collect();
                return Some(mapped);
            }
        }
    }
    fetch_status(api_url).await
}

fn format_age(ms_str: &Option<String>) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    if let Some(s) = ms_str {
        if let Ok(ms) = s.parse::<i64>() {
            let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
            let diff = now_ms - ms;
            if diff < 0 { return "0s".to_string(); }
            let secs = diff / 1000;
            if secs < 60 { return format!("{}s", secs); }
            let mins = secs / 60;
            if mins < 60 { return format!("{}m", mins); }
            let hours = mins / 60;
            return format!("{}h", hours);
        }
    }
    "-".to_string()
}

async fn view_live_status_loop(api_url: &str, session: Option<&str>) {
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    // Spawn a blocking thread that waits for ENTER to stop the live view
    thread::spawn(move || {
        let mut buf = String::new();
        let _ = std::io::stdin().read_line(&mut buf);
        stop_clone.store(true, Ordering::SeqCst);
    });

    // Print header once
    println!("\x1b[1m\x1b[36m╔════════════════════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1m\x1b[36m║               🚀 TOR ROUTER LIVE                      ║\x1b[0m");
    println!("\x1b[1m\x1b[36m╚════════════════════════════════════════════════════════╝\x1b[0m\n");
    println!("\n\x1b[1mNAME\t COUNTRY\t BIND\t PORT\t LATENCY\t TOR EXIT IP\t LAST CHECK\x1b[0m");

    let mut prev_count: usize = 0;
    while !stop.load(Ordering::SeqCst) {
        // Fetch status silently
        let stats_opt = fetch_status_with_session(api_url, session).await;
        if let Some(stats) = stats_opt {
            // Move cursor up to the first data row to overwrite previous values
            if prev_count > 0 {
                print!("\x1b[{}A", prev_count);
            }
            // Probe each route concurrently to get live latency only; use the DB-loaded tor_ip
            let mut handles = Vec::new();
            for s in &stats {
                let bind = s.bind_address.clone();
                let port = s.port;
                let api = api_url.to_string();
                handles.push(tokio::spawn(async move { probe_latency_for_route(&api, &bind, port).await }));
            }
            let mut results = Vec::new();
            for h in handles {
                match h.await {
                    Ok(res) => results.push(Some(res)),
                    Err(_) => results.push(None),
                }
            }
            let mut printed = 0usize;
            for (s, probe) in stats.iter().zip(results.iter()) {
                let lat_str = probe.clone().unwrap_or_else(|| s.latency.clone());
                let ip = s.tor_ip.clone();
                let mut lat_color = "\x1b[32m"; // Green
                if lat_str == "Connecting/Error" {
                    lat_color = "\x1b[31m";
                } else if lat_str.contains('s') && !lat_str.contains("ms") {
                    lat_color = "\x1b[33m";
                }
                let tor_ip = ip.unwrap_or_else(|| "-".to_string());
                let last = format_age(&s.last_checked_at);
                // Clear line and print
                print!("\x1b[2K");
                println!("{}\t {}\t {}\t {}\t {}{}\x1b[0m\t {}\t {}",
                    s.name, s.country, s.bind_address, s.port, lat_color, lat_str, tor_ip, last);
                printed += 1;
            }
            // If previous had more rows, clear the leftovers
            if prev_count > printed {
                for _ in 0..(prev_count - printed) {
                    print!("\x1b[2K\n");
                }
            }
            // Print the static prompt line and include it in prev_count
            print!("\x1b[2K\n");
            println!("Press ENTER to return to menu. Updating every 10s...");
            prev_count = printed + 1;
        } else {
            // If failed to fetch, move up to clear previous rows
            if prev_count > 0 { print!("\x1b[{}A", prev_count); }
            if prev_count > 0 {
                for _ in 0..prev_count { print!("\x1b[2K\n"); }
            }
            prev_count = 0;
            println!("\n\x1b[31m❌ Failed to fetch status.\x1b[0m");
            // Print prompt line
            println!("Press ENTER to return to menu. Updating every 10s...");
        }
        // Sleep for 10s or until stop requested
        for _ in 0..10 {
            if stop.load(Ordering::SeqCst) { break; }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
}

async fn probe_latency_for_route(api_url: &str, bind: &str, port: u16) -> String {
    let client = HTTP_CLIENT.clone();
    let url = format!("{}/probe?bind={}&port={}", api_url.trim_end_matches('/'), bind, port);
    if let Ok(resp) = client.get(&url).send().await {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(lat) = json.get("latency").and_then(|v| v.as_str()) {
                    return lat.to_string();
                }
            }
        }
    }
    "Connecting/Error".to_string()
}

async fn restart_route(api_url: &str) {
    if fetch_status(api_url).await.is_none() {
        return;
    }

    print!("✏️ Enter the NAME of the route to restart (or leave blank to cancel): ");
    io::stdout().flush().unwrap();
    let mut name = String::new();
    io::stdin().read_line(&mut name).unwrap();
    let name = name.trim();

    if name.is_empty() { return; }

    let client = HTTP_CLIENT.clone();
    let res = client.post(&format!("{}/restart?route={}", api_url, name)).send().await;

    match res {
        Ok(r) if r.status().is_success() => {
            println!("\n\x1b[32m✅ Successfully sent restart signal for [{}]!\x1b[0m", name);
        }
        _ => {
            println!("\n\x1b[31m⚠️ Failed to restart [{}]. Route might not exist.\x1b[0m", name);
        }
    }
}

async fn restart_all(api_url: &str) {
    if let Some(stats) = fetch_status(api_url).await {
        println!("\n\x1b[33m🔄 Restarting all routes...\x1b[0m");
        let client = HTTP_CLIENT.clone();
        for s in stats {
            if let Ok(r) = client.post(&format!("{}/restart?route={}", api_url, s.name)).send().await {
                if r.status().is_success() {
                    println!("✅ {} restarted.", s.name);
                }
            } else {
                println!("❌ Failed to restart {}", s.name);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        println!("\n\x1b[32m🎉 All available routes restarted!\x1b[0m");
    }
}

async fn change_ports_with_session(api_url: &str, session: Option<&str>) {
    println!("\n\x1b[33m🌐 Change Web Panel & API Ports\x1b[0m");
    println!("(Press ENTER to skip changing a port)\n");

    print!("👉 Enter new Port for Web Panel and API (default 3000): ");
    io::stdout().flush().unwrap();
    let mut port_input = String::new();
    io::stdin().read_line(&mut port_input).unwrap();
    let port_input = port_input.trim();
    let port: u16 = if port_input.is_empty() { 3000 } else { port_input.parse().unwrap_or(3000) };

    let client = HTTP_CLIENT.clone();
    let payload = serde_json::json!({
        "web_panel_port": port,
        "api_port": port
    });

    let mut req = client.put(&format!("{}/api/settings", api_url)).json(&payload);
    if let Some(cookie) = session { req = req.header(reqwest::header::COOKIE, cookie); }

    let res = req.send().await;

    match res {
        Ok(r) if r.status().is_success() => {
            println!("\n\x1b[32m✅ Successfully updated ports configured in DB!\x1b[0m");
            println!("(Restart the Node.js service for changes to take full effect)");
        }
        _ => {
            println!("\n\x1b[31m❌ Failed to update ports. Is the API online?\x1b[0m");
        }
    }
}


async fn create_route_cli(api_url: &str, session: Option<&str>) {
    print!("👉 Route name: "); io::stdout().flush().unwrap(); let mut name = String::new(); io::stdin().read_line(&mut name).unwrap(); let name = name.trim().to_string();
    if name.is_empty() { println!("Cancelled."); return; }
    print!("👉 Bind address (default 127.0.0.1): "); io::stdout().flush().unwrap(); let mut bind = String::new(); io::stdin().read_line(&mut bind).unwrap(); let bind = bind.trim();
    let bind = if bind.is_empty() { "127.0.0.1".to_string() } else { bind.to_string() };
    print!("👉 Input port: "); io::stdout().flush().unwrap(); let mut port_s = String::new(); io::stdin().read_line(&mut port_s).unwrap(); let port: u16 = port_s.trim().parse().unwrap_or(0);
    print!("👉 Username (optional): "); io::stdout().flush().unwrap(); let mut username = String::new(); io::stdin().read_line(&mut username).unwrap(); let username = username.trim().to_string();
    print!("👉 Password (optional): "); io::stdout().flush().unwrap(); let mut password = String::new(); io::stdin().read_line(&mut password).unwrap(); let password = password.trim().to_string();
    print!("👉 Country code (e.g. us): "); io::stdout().flush().unwrap(); let mut country = String::new(); io::stdin().read_line(&mut country).unwrap(); let country = country.trim().to_string();
    print!("👉 Swap interval minutes (default 1440): "); io::stdout().flush().unwrap(); let mut swap_s = String::new(); io::stdin().read_line(&mut swap_s).unwrap(); let swap = swap_s.trim().parse::<u64>().ok();
    print!("👉 Test interval minutes (default 15): "); io::stdout().flush().unwrap(); let mut test_s = String::new(); io::stdin().read_line(&mut test_s).unwrap(); let test = test_s.trim().parse::<u64>().ok();

    let payload = serde_json::json!({
        "name": name,
        "bind_address": bind,
        "input_port": port,
        "username": if username.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(username) },
        "password": if password.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(password) },
        "country_code": country,
        "swap_interval_minutes": swap,
        "test_interval_minutes": test,
    });

    let client = HTTP_CLIENT.clone();
    let mut req = client.post(&format!("{}/api/routes", api_url)).json(&payload);
    if let Some(cookie) = session { req = req.header(reqwest::header::COOKIE, cookie); }
    let res = req.send().await;

    match res {
        Ok(r) if r.status().is_success() => println!("\n\x1b[32m✅ Route created\x1b[0m"),
        Ok(r) => println!("\n\x1b[31m❌ Failed to create route: {}\x1b[0m", r.status()),
        Err(_) => println!("\n\x1b[31m❌ Request failed\x1b[0m"),
    }
}

async fn edit_route_cli(api_url: &str, session: Option<&str>) {
    let client = HTTP_CLIENT.clone();
    let mut req = client.get(&format!("{}/api/routes", api_url));
    if let Some(cookie) = session { req = req.header(reqwest::header::COOKIE, cookie); }
    let res = req.send().await;
    let items: Vec<RouteApiItem> = match res {
        Ok(r) => match r.json().await { Ok(v) => v, Err(_) => { println!("Failed to list routes"); return; } },
        Err(_) => { println!("Failed to contact API"); return; }
    };
    if items.is_empty() { println!("No routes"); return; }
    println!("\nAvailable routes:");
    for it in &items { println!("{}: {}", it.id, it.name); }
    print!("Enter ID to edit: "); io::stdout().flush().unwrap(); let mut id = String::new(); io::stdin().read_line(&mut id).unwrap(); let id = id.trim().to_string();
    if id.is_empty() { return; }
    let existing = items.into_iter().find(|i| i.id == id);
    if existing.is_none() { println!("Invalid ID"); return; }
    let ex = existing.unwrap();
    print!("Name ({}): ", ex.name); io::stdout().flush().unwrap(); let mut name = String::new(); io::stdin().read_line(&mut name).unwrap(); let name = if name.trim().is_empty() { ex.name } else { name.trim().to_string() };
    print!("Bind address ({}): ", ex.bind_address); io::stdout().flush().unwrap(); let mut bind = String::new(); io::stdin().read_line(&mut bind).unwrap(); let bind = if bind.trim().is_empty() { ex.bind_address } else { bind.trim().to_string() };
    print!("Input port ({}): ", ex.input_port); io::stdout().flush().unwrap(); let mut port_s = String::new(); io::stdin().read_line(&mut port_s).unwrap(); let port = if port_s.trim().is_empty() { ex.input_port } else { port_s.trim().parse().unwrap_or(ex.input_port) };
    print!("Username ({}): ", ex.username.clone().unwrap_or_default()); io::stdout().flush().unwrap(); let mut username = String::new(); io::stdin().read_line(&mut username).unwrap(); let username = if username.trim().is_empty() { ex.username } else { Some(username.trim().to_string()) };
    print!("Password (hidden): "); io::stdout().flush().unwrap(); let mut password = String::new(); io::stdin().read_line(&mut password).unwrap(); let password = if password.trim().is_empty() { ex.password } else { Some(password.trim().to_string()) };
    print!("Country code ({}): ", ex.country_code); io::stdout().flush().unwrap(); let mut country = String::new(); io::stdin().read_line(&mut country).unwrap(); let country = if country.trim().is_empty() { ex.country_code } else { country.trim().to_string() };
    print!("Swap interval minutes ({}): ", ex.swap_interval_minutes); io::stdout().flush().unwrap(); let mut swap_s = String::new(); io::stdin().read_line(&mut swap_s).unwrap(); let swap = if swap_s.trim().is_empty() { ex.swap_interval_minutes } else { swap_s.trim().parse().unwrap_or(ex.swap_interval_minutes) };
    print!("Test interval minutes ({}): ", ex.test_interval_minutes); io::stdout().flush().unwrap(); let mut test_s = String::new(); io::stdin().read_line(&mut test_s).unwrap(); let test = if test_s.trim().is_empty() { ex.test_interval_minutes } else { test_s.trim().parse().unwrap_or(ex.test_interval_minutes) };

    let payload = serde_json::json!({
        "name": name,
        "bind_address": bind,
        "input_port": port,
        "username": username,
        "password": password,
        "country_code": country,
        "swap_interval_minutes": swap,
        "test_interval_minutes": test,
    });

    let mut req = client.put(&format!("{}/api/routes/{}", api_url, id)).json(&payload);
    if let Some(cookie) = session { req = req.header(reqwest::header::COOKIE, cookie); }
    let res = req.send().await;
    match res {
        Ok(r) if r.status().is_success() => println!("\n\x1b[32m✅ Route updated\x1b[0m"),
        Ok(r) => println!("\n\x1b[31m❌ Update failed: {}\x1b[0m", r.status()),
        Err(_) => println!("\n\x1b[31m❌ Request failed\x1b[0m"),
    }
}

async fn delete_route_cli(api_url: &str, session: Option<&str>) {
    let client = HTTP_CLIENT.clone();
    let mut req = client.get(&format!("{}/api/routes", api_url));
    if let Some(cookie) = session { req = req.header(reqwest::header::COOKIE, cookie); }
    let res = req.send().await;
    let items: Vec<RouteApiItem> = match res {
        Ok(r) => match r.json().await { Ok(v) => v, Err(_) => { println!("Failed to list routes"); return; } },
        Err(_) => { println!("Failed to contact API"); return; }
    };
    if items.is_empty() { println!("No routes"); return; }
    println!("\nAvailable routes:");
    for it in &items { println!("{}: {}", it.id, it.name); }
    print!("Enter ID to delete: "); io::stdout().flush().unwrap(); let mut id = String::new(); io::stdin().read_line(&mut id).unwrap(); let id = id.trim().to_string();
    if id.is_empty() { return; }
    print!("Are you sure? Type DELETE to confirm: "); io::stdout().flush().unwrap(); let mut confirm = String::new(); io::stdin().read_line(&mut confirm).unwrap(); if confirm.trim() != "DELETE" { println!("Cancelled"); return; }
    let mut req = client.delete(&format!("{}/api/routes/{}", api_url, id));
    if let Some(cookie) = session { req = req.header(reqwest::header::COOKIE, cookie); }
    let res = req.send().await;
    match res {
        Ok(r) if r.status().is_success() => println!("\n\x1b[32m✅ Route deleted\x1b[0m"),
        Ok(r) => println!("\n\x1b[31m❌ Delete failed: {}\x1b[0m", r.status()),
        Err(_) => println!("\n\x1b[31m❌ Request failed\x1b[0m"),
    }
}

async fn update_admin_credentials(api_url: &str, session: Option<&str>) {
    print!("New admin username (leave blank to skip): "); io::stdout().flush().unwrap(); let mut user = String::new(); io::stdin().read_line(&mut user).unwrap(); let user = user.trim().to_string();
    print!("New admin password (leave blank to skip): "); io::stdout().flush().unwrap(); let mut pass = String::new(); io::stdin().read_line(&mut pass).unwrap(); let pass = pass.trim().to_string();
    print!("New Web Base Path (leave blank to skip, '-' to clear): "); io::stdout().flush().unwrap(); let mut base = String::new(); io::stdin().read_line(&mut base).unwrap(); 
    let base = base.trim();
    let base = base.trim_matches(|c| c == '\'' || c == '"').trim().to_string();
    if user.is_empty() && pass.is_empty() && base.is_empty() { println!("Nothing to do"); return; }
    let mut payload = serde_json::Map::new();
    if !user.is_empty() { payload.insert("admin_username".to_string(), serde_json::Value::String(user)); }
    if !pass.is_empty() { payload.insert("admin_password".to_string(), serde_json::Value::String(pass)); }
    if !base.is_empty() { 
        let final_base = if base == "-" { "".to_string() } else { base };
        payload.insert("web_base_path".to_string(), serde_json::Value::String(final_base)); 
    }
    let client = HTTP_CLIENT.clone();
    let mut req = client.put(&format!("{}/api/settings", api_url)).json(&payload);
    if let Some(cookie) = session { req = req.header(reqwest::header::COOKIE, cookie); }
    let res = req.send().await;
    match res {
        Ok(r) if r.status().is_success() => println!("\n\x1b[32m✅ Admin credentials updated\x1b[0m"),
        Ok(r) => println!("\n\x1b[31m❌ Update failed: {}\x1b[0m", r.status()),
        Err(_) => println!("\n\x1b[31m❌ Request failed\x1b[0m"),
    }
}


async fn start_service() {
    let exe_path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("/opt/ToRouter-Multi-Location/dist/ToRouter"));
    let exe_dir = exe_path.parent().unwrap_or(std::path::Path::new("/opt/ToRouter-Multi-Location/dist"));
    let project_dir = exe_dir.parent().unwrap_or(std::path::Path::new("/opt/ToRouter-Multi-Location"));
    let db_path = exe_dir.join("ToRouter.sqlite");

    let db_path_str = db_path.to_str().unwrap_or("ToRouter.sqlite");

    let mut settings = match crate::config::load_settings(db_path_str) {
        Ok(s) => s,
        Err(_) => crate::config::Settings::default(),
    };

    let mut current_port = settings.web_panel_port;
    let bind_addr = if settings.web_bind_address.is_empty() { "0.0.0.0".to_string() } else { settings.web_bind_address.clone() };
    
    loop {
        if std::net::TcpListener::bind((bind_addr.as_str(), current_port)).is_ok() {
            break;
        }
        
        println!("\n\x1b[31m⚠️  Port {} is currently in use by another application.\x1b[0m", current_port);
        print!("\x1b[36m👉 Please enter a new free port for the web panel [1024-65535]: \x1b[0m");
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        
        let mut new_port_str = String::new();
        std::io::stdin().read_line(&mut new_port_str).unwrap();
        
        if let Ok(p) = new_port_str.trim().parse::<u16>() {
            if p > 0 {
                current_port = p;
            } else {
                println!("\x1b[31m❌ Invalid port number.\x1b[0m");
            }
        } else {
            println!("\x1b[31m❌ Invalid input.\x1b[0m");
        }
    }
    
    if current_port != settings.web_panel_port || crate::config::load_settings(db_path_str).is_err() {
        settings.web_panel_port = current_port;
        settings.api_port = current_port;
        let _ = crate::config::init_db(db_path_str);
        let _ = crate::config::save_settings(db_path_str, &settings);
        println!("\x1b[32m✅ Port updated to {} in database.\x1b[0m", current_port);

        // Allow port in UFW
        if let Ok(status) = std::process::Command::new("ufw").arg("status").output() {
            if String::from_utf8_lossy(&status.stdout).to_lowercase().contains("active") {
                let _ = std::process::Command::new("ufw")
                    .arg("allow")
                    .arg(format!("{}/tcp", current_port))
                    .status();
                println!("\x1b[32m✅ Port {} allowed in UFW firewall.\x1b[0m", current_port);
            }
        }
    }

    println!("\nCreating ToRouter.service...");
    
    let exe_str = exe_path.to_string_lossy();
    let project_dir_str = project_dir.to_string_lossy();
    let web_dir_str = exe_dir.join("web").to_string_lossy().to_string();

    let service_content = format!(r#"[Unit]
Description=ToRouter Multi-Location Tor Manager
Documentation=https://github.com/ArashAfkandeh/ToRouter-Multi-Location
After=network-online.target
Wants=network-online.target

[Service]
Type=simple

WorkingDirectory={project_dir_str}

ExecStart={exe_str} --web-dir {web_dir_str}

Restart=always
RestartSec=5

KillSignal=SIGINT
TimeoutStopSec=30

StandardOutput=journal
StandardError=journal
SyslogIdentifier=torouter

LimitNOFILE=65536

NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=full
ProtectHome=false

ReadWritePaths={project_dir_str}

[Install]
WantedBy=multi-user.target
"#);

    if let Err(e) = std::fs::write("/etc/systemd/system/ToRouter.service", service_content) {
        println!("\n\x1b[31m❌ Failed to write service file: {}. Try running with sudo.\x1b[0m", e);
        return;
    }

    println!("Reloading systemd daemon...");
    let _ = std::process::Command::new("systemctl").arg("daemon-reload").status();

    println!("Enabling ToRouter service...");
    let _ = std::process::Command::new("systemctl").arg("enable").arg("ToRouter.service").status();

    println!("Starting ToRouter service...");
    let status = std::process::Command::new("systemctl").arg("start").arg("ToRouter.service").status();
    
    match status {
        Ok(s) if s.success() => {
            println!("\n\x1b[32m✅ Service started successfully!\x1b[0m");
            display_panel_info().await;
        },
        _ => println!("\n\x1b[31m❌ Failed to start service.\x1b[0m"),
    }
}

async fn stop_service() {
    println!("Stopping ToRouter service...");
    let status = std::process::Command::new("systemctl").arg("stop").arg("ToRouter.service").status();
    
    match status {
        Ok(s) if s.success() => {
            println!("\n\x1b[32m✅ Service stopped successfully!\x1b[0m");
            println!("Disabling ToRouter service...");
            let _ = std::process::Command::new("systemctl").arg("disable").arg("ToRouter.service").status();
        },
        _ => println!("\n\x1b[31m❌ Failed to stop service.\x1b[0m"),
    }
}

async fn get_service_status() -> String {
    let output = std::process::Command::new("systemctl")
        .arg("is-active")
        .arg("ToRouter.service")
        .output();
    match output {
        Ok(out) => {
            let status = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if status == "active" {
                "\x1b[32mActive\x1b[0m".to_string()
            } else {
                "\x1b[31mInactive\x1b[0m".to_string()
            }
        }
        Err(_) => "\x1b[31mInactive\x1b[0m".to_string(),
    }
}

async fn restart_systemd_service() {
    println!("Restarting ToRouter service...");
    let status = std::process::Command::new("systemctl").arg("restart").arg("ToRouter.service").status();
    
    match status {
        Ok(s) if s.success() => println!("\n\x1b[32m✅ Service restarted successfully!\x1b[0m"),
        _ => println!("\n\x1b[31m❌ Failed to restart service.\x1b[0m"),
    }
}

async fn update_downgrade() {
    println!("\nFetching releases from GitHub...");
    let client = reqwest::Client::builder()
        .user_agent("ToRouter-CLI")
        .build()
        .unwrap();

    let url = "https://api.github.com/repos/ArashAfkandeh/ToRouter-Multi-Location/releases";
    let res = client.get(url).send().await;

    let releases: Vec<serde_json::Value> = match res {
        Ok(r) => r.json().await.unwrap_or_else(|_| vec![]),
        Err(_) => {
            println!("\x1b[31m❌ Failed to fetch releases from GitHub.\x1b[0m");
            return;
        }
    };

    if releases.is_empty() {
        println!("\x1b[31m❌ No releases found.\x1b[0m");
        return;
    }

    let limit = std::cmp::min(10, releases.len());
    let recent_releases = &releases[0..limit];

    println!("\n\x1b[1m\x1b[36mAvailable Releases:\x1b[0m");
    for (i, release) in recent_releases.iter().enumerate() {
        let tag_name = release["tag_name"].as_str().unwrap_or("Unknown");
        let name = release["name"].as_str().unwrap_or("Unknown");
        println!("  \x1b[36m{}.\x1b[0m {} - {}", i + 1, tag_name, name);
    }
    println!("  \x1b[36m0.\x1b[0m Cancel");

    print!("\n👉 Select a release to install (0-{}): ", limit);
    use std::io::Write;
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let choice: usize = match input.trim().parse() {
        Ok(n) => n,
        Err(_) => {
            println!("Invalid selection.");
            return;
        }
    };

    if choice == 0 || choice > limit {
        println!("Cancelled.");
        return;
    }

    let selected_release = &recent_releases[choice - 1];
    let tag_name = selected_release["tag_name"].as_str().unwrap_or("Unknown");
    
    // Find tar.gz asset
    let assets = selected_release["assets"].as_array().unwrap();
    let mut download_url = String::new();
    for asset in assets {
        if let Some(name) = asset["name"].as_str() {
            if name.ends_with(".tar.gz") {
                download_url = asset["browser_download_url"].as_str().unwrap_or("").to_string();
                break;
            }
        }
    }

    if download_url.is_empty() {
        println!("\x1b[31m❌ Could not find a .tar.gz asset for release {}.\x1b[0m", tag_name);
        return;
    }

    println!("\nDownloading {}...", tag_name);
    let tmp_file = "/tmp/torouter_update.tar.gz";
    
    // 1. Download
    let status = std::process::Command::new("curl")
        .arg("-L")
        .arg("-o")
        .arg(tmp_file)
        .arg(&download_url)
        .status();

    if !status.map(|s| s.success()).unwrap_or(false) {
        println!("\x1b[31m❌ Failed to download the release.\x1b[0m");
        return;
    }

    // 2. Stop service
    println!("Stopping ToRouter service...");
    let _ = std::process::Command::new("systemctl").arg("stop").arg("ToRouter.service").status();

    // 3. Backup DB
    println!("Backing up database files...");
    let exe_path = std::env::current_exe().unwrap_or_default();
    let current_dir = exe_path.parent().unwrap_or(std::path::Path::new("."));
    
    let db_files = ["ToRouter.sqlite", "ToRouter.sqlite-shm", "ToRouter.sqlite-wal"];
    for file in &db_files {
        let src = current_dir.join(file);
        let dst = format!("/tmp/{}", file);
        if src.exists() {
            let _ = std::fs::copy(&src, &dst);
        }
    }

    // 4. Extract
    let parent_of_project = current_dir.parent().and_then(|p| p.parent()).unwrap_or(std::path::Path::new("/opt"));
    println!("Extracting to {} ...", parent_of_project.display());
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(tmp_file)
        .arg("-C")
        .arg(parent_of_project)
        .arg("--unlink-first")
        .status();

    if !status.map(|s| s.success()).unwrap_or(false) {
        println!("\x1b[31m❌ Failed to extract the release.\x1b[0m");
    }

    // 5. Restore DB
    println!("Restoring database files...");
    for file in &db_files {
        let src = format!("/tmp/{}", file);
        let dst = current_dir.join(file);
        if std::path::Path::new(&src).exists() {
            let _ = std::fs::copy(&src, &dst);
            let _ = std::fs::remove_file(&src); // cleanup backup
        }
    }

    // 6. Start service
    println!("Starting ToRouter service...");
    let status = std::process::Command::new("systemctl").arg("start").arg("ToRouter.service").status();
    if !status.map(|s| s.success()).unwrap_or(false) {
        println!("\x1b[31m⚠️ Service might not have started successfully.\x1b[0m");
    }

    // 7. Cleanup tmp file
    let _ = std::fs::remove_file(tmp_file);

    println!("\n\x1b[32m✅ Upgrade/Downgrade to {} completed successfully!\x1b[0m", tag_name);
}

async fn uninstall_app() -> bool {
    print!("\nAre you sure you want to uninstall ToRouter-Multi-Location? [y/N]: ");
    use std::io::Write;
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let choice = input.trim().to_lowercase();

    if choice != "y" && choice != "yes" {
        println!("Uninstall cancelled.");
        return false;
    }

    println!("\n\x1b[31mUninstalling ToRouter-Multi-Location...\x1b[0m");

    // 1. Stop the service
    println!("Stopping ToRouter service...");
    let _ = std::process::Command::new("systemctl").arg("stop").arg("ToRouter.service").status();
    let _ = std::process::Command::new("systemctl").arg("disable").arg("ToRouter.service").status();
    let _ = std::fs::remove_file("/etc/systemd/system/ToRouter.service");
    let _ = std::process::Command::new("systemctl").arg("daemon-reload").status();

    // 2. Remove application files
    println!("Removing application files...");
    let exe_path = std::env::current_exe().unwrap_or_default();
    if let Some(exe_dir) = exe_path.parent() {
        if let Some(project_dir) = exe_dir.parent() {
            let p_str = project_dir.to_string_lossy();
            if p_str.contains("ToRouter") || p_str.contains("torouter") {
                let _ = std::fs::remove_dir_all(project_dir);
            } else {
                println!("\x1b[31m⚠️ Safety check failed: project directory {} doesn't seem to be ToRouter.\x1b[0m", p_str);
            }
        }
    }

    // 3. Remove symlink
    println!("Removing symlink...");
    let _ = std::fs::remove_file("/usr/local/bin/tor-p");

    println!("\n\x1b[32mUninstall completed successfully!\x1b[0m");
    println!("If you wish to reinstall, you can run the following command:\n");
    println!("\x1b[32mcurl -s https://raw.githubusercontent.com/ArashAfkandeh/ToRouter-Multi-Location/main/install.sh | sudo bash\x1b[0m\n");

    true // Return true to signal the loop to exit
}
