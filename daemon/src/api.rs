use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tower_http::services::ServeDir;

use crate::config::{self, RouteConfig, SettingsUpdate};
use crate::daemon::{ActiveNode, SharedNodes, NOT_CONNECTED};

// ─── Shared state ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub restart_tx: mpsc::Sender<String>,
    pub nodes: SharedNodes,
    pub db_path: String,
    // In-memory session tokens (reset on daemon restart – intentional)
    pub sessions: Arc<RwLock<HashSet<String>>>,
}

// ─── Wire up the server ──────────────────────────────────────────────────────

pub async fn start_web_server(
    bind_addr: String,
    restart_tx: mpsc::Sender<String>,
    nodes: SharedNodes,
    db_path: String,
    web_dir: Option<String>,
) {
    let state = AppState {
        restart_tx,
        nodes,
        db_path,
        sessions: Arc::new(RwLock::new(HashSet::new())),
    };

    // ── API routes ──────────────────────────────────────────────────────────
    // NOTE: axum matches static segments before parameterised ones, so
    // /api/routes/restart-all will win over /api/routes/:id.
    let api = Router::new()
        .route("/api/login",               post(login))
        .route("/api/routes",              get(list_routes).post(create_route_handler))
        .route("/api/routes/restart-all",  post(restart_all_handler))
        .route("/api/routes/:id/restart",  post(restart_by_id_handler))
        .route("/api/routes/:id",          put(update_route_handler).delete(delete_route_handler))
        .route("/api/settings",            get(get_settings_handler).put(save_settings_handler))
        // Legacy CLI endpoint – keep backward-compat
        .route("/restart",                 post(legacy_restart))
        .route("/status",                  get(legacy_status))
        .with_state(state);

    // ── Static files (web panel) ────────────────────────────────────────────
    let app = if let Some(ref dir) = web_dir {
        // Serve the built React app; fall back to index.html for SPA routing
        let serve = ServeDir::new(&dir)
            .fallback(tower_http::services::ServeFile::new(format!("{}/index.html", dir)));
        api.fallback_service(serve)
    } else {
        api.fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                "Web panel not configured. Start the daemon with --web-dir <path/to/dist>",
            )
        })
    };

    let addr: std::net::SocketAddr = match bind_addr.parse() {
        Ok(a) => a,
        Err(e) => { eprintln!("❌ Invalid bind address {}: {}", bind_addr, e); return; }
    };

    if web_dir.is_some() {
        println!("🌐 Web panel listening on http://{}", addr);
    }
    if let Err(e) = axum::Server::bind(&addr).serve(app.into_make_service()).await {
        eprintln!("❌ Web server error: {}", e);
    }
}

// ─── Session helpers ─────────────────────────────────────────────────────────

fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:x}{:x}", ns, std::process::id())
}

fn extract_session(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';')
        .find_map(|part| {
            let part = part.trim();
            part.strip_prefix("session=").map(|t| t.to_string())
        })
}

fn require_auth(state: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, &'static str)> {
    let token = extract_session(headers)
        .ok_or((StatusCode::UNAUTHORIZED, "Not authenticated"))?;
    if state.sessions.read().contains(&token) {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "Invalid session"))
    }
}

// ─── Auth endpoint ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LoginRequest { username: String, password: String }

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    let settings = config::load_settings(&state.db_path)
        .unwrap_or_default();

    if body.username == settings.admin_username && body.password == settings.admin_password {
        let token = generate_token();
        state.sessions.write().insert(token.clone());
        let cookie = format!("session={}; Path=/; HttpOnly; SameSite=Strict", token);
        (
            StatusCode::OK,
            [(header::SET_COOKIE, cookie)],
            Json(serde_json::json!({ "ok": true })),
        ).into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(header::SET_COOKIE, String::new())],
            Json(serde_json::json!({ "error": "Invalid credentials" })),
        ).into_response()
    }
}

// ─── Route status (the JSON the web panel displays) ──────────────────────────

#[derive(Serialize)]
struct RouteStatusResponse {
    id: String,
    name: String,
    bind_address: String,
    input_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    country_code: String,
    swap_interval_hours: u64,
    test_interval_minutes: u64,
    latency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tor_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_checked_at: Option<String>,
    status: &'static str,
}

fn latency_to_status(lat: Duration) -> &'static str {
    if lat >= NOT_CONNECTED { "error" }
    else if lat >= Duration::from_millis(500) { "warning" }
    else { "healthy" }
}

fn latency_to_string(lat: Duration) -> String {
    if lat >= NOT_CONNECTED {
        "Connecting/Error".to_string()
    } else if lat.as_millis() > 0 {
        format!("{}ms", lat.as_millis())
    } else {
        "Pending".to_string()
    }
}

fn node_to_response(cfg: &RouteConfig, node: Option<&Arc<ActiveNode>>) -> RouteStatusResponse {
    let (lat, tor_ip, last_checked_at) = match node {
        Some(n) => (
            *n.latency.read(),
            n.tor_ip.read().clone(),
            n.last_checked_at.read().clone(),
        ),
        None => (
            NOT_CONNECTED,
            cfg.tor_ip.clone(),
            cfg.last_checked_at.clone(),
        ),
    };
    RouteStatusResponse {
        id:                   cfg.id.to_string(),
        name:                 cfg.name.clone(),
        bind_address:         cfg.bind_address.clone().unwrap_or_else(|| "127.0.0.0".to_string()),
        input_port:           cfg.input_port,
        username:             cfg.username.clone(),
        password:             cfg.password.clone(),
        country_code:         cfg.country_code.to_uppercase(),
        swap_interval_hours:  cfg.swap_interval_hours.unwrap_or(24),
        test_interval_minutes:cfg.test_interval_minutes.unwrap_or(15),
        latency:              latency_to_string(lat),
        tor_ip,
        last_checked_at,
        status:               latency_to_status(lat),
    }
}

// ─── Route CRUD handlers ─────────────────────────────────────────────────────

async fn list_routes(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) { return e.into_response(); }

    let cfg = match config::load_from_db(&state.db_path) {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let nodes = state.nodes.read();
    let list: Vec<RouteStatusResponse> = cfg.routes.iter()
        .map(|r| node_to_response(r, nodes.get(&r.name).map(|a| a)))
        .collect();
    Json(list).into_response()
}

// Body the panel sends when creating/editing a route
#[derive(Deserialize)]
struct RouteBody {
    name: String,
    bind_address: Option<String>,
    input_port: u16,
    username: Option<String>,
    password: Option<String>,
    country_code: String,
    swap_interval_hours: Option<u64>,
    test_interval_minutes: Option<u64>,
}

impl From<RouteBody> for RouteConfig {
    fn from(b: RouteBody) -> Self {
        RouteConfig {
            id: 0,
            name: b.name,
            bind_address: b.bind_address.or_else(|| Some("127.0.0.1".to_string())),
            input_port: b.input_port,
            username: b.username.filter(|s| !s.is_empty()),
            password: b.password.filter(|s| !s.is_empty()),
            country_code: b.country_code.to_lowercase(),
            swap_interval_hours: Some(b.swap_interval_hours.unwrap_or(24)),
            test_interval_minutes: Some(b.test_interval_minutes.unwrap_or(15)),
            restart_trigger: None,
            tor_ip: None,
            last_checked_at: None,
        }
    }
}

async fn create_route_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RouteBody>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) { return e.into_response(); }
    let route: RouteConfig = body.into();
    match config::create_route(&state.db_path, &route) {
        Ok(id) => Json(serde_json::json!({ "id": id.to_string(), "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn update_route_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Json(body): Json<RouteBody>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) { return e.into_response(); }
    let id: i64 = match id_str.parse() {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid ID").into_response(),
    };

    // Find the old name so we can trigger a restart if it's running
    let old_name = config::get_route_by_id(&state.db_path, id)
        .ok().map(|r| r.name);

    let route: RouteConfig = body.into();
    if let Err(e) = config::update_route(&state.db_path, id, &route) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
    }

    // If the route was running under its old name, signal a restart so the
    // daemon's config-reload diff picks it up immediately.
    if let Some(name) = old_name {
        let _ = state.restart_tx.try_send(name);
    }

    Json(serde_json::json!({ "ok": true })).into_response()
}

async fn delete_route_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) { return e.into_response(); }
    let id: i64 = match id_str.parse() {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid ID").into_response(),
    };

    // Signal stop before deleting so the running process is killed cleanly
    if let Ok(route) = config::get_route_by_id(&state.db_path, id) {
        let _ = state.restart_tx.try_send(route.name);
    }

    match config::delete_route(&state.db_path, id) {
        Ok(_) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ─── Restart handlers ────────────────────────────────────────────────────────

async fn restart_by_id_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) { return e.into_response(); }
    let id: i64 = match id_str.parse() {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid ID").into_response(),
    };
    match config::get_route_by_id(&state.db_path, id) {
        Ok(route) => {
            let _ = state.restart_tx.send(route.name.clone()).await;
            Json(serde_json::json!({ "ok": true, "name": route.name })).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "Route not found").into_response(),
    }
}

async fn restart_all_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) { return e.into_response(); }
    let names: Vec<String> = state.nodes.read().keys().cloned().collect();
    let count = names.len();
    for name in names {
        let _ = state.restart_tx.send(name).await;
    }
    Json(serde_json::json!({ "ok": true, "restarted": count })).into_response()
}

// ─── Settings handlers ───────────────────────────────────────────────────────

async fn get_settings_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) { return e.into_response(); }
    match config::load_settings(&state.db_path) {
        Ok(s) => Json(serde_json::json!({
            "web_panel_port":   s.web_panel_port,
            "web_bind_address": s.web_bind_address,
            "api_port":         s.api_port,
        })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn save_settings_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(update): Json<SettingsUpdate>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) { return e.into_response(); }
    let mut settings = config::load_settings(&state.db_path).unwrap_or_default();
    if let Some(p) = update.web_panel_port   { settings.web_panel_port   = p; }
    if let Some(a) = update.web_bind_address { settings.web_bind_address = a; }
    if let Some(p) = update.api_port         { settings.api_port         = p; }
    if let Some(u) = update.admin_username   { settings.admin_username   = u; }
    if let Some(pw) = update.admin_password  { settings.admin_password   = pw; }
    match config::save_settings(&state.db_path, &settings) {
        Ok(_) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ─── Legacy CLI endpoints (backward compat) ──────────────────────────────────

#[derive(Deserialize)]
struct LegacyRestartQuery { route: String }

async fn legacy_restart(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<LegacyRestartQuery>,
) -> impl IntoResponse {
    if state.restart_tx.send(q.route.clone()).await.is_ok() {
        (StatusCode::OK, format!("Restart signal sent for {}\n", q.route))
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "System busy\n".to_string())
    }
}

async fn legacy_status(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = config::load_from_db(&state.db_path).unwrap_or(crate::config::Config { routes: vec![] });
    let nodes = state.nodes.read();
    let list: Vec<RouteStatusResponse> = cfg.routes.iter()
        .map(|r| node_to_response(r, nodes.get(&r.name).map(|a| a)))
        .collect();
    Json(list)
}
