mod request;
mod response;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use clap::Parser;
use rand::{seq::IteratorRandom, SeedableRng};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::RwLock,
};

/// Contains information parsed from the command-line invocation of balancebeam. The Clap macros
/// provide a fancy way to automatically construct a command-line argument parser.
#[derive(Parser, Debug)]
#[command(about = "Fun with load balancing")]
struct CmdOptions {
    /// "IP/port to bind to"
    #[arg(short, long, default_value = "0.0.0.0:1100")]
    bind: String,
    /// "Upstream host to forward requests to"
    #[arg(short, long)]
    upstream: Vec<String>,
    /// "Perform active health checks on this interval (in seconds)"
    #[arg(long, default_value = "10")]
    active_health_check_interval: usize,
    /// "Path to send request to for active health checks"
    #[arg(long, default_value = "/")]
    active_health_check_path: String,
    /// "Maximum number of requests to accept per IP per minute (0 = unlimited)"
    #[arg(long, default_value = "0")]
    max_requests_per_minute: usize,
}

/// Contains information about the state of balancebeam (e.g. what servers we are currently proxying
/// to, what servers have failed, rate limiting counts, etc.)
///
/// You should add fields to this struct in later milestones.
#[derive(Clone)]
struct ProxyState {
    /// How frequently we check whether upstream servers are alive (Milestone 4)
    #[allow(dead_code)]
    active_health_check_interval: usize,
    /// Where we should send requests when doing active health checks (Milestone 4)
    #[allow(dead_code)]
    active_health_check_path: String,
    /// Maximum number of requests an individual IP can make in a minute (Milestone 5)
    #[allow(dead_code)]
    max_requests_per_minute: usize,
    /// Addresses of servers that we are proxying to
    upstream_addresses: Vec<String>,
    /// living addresses record, read-write-lock has better performance, maybe
    living_upstream_addresses: Arc<RwLock<HashSet<String>>>,
    /// rate limiting counter
    rate_limiter: Arc<RwLock<HashMap<String, usize>>>,
}

#[tokio::main]
async fn main() {
    // Initialize the logging library. You can print log messages using the `log` macros:
    // https://docs.rs/log/0.4.8/log/ You are welcome to continue using print! statements; this
    // just looks a little prettier.
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "debug");
    }
    pretty_env_logger::init();

    // Parse the command line arguments passed to this program
    let options = CmdOptions::parse();
    if options.upstream.is_empty() {
        log::error!("At least one upstream server must be specified using the --upstream option.");
        std::process::exit(1);
    }

    // Start listening for connections
    let listener = match TcpListener::bind(&options.bind).await {
        Ok(listener) => listener,
        Err(err) => {
            log::error!("Could not bind to {}: {}", options.bind, err);
            std::process::exit(1);
        }
    };
    log::info!("Listening for requests on {}", options.bind);

    // Handle incoming connections
    let state = ProxyState {
        upstream_addresses: options.upstream.clone(),
        active_health_check_interval: options.active_health_check_interval,
        active_health_check_path: options.active_health_check_path,
        max_requests_per_minute: options.max_requests_per_minute,
        living_upstream_addresses: Arc::new(RwLock::new(options.upstream.into_iter().collect())),
        rate_limiter: Arc::new(RwLock::new(HashMap::new())),
    };

    // do active health check
    let stat = state.clone();
    tokio::spawn(async move {
        active_health_check(&stat).await;
    });

    // do rate limiting check
    let stat = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let mut limiter = stat.rate_limiter.write().await;
            limiter.clear();
        }
    });

    // Handle the connection!
    loop {
        if let Ok((stream, _)) = listener.accept().await {
            let state = state.clone();
            tokio::spawn(async move {
                handle_connection(stream, &state).await;
            });
        }
    }
}

/// simply using fixed window
async fn rate_limit_check(
    state: &ProxyState,
    client_conn: &mut TcpStream,
    client_ip: &String,
) -> Result<(), std::io::Error> {
    let mut rate = state.rate_limiter.write().await;
    let count = rate.entry(client_ip.to_string()).or_insert(0);
    *count += 1;
    if *count > state.max_requests_per_minute {
        let res = response::make_http_error(http::StatusCode::TOO_MANY_REQUESTS);
        if let Err(err) = response::write_to_stream(&res, client_conn).await {
            log::error!("Failed to response client {}: {}", client_ip, err)
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Too many requests",
        ));
    }
    Ok(())
}

async fn active_health_check(state: &ProxyState) {
    loop {
        tokio::time::sleep(Duration::new(state.active_health_check_interval as u64, 0)).await;

        for upstream_ip in &state.upstream_addresses {
            let request = http::Request::builder()
                .method(http::Method::GET)
                .uri(&state.active_health_check_path)
                .header("Host", upstream_ip)
                .body(Vec::new())
                .unwrap();

            match TcpStream::connect(upstream_ip).await {
                Ok(mut upstream) => {
                    if let Err(err) = request::write_to_stream(&request, &mut upstream).await {
                        log::error!("Failed to request upstream {}: {}", upstream_ip, err);
                        continue;
                    }

                    match response::read_from_stream(&mut upstream, request.method()).await {
                        Ok(response) => {
                            if response.status().as_u16() == 200 {
                                // If a failed upstream returns HTTP 200, put it back in the rotation of upstream servers.
                                let mut living = state.living_upstream_addresses.write().await;
                                if !living.contains(upstream_ip) {
                                    living.insert(upstream_ip.to_string());
                                }
                            } else {
                                //  If an online upstream returns a non-200 status code, mark that server as failed.
                                let mut living = state.living_upstream_addresses.write().await;
                                if living.contains(upstream_ip) {
                                    living.remove(upstream_ip);
                                }
                            }
                        }
                        Err(_) => {
                            //  If an online upstream fails to return a response, mark that server as failed.
                            log::error!("Failed to get response from the upstream {}", upstream_ip);
                            let mut living = state.living_upstream_addresses.write().await;
                            if living.contains(upstream_ip) {
                                living.remove(upstream_ip);
                            }
                        }
                    }
                }
                Err(err) => {
                    log::error!("Failed to connect to upstream {}: {}", upstream_ip, err);
                }
            }
        }
    }
}

async fn connect_to_upstream(state: &ProxyState) -> Result<TcpStream, std::io::Error> {
    loop {
        let living = state.living_upstream_addresses.read().await;
        let mut rng = rand::rngs::StdRng::from_entropy();
        let upstream_ip = &living.iter().choose(&mut rng).unwrap().clone();
        drop(living);

        match TcpStream::connect(upstream_ip).await {
            Ok(stream) => {
                return Ok(stream);
            }
            Err(err) => {
                log::error!("Failed to connect to upstream {}: {}", upstream_ip, err);

                let mut living = state.living_upstream_addresses.write().await;
                living.remove(upstream_ip);

                if living.is_empty() {
                    log::error!("Failed to connect upstream: all upstreams are dead");
                    return Err(err);
                }
            }
        }
    }
}

async fn send_response(client_conn: &mut TcpStream, response: &http::Response<Vec<u8>>) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!(
        "{} <- {}",
        client_ip,
        response::format_response_line(response)
    );
    if let Err(error) = response::write_to_stream(response, client_conn).await {
        log::warn!("Failed to send response to client: {}", error);
    }
}

async fn handle_connection(mut client_conn: TcpStream, state: &ProxyState) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("Connection received from {}", client_ip);

    // Open a connection to a random destination server
    let mut upstream_conn = match connect_to_upstream(state).await {
        Ok(stream) => stream,
        Err(_error) => {
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
    };
    let upstream_ip = upstream_conn.peer_addr().unwrap().ip().to_string();

    // The client may now send us one or more requests. Keep trying to read requests until the
    // client hangs up or we get an error.
    loop {
        // Read a request from the client
        let mut request = match request::read_from_stream(&mut client_conn).await {
            Ok(request) => request,
            // Handle case where client closed connection and is no longer sending requests
            Err(request::Error::IncompleteRequest(0)) => {
                log::debug!("Client finished sending requests. Shutting down connection");
                return;
            }
            // Handle I/O error in reading from the client
            Err(request::Error::ConnectionError(io_err)) => {
                log::info!("Error reading request from client stream: {}", io_err);
                return;
            }
            Err(error) => {
                log::debug!("Error parsing request: {:?}", error);
                let response = response::make_http_error(match error {
                    request::Error::IncompleteRequest(_)
                    | request::Error::MalformedRequest(_)
                    | request::Error::InvalidContentLength
                    | request::Error::ContentLengthMismatch => http::StatusCode::BAD_REQUEST,
                    request::Error::RequestBodyTooLarge => http::StatusCode::PAYLOAD_TOO_LARGE,
                    request::Error::ConnectionError(_) => http::StatusCode::SERVICE_UNAVAILABLE,
                });
                send_response(&mut client_conn, &response).await;
                continue;
            }
        };
        log::info!(
            "{} -> {}: {}",
            client_ip,
            upstream_ip,
            request::format_request_line(&request)
        );

        // check if too many request
        if state.max_requests_per_minute > 0 {
            if let Err(err) = rate_limit_check(state, &mut client_conn, &client_ip).await {
                log::error!("rate limit: {}", err);
                continue;
            }
        }

        // Add X-Forwarded-For header so that the upstream server knows the client's IP address.
        // (We're the ones connecting directly to the upstream server, so without this header, the
        // upstream server will only know our IP, not the client's.)
        request::extend_header_value(&mut request, "x-forwarded-for", &client_ip);

        // Forward the request to the server
        if let Err(error) = request::write_to_stream(&request, &mut upstream_conn).await {
            log::error!(
                "Failed to send request to upstream {}: {}",
                upstream_ip,
                error
            );
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
        log::debug!("Forwarded request to server");

        // Read the server's response
        let response = match response::read_from_stream(&mut upstream_conn, request.method()).await
        {
            Ok(response) => response,
            Err(error) => {
                log::error!("Error reading response from server: {:?}", error);
                let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
                send_response(&mut client_conn, &response).await;
                return;
            }
        };
        // Forward the response to the client
        send_response(&mut client_conn, &response).await;
        log::debug!("Forwarded response to client");
    }
}
