//! The `tiles` URL scheme: the web views load map imagery from the core's tile proxy.

use rocketviz_core::tiles::{TileError, TileProxy};
use tauri::http::{header, Response, StatusCode};

/// Serves `tiles://localhost/<provider>/<z>/<x>/<y>` (on Windows `http://tiles.localhost/...`) from the tile proxy.
pub async fn tile_response(proxy: &TileProxy, path: &str) -> Response<Vec<u8>> {
    let respond = |status: StatusCode, content_type: &str, max_age: u64, body: Vec<u8>| {
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, format!("max-age={max_age}"))
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(body)
            .expect("valid response")
    };
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let parsed = match parts.as_slice() {
        [provider, z, x, y] => {
            match (z.parse(), x.parse(), y.trim_end_matches(|c: char| !c.is_ascii_digit()).parse()) {
                (Ok(z), Ok(x), Ok(y)) => Some((*provider, z, x, y)),
                _ => None,
            }
        }
        _ => None,
    };
    let Some((provider, z, x, y)) = parsed else {
        return respond(StatusCode::BAD_REQUEST, "text/plain", 0, format!("bad tile path {path}").into_bytes());
    };
    match proxy.tile(provider, z, x, y).await {
        Ok(t) => respond(StatusCode::OK, &t.content_type, t.max_age, t.bytes),
        Err(e) => {
            let status = match e {
                TileError::UnknownProvider(_) | TileError::OutOfRange { .. } => StatusCode::NOT_FOUND,
                TileError::Upstream { status: 404, .. } => StatusCode::NOT_FOUND,
                _ => StatusCode::BAD_GATEWAY,
            };
            log_tile_error(&e);
            respond(status, "text/plain", 0, e.to_string().into_bytes())
        }
    }
}

fn log_tile_error(e: &TileError) {
    eprintln!("rocketviz tiles: {e}");
}
