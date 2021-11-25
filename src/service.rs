use crate::fileutil::{get_full_of_file, get_part_of_file};
use crate::base16::{base16_decode, base16_encode};
use crate::AppContext;
use crate::JsonHelper;

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;
use warp::http::{self, Response, StatusCode};
use warp::Filter;
use warp::{Rejection, Reply};

pub(crate) fn api(
    context: Arc<AppContext>,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    let ctx = context.clone();
    let get_download = warp::get()
        .map(move || ctx.clone())
        .and(warp::addr::remote())
        .and(warp::path!("download" / String))
        .and_then(download_file);
    get_download
}

async fn download_file(
    context: Arc<AppContext>,
    addr: Option<SocketAddr>,
    params: String,
) -> Result<impl Reply, Infallible> {
    use std::path::Path;
    let res = if let Ok(params) = base16_decode(&params) {
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
                Err(anyhow!("{} does not exist",file_name))
            }
        } else {
            Err(anyhow!("file name not provided"))
        };
        match bytes {
            Ok((skip, take, bytes)) => Response::builder()
                .header("x-skip", skip)
                .header("x-take", take)
                .header("content-type", "application/octet-stream")
                .body(bytes),
            Err(e) => bad_response(&format!("Error：{:?}", e)),
        }
    } else {
        bad_response(&format!(
            "Error：download file fail, expect base16 encoded string as param,for example: {}, but get param: {} ",
            base16_encode(r#"{"catalog":"tcsoftV6","file":"filelist.txt"}"#).unwrap(),
            params
        ))
    };
    Ok(res.unwrap())
}
fn bad_response(msg: &str) -> http::Result<Response<Vec<u8>>> {
    Response::builder()
        .header("x-body-is-error", "yes")
        .header("content-type", "text/plain;charset=utf-8")
        .status(StatusCode::NOT_ACCEPTABLE)
        .body(Vec::<u8>::from(msg))
}
