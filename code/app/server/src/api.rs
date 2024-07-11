use {
    crate::{
        AppState,
        ServerError,
    },
    axum::{
        extract::State,
        http::StatusCode,
        response::Json,
    },
    serde_json::json,
};

pub async fn any_debug(
    State(_state): State<AppState>,
) -> std::result::Result<(StatusCode, Json<serde_json::Value>), ServerError> {
    Ok((
        StatusCode::OK,
        Json(json!({
            "ok": true,
        })),
    ))
}
