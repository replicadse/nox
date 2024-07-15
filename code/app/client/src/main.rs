use std::{
    fs::File,
    io::BufReader,
    sync::Arc,
};

use futures_util::StreamExt;
use rustls::client::WebPkiServerVerifier;
use tokio_rustls::rustls::{
    pki_types::PrivateKeyDer,
    ClientConfig,
    RootCertStore,
};

#[forbid(unsafe_code)]
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    if Ok("1".to_owned()) == std::env::var("APP_DOTENV") {
        dotenv::dotenv().unwrap();
    }

    let mut ca_cert_reader = BufReader::new(File::open("../cert/gen/ca/ca.crt").unwrap());
    let ca_certs: Vec<tokio_rustls::rustls::pki_types::CertificateDer<'_>> =
        rustls_pemfile::certs(&mut ca_cert_reader).into_iter().filter_map(|cert| cert.ok()).collect();
    let mut root_cert_store = RootCertStore::empty();
    for c in ca_certs {
        root_cert_store.add(c)?;
    }

    let mut v = String::new();
    std::io::stdin().read_line(&mut v).unwrap();
    let v = v.trim();
    dbg!(&v);

    let mut client_cert_reader = BufReader::new(File::open(format!("../cert/gen/client/{}/cert/client.crt", v)).unwrap());
    let client_certs: Vec<tokio_rustls::rustls::pki_types::CertificateDer<'_>> =
        rustls_pemfile::certs(&mut client_cert_reader).into_iter().filter_map(|cert| cert.ok()).collect();
    let mut client_key_reader = BufReader::new(File::open(format!("../cert/gen/client/{}/cert/client.key", v)).unwrap());
    let client_keys: Vec<PrivateKeyDer> = rustls_pemfile::pkcs8_private_keys(&mut client_key_reader)
        .into_iter()
        .filter_map(|cert| cert.ok())
        .map(|v| PrivateKeyDer::Pkcs8(v))
        .collect();

    let config = ClientConfig::builder()
        .with_webpki_verifier(WebPkiServerVerifier::builder(Arc::new(root_cert_store)).build().unwrap())
        .with_client_auth_cert(client_certs, client_keys[0].clone_key())
        .unwrap();

    let url = "wss://localhost:8080/ws";
    let connector = tokio_tungstenite::Connector::Rustls(Arc::new(config));
    let (ws_stream, _) = tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(connector)).await?;
    let (_, mut read) = ws_stream.split();
    while let Some(c) = read.next().await {
        println!("{:?}", c);
    }

    Ok(())
}
