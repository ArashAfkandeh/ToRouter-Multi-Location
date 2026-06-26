use parking_lot::RwLock;
use std::sync::Arc;
use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, error};

use crate::config::RouteConfig;

#[derive(Clone)]
pub struct Backend {
    pub socks: String,
}

#[derive(Clone)]
pub struct Slot {
    pub active: Option<Backend>,
    pub draining: Option<Backend>,
}

pub async fn handle_socks5_auth(
    client: &mut TcpStream,
    expected_user: &str,
    expected_pass: &str,
) -> bool {
    let mut buf = [0u8; 512];
    
    // Read greeting
    if client.read_exact(&mut buf[0..2]).await.is_err() { return false; }
    if buf[0] != 0x05 { return false; }
    let nmethods = buf[1] as usize;
    if client.read_exact(&mut buf[0..nmethods]).await.is_err() { return false; }
    
    // Check if auth is configured
    let require_auth = !expected_user.is_empty() || !expected_pass.is_empty();

    if require_auth {
        if !buf[0..nmethods].contains(&0x02) {
            let _ = client.write_all(&[0x05, 0xFF]).await; // No acceptable methods
            return false;
        }
        
        // Reply with 0x02 (Username/Password)
        if client.write_all(&[0x05, 0x02]).await.is_err() { return false; }
        
        // Read auth request
        if client.read_exact(&mut buf[0..2]).await.is_err() { return false; }
        if buf[0] != 0x01 { return false; } // Auth version must be 1
        let ulen = buf[1] as usize;
        if client.read_exact(&mut buf[0..ulen]).await.is_err() { return false; }
        let uname = String::from_utf8_lossy(&buf[0..ulen]).to_string();
        
        if client.read_exact(&mut buf[0..1]).await.is_err() { return false; }
        let plen = buf[0] as usize;
        if client.read_exact(&mut buf[0..plen]).await.is_err() { return false; }
        let pass = String::from_utf8_lossy(&buf[0..plen]).to_string();
        
        if uname == expected_user && pass == expected_pass {
            // Success
            if client.write_all(&[0x01, 0x00]).await.is_err() { return false; }
        } else {
            // Failure
            let _ = client.write_all(&[0x01, 0x01]).await;
            return false;
        }
    } else {
        // No auth required, reply with 0x00 (No authentication required)
        if !buf[0..nmethods].contains(&0x00) {
            let _ = client.write_all(&[0x05, 0xFF]).await;
            return false;
        }
        if client.write_all(&[0x05, 0x00]).await.is_err() { return false; }
    }
    
    true
}

pub async fn fake_tor_handshake(tor: &mut TcpStream) -> bool {
    // Send NO AUTH greeting to Tor
    if tor.write_all(&[0x05, 0x01, 0x00]).await.is_err() { return false; }
    
    let mut buf = [0u8; 2];
    if tor.read_exact(&mut buf).await.is_err() { return false; }
    if buf[0] != 0x05 || buf[1] != 0x00 { return false; }
    
    true
}

pub async fn start_router_listener(
    bind_address: String,
    port: u16,
    slot: Arc<RwLock<Slot>>,
    config: Arc<RwLock<RouteConfig>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let listener_res = TcpListener::bind((bind_address.clone(), port)).await;
        let listener = match listener_res {
            Ok(l) => l,
            Err(e) => {
                error!("❌ Router failed to bind to {}:{}: {}", bind_address, port, e);
                return;
            }
        };

        info!("✅ Router listening on {}:{}", bind_address, port);

        loop {
            if let Ok((mut client, _)) = listener.accept().await {
                let active_backend = {
                    let s = slot.read();
                    s.active.clone()
                };

                let route_config = config.read().clone();
                let expected_user = route_config.username.clone().unwrap_or_default();
                let expected_pass = route_config.password.clone().unwrap_or_default();

                if let Some(backend) = active_backend {
                    tokio::spawn(async move {
                        if !handle_socks5_auth(&mut client, &expected_user, &expected_pass).await {
                            return;
                        }

                        if let Ok(mut server) = TcpStream::connect(&backend.socks).await {
                            if !fake_tor_handshake(&mut server).await {
                                return;
                            }
                            let _ = copy_bidirectional(&mut client, &mut server).await;
                        }
                    });
                }
            }
        }
    })
}

