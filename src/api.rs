use crate::base16::{base16_decode, base16_encode};
use crate::fileutil::{get_full_of_file, get_part_of_file};
use crate::AppContext;
use crate::JsonHelper;

use anyhow::anyhow;
use axum::{
    extract::{ConnectInfo, Extension, Path},
    http::{
        header::{HeaderMap, HeaderName, HeaderValue},
        StatusCode,
    },
    routing::get,
    AddExtensionLayer, Router,
};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;

pub(crate) fn api(context: Arc<AppContext>) -> Router {
    Router::new()
        .route("/download/:download", get(download_file))
        .layer(AddExtensionLayer::new(context))
}
async fn download_file(
    Extension(context): Extension<Arc<AppContext>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(params): Path<String>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    use std::path::Path;
    use tracing::info;
    //debug!("params={} from {}", params, addr);
    if let Ok(params) = base16_decode(&params) {
        let config = &context.config.clone();
        let params: Value = serde_json::from_str(&params).unwrap();
        let catalog = params["catalog"].str("tcsoftV6");
        let path = config[catalog]["path"].str("download");
        let file = params["file"].str("");
        let skip = params["skip"].u64(0);
        let take = params["take"].u64(0);

        let bytes = if !file.is_empty() {
            let file_name = String::from(path) + "/" + file;
            if file == "filelist.txt" {
                info!("from {:?}, download {}", addr, catalog);
            }
            let path = Path::new(&file_name);
            if path.exists() {
                if take == 0 {
                    get_full_of_file(&file_name).await
                } else {
                    get_part_of_file(&file_name, skip, take).await
                }
            } else {
                Err(anyhow!("{} does not exist", file_name))
            }
        } else {
            Err(anyhow!("file name not provided"))
        };
        match bytes {
            Ok((skip, take, bytes)) => {
                let mut headers = HeaderMap::new();
                headers.insert(HeaderName::from_static("x-skip"), HeaderValue::from(skip)); // .header("x-skip", skip)
                headers.insert(HeaderName::from_static("x-take"), HeaderValue::from(take)); // .header("x-take", take)
                headers.insert(
                    HeaderName::from_static("content-type"),
                    HeaderValue::from_static("application/octet-stream"),
                ); //.header("content-type", "application/octet-stream")
                (StatusCode::OK, headers, bytes)
            }
            Err(e) => bad_response(&format!("Error：{:?}", e)),
        }
    } else {
        bad_response(&format!(
            "Error：download file fail, expect base16 encoded string as param,for example: {}, but get param: {} ",
            base16_encode(r#"{"catalog":"tcsoftV6","file":"filelist.txt"}"#).unwrap(),
            params
        ))
    }
}
fn bad_response(msg: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-body-is-error"),
        HeaderValue::from_static("yes"),
    ); //.header("x-body-is-error", "yes")
    headers.insert(
        HeaderName::from_static("content-type"),
        HeaderValue::from_static("text/plain;charset=utf-8"),
    ); //.header("content-type", "text/plain;charset=utf-8")
    (StatusCode::NOT_ACCEPTABLE, headers, Vec::<u8>::from(msg))
}
