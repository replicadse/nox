use {
    axum::{
        body::Body,
        extract::{
            ws::{
                Message,
                WebSocket,
            },
            Path,
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
        Router,
    },
    axum_server::tls_rustls::RustlsConfig,
    futures_util::{
        stream::SplitSink,
        SinkExt,
        StreamExt,
    },
    rustls::{
        server::AllowAnyAuthenticatedClient,
        Certificate,
        PrivateKey,
        RootCertStore,
        ServerConfig,
    },
    serde_json::json,
    std::{
        collections::HashMap,
        fs::File,
        io::BufReader,
        net::SocketAddr,
        str::FromStr,
        sync::Arc,
    },
    tokio::sync::{
        Mutex,
        RwLock,
    },
    tower_http::{
        compression::CompressionLayer,
        cors::CorsLayer,
        trace::TraceLayer,
    },
};

mod api;

#[derive(Clone)]
pub struct AppConfig {
    pub debug: bool,
}
#[derive(Clone)]
pub struct AppData {
    pub connections: Arc<RwLock<HashMap<String, Mutex<(SocketAddr, SplitSink<WebSocket, Message>)>>>>,
}
#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub data: AppData,
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

    let mut ca_cert_reader = BufReader::new(File::open("../cert/ca/ca.crt")?);
    let ca_certs: Vec<Certificate> = rustls_pemfile::certs(&mut ca_cert_reader)
        .into_iter()
        .filter_map(|cert| cert.ok())
        .map(|v| Certificate(v.to_vec()))
        .collect();
    let mut root_cert_store = RootCertStore::empty();
    for c in &ca_certs {
        root_cert_store.add(c)?;
    }

    let mut server_cert_reader = BufReader::new(File::open("../cert/server/server.crt")?);
    let server_certs: Vec<Certificate> = rustls_pemfile::certs(&mut server_cert_reader)
        .into_iter()
        .filter_map(|cert| cert.ok())
        .map(|v| Certificate(v.to_vec()))
        .collect();
    let mut server_key_reader = BufReader::new(File::open("../cert/server/server.key")?);
    let server_keys: Vec<tokio_rustls::rustls::pki_types::PrivatePkcs8KeyDer<'_>> =
        rustls_pemfile::pkcs8_private_keys(&mut server_key_reader)
            .into_iter()
            .filter_map(|cert| cert.ok())
            .collect();

    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_client_cert_verifier(Arc::new(AllowAnyAuthenticatedClient::new(root_cert_store)))
        .with_single_cert(server_certs, PrivateKey(server_keys[0].secret_pkcs8_der().to_vec()))?;
    let config = RustlsConfig::from_config(Arc::new(config));

    let app = Router::new()
        .route("/mailbox/:mailbox_id/ws", get(ws_handler).with_state(state.clone()))
        .route("/debug", any(crate::api::any_debug).with_state(state.clone()))
        .layer(CompressionLayer::new().gzip(true).deflate(true));
    let app = if state.config.debug {
        app.layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::AllowOrigin::mirror_request())
                .allow_credentials(true)
                .allow_methods(tower_http::cors::AllowMethods::mirror_request())
                .allow_headers(tower_http::cors::AllowHeaders::mirror_request()),
        )
    } else {
        app.layer(
            TraceLayer::new_for_http().on_request(|r: &Request<Body>, _: &tracing::Span| {
                tracing::info!(message = format!("{:?}", r));
            }),
        )
    };

    let svc = tower::ServiceBuilder::new().service(app);
    axum_server::bind_rustls(SocketAddr::from_str("0.0.0.0:8080").unwrap(), config)
        .serve(svc.into_make_service_with_connect_info::<SocketAddr>())
        .await?;

    Ok(())
}

async fn ws_handler(
    State(state): State<AppState>,
    Path(mailbox_id): Path<String>,
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::connect_info::ConnectInfo(addr): axum::extract::connect_info::ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    tracing::info!(message = format!("websocket connection from {:?}", addr));
    ws.on_upgrade(move |socket| handle_socket(socket, mailbox_id, addr, state))
}

async fn handle_socket<'a>(socket: WebSocket, mailbox: String, addr: SocketAddr, state: AppState) {
    let (send, mut recv) = socket.split();
    state
        .data
        .connections
        .write()
        .await
        .insert(mailbox.clone(), Mutex::new((addr, send)));

    while let Some(msg) = recv.next().await {
        match msg {
            | Ok(Message::Text(text)) => {
                tracing::info!(message = format!("message received from {:?}: {:?}", addr, text));
                let lock = state.data.connections.read().await;
                match lock.get("alice") {
                    | Some(v) => v.lock().await.1.send(Message::Text(text)).await.unwrap(),
                    | None => (),
                }
            },
            | Ok(Message::Ping(ping)) => {
                tracing::info!(message = format!("ping from: {:?}", addr));
                let lock = state.data.connections.read().await;
                if lock
                    .get(&mailbox)
                    .unwrap()
                    .lock()
                    .await
                    .1
                    .send(Message::Pong(ping))
                    .await
                    .is_err()
                {
                    break;
                }
            },
            | Ok(Message::Pong(_)) => {
                tracing::info!(message = format!("pong received from {:?}", addr));
            },
            | Ok(Message::Close(_)) => {
                tracing::info!(message = format!("connection closed: {:?}", addr));
                let mut lock = state.data.connections.write().await;
                lock.remove(&mailbox);
                break;
            },
            | Err(e) => {
                tracing::info!(message = format!("websocket error: {:?}", e));
                let mut lock = state.data.connections.write().await;
                lock.remove(&mailbox);
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
