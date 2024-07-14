use {
    futures_util::{
        SinkExt,
        StreamExt,
    },
    std::{
        fs::File,
        io::BufReader,
        sync::Arc,
    },
    tokio_rustls::rustls::{
        pki_types::PrivateKeyDer,
        ClientConfig,
        RootCertStore,
    },
    tokio_tungstenite::tungstenite::Message,
};

#[forbid(unsafe_code)]
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    if Ok("1".to_owned()) == std::env::var("APP_DOTENV") {
        dotenv::dotenv().unwrap();
    }

    let mut ca_cert_reader = BufReader::new(File::open("../cert/ca/ca.crt").unwrap());
    let ca_certs: Vec<tokio_rustls::rustls::pki_types::CertificateDer<'_>> = rustls_pemfile::certs(&mut ca_cert_reader)
        .into_iter()
        .filter_map(|cert| cert.ok())
        .collect();
    let mut root_cert_store = RootCertStore::empty();
    for c in ca_certs {
        root_cert_store.add(c)?;
    }

    let mut client_cert_reader = BufReader::new(File::open("../cert/client/cert/client.crt").unwrap());
    let client_certs: Vec<tokio_rustls::rustls::pki_types::CertificateDer<'_>> =
        rustls_pemfile::certs(&mut client_cert_reader)
            .into_iter()
            .filter_map(|cert| cert.ok())
            .collect();
    let mut client_key_reader = BufReader::new(File::open("../cert/client/cert/client.key").unwrap());
    let client_keys: Vec<PrivateKeyDer> = rustls_pemfile::pkcs8_private_keys(&mut client_key_reader)
        .into_iter()
        .filter_map(|cert| cert.ok())
        .map(|v| PrivateKeyDer::Pkcs8(v))
        .collect();

    let config = ClientConfig::builder()
        .with_root_certificates(Arc::new(root_cert_store))
        .with_client_auth_cert(client_certs, client_keys[0].clone_key())
        .unwrap();

    let url = "wss://localhost:8080/mailbox/alice/ws";
    let connector = tokio_tungstenite::Connector::Rustls(Arc::new(config));
    let (ws_stream, _) = tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(connector)).await?;
    let (mut write, mut read) = ws_stream.split();

    // Send a message to the server
    write.send(Message::Text("Hello, WebSocket server!".into())).await?;
    while let Some(c) = read.next().await {
        println!("{:?}", c);
    }

    Ok(())
}
