use regex::Regex;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use tokio::io::{copy, AsyncReadExt};
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

async fn run_proxy(state: ProxyState) -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:6969").await?;
    log("SOCKS5 Proxy running on 127.0.0.1:6969", "INFO");

    loop {
        let (mut client_stream, _) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(&mut client_stream, state).await {
                log(&format!("Error handling connection: {}", e), "ERROR");
            }
        });
    }
}

async fn handle_connection(client_stream: &mut TcpStream, state: ProxyState) -> std::io::Result<()> {
    let mut buf = [0u8; 1024];
    client_stream.read(&mut buf).await?;

    let target_addr = {
        let addr = state.target_addr.lock().unwrap();
        addr.clone()
    };

    let mut server_stream = TcpStream::connect(&target_addr).await?;
    log(&format!("Redirecting to: {}", target_addr), "INFO");

    let (mut client_read, mut client_write) = client_stream.split();
    let (mut server_read, mut server_write) = server_stream.split();

    tokio::try_join!(
        copy(&mut client_read, &mut server_write),
        copy(&mut server_read, &mut client_write)
    )?;

    return Ok(())
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