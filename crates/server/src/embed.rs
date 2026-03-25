use axum::{
    body::Body,
    http::{header, Response, StatusCode, Uri},
    response::IntoResponse,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/build/"]
#[prefix = "/"]
pub struct WebAssets;

pub async fn serve_static(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    match WebAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            // SPA fallback: serve index.html for unknown paths
            match WebAssets::get("index.html") {
                Some(index) => Response::builder()
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(index.data))
                    .unwrap(),
                None => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("not found"))
                    .unwrap(),
            }
        }
    }
}
