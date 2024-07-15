use std::{
    cell::Cell,
    collections::HashMap,
    fs::File,
    io::BufReader,
    net::SocketAddr,
    sync::Arc,
};

use axum::{
    body::Body,
    extract::{
        ws::{
            Message,
            WebSocket,
        },
        State,
    },
    http::{
        Request,
        StatusCode,
    },
    response::{
        IntoResponse,
        Json,
    },
    routing::{
        any,
        get,
    },
    Extension,
    Router,
};
use chrono::{DateTime, Utc};
use futures_util::{
    stream::SplitSink,
    SinkExt,
    StreamExt,
};
use rustls::{
    pki_types::CertificateDer,
    RootCertStore,
};
use serde_json::json;
use tokio::{
    net::TcpListener,
    sync::{
        Mutex,
        RwLock,
    },
};
use tokio_rustls::{
    rustls::pki_types::PrivateKeyDer,
    TlsAcceptor,
};
use tower::ServiceExt;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    trace::TraceLayer,
};

mod api;

#[derive(Clone)]
pub struct AppConfig {
    pub debug: bool,
}
#[derive(Debug)]
pub struct ConnectionData {
    pub last_pong: DateTime<Utc>,
    pub sink: Mutex<SplitSink<WebSocket, Message>>,
}
#[derive(Clone)]
pub struct AppData {
    pub connections: Arc<RwLock<HashMap<String, RwLock<HashMap<SocketAddr, ConnectionData>>>>>,
}
#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub data: AppData,
}

#[derive(Clone)]
pub struct TlsConnectInfo<'a> {
    pub addr: SocketAddr,
    pub client_cert: CertificateDer<'a>,
}

#[forbid(unsafe_code)]
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    if Ok("1".to_owned()) == std::env::var("APP_DOTENV") {
        dotenv::dotenv().unwrap();
    }

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_target(false)
        .with_ansi(false)
        .without_time()
        .init();

    let config = AppConfig { debug: true };
    let data = AppData {
        connections: Arc::new(RwLock::new(HashMap::<_, _>::new())),
    };

    let state = AppState { config, data };

    let mut ca_cert_reader = BufReader::new(File::open("../cert/gen/ca/ca.crt").unwrap());
    let ca_certs: Vec<tokio_rustls::rustls::pki_types::CertificateDer<'_>> =
        rustls_pemfile::certs(&mut ca_cert_reader).into_iter().filter_map(|cert| cert.ok()).collect();
    let mut root_cert_store = RootCertStore::empty();
    for c in ca_certs {
        root_cert_store.add(c).unwrap();
    }

    let mut server_cert_reader = BufReader::new(File::open("../cert/gen/server/server.crt").unwrap());
    let server_certs: Vec<rustls::pki_types::CertificateDer<'_>> =
        rustls_pemfile::certs(&mut server_cert_reader).into_iter().filter_map(|cert| cert.ok()).collect();
    let mut server_key_reader = BufReader::new(File::open("../cert/gen/server/server.key").unwrap());
    let server_keys: Vec<PrivateKeyDer<'_>> = rustls_pemfile::pkcs8_private_keys(&mut server_key_reader)
        .into_iter()
        .filter_map(|cert| cert.ok())
        .map(|cert| PrivateKeyDer::Pkcs8(cert))
        .collect();

    let config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(
            rustls::server::WebPkiClientVerifier::builder(Arc::new(root_cert_store)).build().unwrap(),
        )
        .with_single_cert(server_certs, server_keys[0].clone_key())
        .unwrap();

    let router = Router::new()
        .route("/ws", get(ws_handler).with_state(state.clone()))
        .route("/debug", any(crate::api::any_debug).with_state(state.clone()))
        .layer(CompressionLayer::new().gzip(true).deflate(true));
    let router = if state.config.debug {
        router.layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::AllowOrigin::mirror_request())
                .allow_credentials(true)
                .allow_methods(tower_http::cors::AllowMethods::mirror_request())
                .allow_headers(tower_http::cors::AllowHeaders::mirror_request()),
        )
    } else {
        router
    };
    let router = router.layer(
        TraceLayer::new_for_http().on_request(|r: &Request<Body>, _: &tracing::Span| {
            tracing::info!(message = format!("{:?}", r));
        }),
    );

    let listener = TcpListener::bind("0.0.0.0:8080").await.unwrap();
    let tls_acceptor = TlsAcceptor::from(Arc::new(config));
    let builder = hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());

    tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            dbg!(&state.data.connections.read().await);
        }
    });

    loop {
        let (stream, _) = listener.accept().await?;
        let acceptor = tls_acceptor.clone();
        let router = router.clone();
        let builder = builder.clone();

        tokio::task::spawn(async move {
            let tls_stream = acceptor.accept(stream).await.unwrap();
            let router = router.clone();
            let addr = tls_stream.get_ref().0.peer_addr().unwrap().clone();

            let client_cert = Cell::new(Option::<CertificateDer<'_>>::None);
            let certs = tls_stream.get_ref().1.peer_certificates().unwrap().to_vec();
            for cert in certs {
                let x509_cert = x509_parser::parse_x509_certificate(&cert).unwrap().1;
                if !x509_cert.is_ca() {
                    client_cert.set(Some(cert.clone()));
                }
            }

            let service = hyper::service::service_fn(move |mut req: Request<_>| {
                let router = router.clone();
                let ex = TlsConnectInfo {
                    addr,
                    // trust_anchor: ca_cert.take().unwrap(),
                    client_cert: client_cert.take().unwrap(),
                };
                req.extensions_mut().insert(Arc::new(ex));
                router.oneshot(req)
            });

            builder.serve_connection_with_upgrades(hyper_util::rt::TokioIo::new(tls_stream), service).await.unwrap();
        });
    }
}

async fn ws_handler(
    State(state): State<AppState>,
    Extension(conn_info): Extension<Arc<TlsConnectInfo<'_>>>,
    ws: axum::extract::WebSocketUpgrade,
) -> impl IntoResponse {
    let socket_address = conn_info.addr.clone();
    let client_cert_serial =
        x509_parser::parse_x509_certificate(&conn_info.client_cert).unwrap().1.raw_serial_as_string();
    ws.on_upgrade(move |socket| handle_socket(socket, client_cert_serial, socket_address, state))
}

async fn handle_socket<'a>(socket: WebSocket, client_cert_serial: String, addr: SocketAddr, state: AppState) {
    let (send, mut recv) = socket.split();

    {
        let mut lock = state.data.connections.write().await;
        let connections = lock.entry(client_cert_serial.clone()).or_insert_with(|| RwLock::new(HashMap::new()));
        connections.write().await.insert(addr, ConnectionData {
            last_pong: Utc::now(),
            sink: Mutex::new(send),
        });
    }

    tracing::info!(message = format!("{:?}", client_cert_serial));
    while let Some(msg) = recv.next().await {
        match msg {
            | Ok(Message::Text(text)) => {
                tracing::info!(message = format!("message received from {:?}: {:?}", addr, text));
                for (_, lock) in state.data.connections.read().await.iter() {
                    for (_, conn) in lock.read().await.iter() {
                        conn.sink.lock().await.send(Message::Text(text.clone())).await.unwrap();
                    }
                }
            },
            | Ok(Message::Ping(ping)) => {
                tracing::info!(message = format!("ping from: {:?}", addr));
                let lock = state.data.connections.read().await;
                lock.get(&client_cert_serial).unwrap().write().await.get_mut(&addr).unwrap().sink.lock().await.send(Message::Pong(ping)).await.unwrap();
            },
            | Ok(Message::Pong(_)) => {
                tracing::info!(message = format!("pong received from {:?}", addr));
            },
            | Ok(Message::Close(_)) => {
                tracing::info!(message = format!("connection closed: {:?}", addr));
                let mut lock = state.data.connections.write().await;
                let mut remove_after = false;
                {
                    let mut conns = lock.get_mut(&client_cert_serial).unwrap().write().await;
                    conns.remove(&addr);
                    if conns.is_empty() {
                        remove_after = true;
                    }
                }
                if remove_after {
                    lock.remove(&client_cert_serial);
                }

                break;
            },
            | Err(e) => {
                tracing::info!(message = format!("websocket error: {:?}", e));
                let mut lock = state.data.connections.write().await;
                let mut remove_after = false;
                {
                    let mut conns = lock.get_mut(&client_cert_serial).unwrap().write().await;
                    conns.remove(&addr);
                    if conns.is_empty() {
                        remove_after = true;
                    }
                }
                if remove_after {
                    lock.remove(&client_cert_serial);
                }
                break;
            },
            | _ => break,
        }
    }
}

pub enum ServerError {
    BadRequest(String),
    Unauthorized,
    Forbidden,
}
impl IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        match self {
            | Self::Unauthorized => {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({
                        "error": "unauthorized"
                    })),
                )
            },
            | Self::Forbidden => {
                (
                    StatusCode::FORBIDDEN,
                    Json(json!({
                        "error": "forbidden"
                    })),
                )
            },
            | Self::BadRequest(s) => {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": s
                    })),
                )
            },
        }
        .into_response()
    }
}
