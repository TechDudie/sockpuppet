use regex::Regex;
use std::sync::{Arc, Mutex};
use tokio::io::{copy, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use warp::Filter;

#[derive(Clone)]
struct ProxyState {
    target_addr: Arc<Mutex<String>>,
}

fn log(message: &str, level: &str) {
    let formatted_time = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
    println!("[sockpuppet] [{}] [{}] {}", formatted_time, level, message);
}

fn is_valid_target(target: &str) -> bool {
    let re = Regex::new(r"^(?:\d{1,3}\.){3}\d{1,3}:\d{4,5}$").unwrap();
    re.is_match(target)
}

async fn handle_proxy(client_stream: &mut TcpStream, state: ProxyState) -> std::io::Result<TcpStream> {
    let mut buf = [0u8; 1024];
    
    client_stream.read_exact(&mut buf[..2]).await?;
    if buf[0] != 0x05 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Not SOCKS5"));
    }
    
    client_stream.write_all(&[0x05, 0x00]).await?;
    
    client_stream.read_exact(&mut buf[..4]).await?;
    if buf[1] != 0x01 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Only TCP connect supported"));
    }
    
    let addr = match buf[3] {
        0x01 => {
            client_stream.read_exact(&mut buf[..6]).await?;
            let ip = format!("{}.{}.{}.{}", buf[0], buf[1], buf[2], buf[3]);
            let port = u16::from_be_bytes([buf[4], buf[5]]);
            format!("{}:{}", ip, port)
        }
        0x03 => {
            client_stream.read_exact(&mut buf[..1]).await?;
            let len = buf[0] as usize;
            client_stream.read_exact(&mut buf[..len + 2]).await?;
            let domain = String::from_utf8_lossy(&buf[..len]).to_string();
            let port = u16::from_be_bytes([buf[len], buf[len + 1]]);
            format!("{}:{}", domain, port)
        }
        _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid address type")),
    };
    
    let target_addr = state.target_addr.lock().unwrap().clone();
    let mut proxy_stream = TcpStream::connect(target_addr).await?;
    client_stream.write_all(&[0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0, 0]).await?;
    
    Ok(proxy_stream)
}

async fn handle_connection(mut client_stream: TcpStream, state: ProxyState) -> std::io::Result<()> {
    match handle_proxy(&mut client_stream, state).await {
        Ok(mut server_stream) => {
            let (mut client_read, mut client_write) = client_stream.split();
            let (mut server_read, mut server_write) = server_stream.split();
            
            let transfer = tokio::try_join!(
                copy(&mut client_read, &mut server_write),
                copy(&mut server_read, &mut client_write)
            );
            
            if let Err(e) = transfer {
                log(&format!("Error during data transfer: {}", e), "ERROR");
            }
        }
        Err(e) => {
            log(&format!("SOCKS5 handshake failed: {}", e), "ERROR");
        }
    }
    Ok(())
}

async fn run_proxy(state: ProxyState) -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:6969").await?;
    log("SOCKS5 Proxy running on 127.0.0.1:6969", "INFO");
    
    loop {
        let (client_stream, _) = listener.accept().await?;
        let state = state.clone();
        
        tokio::spawn(async move {
            if let Err(e) = handle_connection(client_stream, state).await {
                log(&format!("Error handling connection: {}", e), "ERROR");
            }
        });
    }
}

async fn run_api(state: ProxyState) {
    let state_filter = warp::any().map(move || state.clone());
    let set_target = warp::path!("set_proxy" / String)
        .and(state_filter)
        .and_then(|new_target: String, state: ProxyState| async move {
            if !is_valid_target(&new_target) {
                log(&format!("Received invalid proxy address: {}", new_target), "WARN");
                return Ok::<_, warp::Rejection>(warp::reply::with_status("Invalid address", warp::http::StatusCode::BAD_REQUEST));
            }
            let mut addr = state.target_addr.lock().unwrap();
            *addr = new_target.clone();
            log(&format!("Proxy server set to: {}", new_target), "INFO");
            Ok::<_, warp::Rejection>(warp::reply::with_status("Proxy updated", warp::http::StatusCode::OK))
        });
    warp::serve(set_target).run(([127, 0, 0, 1], 7070)).await;
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let state = ProxyState {
        target_addr: Arc::new(Mutex::new("127.0.0.1:31337".to_string())),
    };
    
    let api_state = state.clone();
    tokio::spawn(run_api(api_state));
    run_proxy(state).await
}
