use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    Extension,
};
use serde_json::json;

use crate::{
    AppState,
    ServerError,
};

pub async fn any_debug(
    State(_state): State<AppState>,
    Extension(certs): Extension<Vec<rustls::pki_types::CertificateDer<'_>>>,
) -> std::result::Result<(StatusCode, Json<serde_json::Value>), ServerError> {
    tracing::info!(message = format!("certs {:?}", certs));
    Ok((
        StatusCode::OK,
        Json(json!({
            "ok": true,
        })),
    ))
}
