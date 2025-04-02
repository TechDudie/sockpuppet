use regex::Regex;
use std::convert::Infallible;
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

    println!(
        "[sockpuppet] [{}] [{}] {}",
        formatted_time, level, message
    );
}

fn is_valid_target(target: &str) -> bool {
    let re = Regex::new(r"^(?:\d{1,3}\.){3}\d{1,3}:\d{4,5}$").unwrap();
    return re.is_match(target)
}

async fn handle_proxy(client_stream: &mut TcpStream) -> std::io::Result<TcpStream> {
    let mut buf = [0u8; 1024];

    // Read client's greeting
    client_stream.read(&mut buf).await?;
    if buf[0] != 0x05 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Not SOCKS5"));
    }

    // Send authentication method (0x05 = SOCKS5, 0x00 = No authentication)
    client_stream.write_all(&[0x05, 0x00]).await?;

    // Read client request
    client_stream.read(&mut buf).await?;
    if buf[1] != 0x01 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Only TCP connect supported"));
    }

    // Parse target address
    let addr = match buf[3] {
        0x01 => {
            // IPv4
            let ip = format!("{}.{}.{}.{}", buf[4], buf[5], buf[6], buf[7]);
            let port = u16::from_be_bytes([buf[8], buf[9]]);
            format!("{}:{}", ip, port)
        }
        0x03 => {
            // Domain name
            let len = buf[4] as usize;
            let domain = String::from_utf8_lossy(&buf[5..5 + len]).to_string();
            let port = u16::from_be_bytes([buf[5 + len], buf[6 + len]]);
            format!("{}:{}", domain, port)
        }
        _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid address type")),
    };

    // Connect to the requested address
    let server_stream = TcpStream::connect(&addr).await?;

    // Send success response
    let response = [0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]; // Success with 0.0.0.0:0
    client_stream.write_all(&response).await?;

    Ok(server_stream)
}

async fn handle_connection(mut client_stream: TcpStream, _state: ProxyState) -> std::io::Result<()> {
    // Perform SOCKS5 handshake and obtain the destination server stream
    let mut server_stream = handle_proxy(&mut client_stream).await?;

    log("SOCKS5 connection established", "INFO");

    let (mut client_read, mut client_write) = client_stream.split();
    let (mut server_read, mut server_write) = server_stream.split();

    tokio::try_join!(
        copy(&mut client_read, &mut server_write),
        copy(&mut server_read, &mut client_write)
    )?;

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
                log(
                    &format!(
                        "Received command to switch to invalid server address: {}",
                        new_target
                    ),
                    "WARN",
                );
                return Ok::<_, Infallible>(warp::reply::with_status(
                    "Invalid address".to_string(),
                    warp::http::StatusCode::BAD_REQUEST,
                ));
            }

            let mut addr = state.target_addr.lock().unwrap();
            *addr = new_target.clone();
            log(&format!("Proxy server set to: {}", new_target), "INFO");

            Ok::<_, Infallible>(warp::reply::with_status(
                "Proxy updated".to_string(),
                warp::http::StatusCode::OK,
            ))
        });

    warp::serve(set_target).run(([127, 0, 0, 1], 7070)).await;
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let state = ProxyState {
        target_addr: Arc::new(Mutex::new("127.0.0.1:6868".to_string()))
    };

    let api_state = state.clone();
    tokio::spawn(run_api(api_state));
    return run_proxy(state).await
}