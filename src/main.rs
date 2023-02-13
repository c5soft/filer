mod config;
mod context;
mod fileutil;
mod json_helper;
mod log;

#[cfg(feature = "server")]
mod api;
#[cfg(any(feature = "server", feature = "download"))]
mod base16;
#[cfg(feature = "download")]
mod download;

#[cfg(feature = "server")]
mod static_files;

#[cfg(any(feature = "server", feature = "download"))]
mod addr;
#[cfg(feature = "xcopy")]
mod xcopy;
#[cfg(any(feature = "server", feature = "download"))]
use axum::Router;
#[cfg(any(feature = "server", feature = "download"))]
use std::sync::Arc;

use anyhow::{anyhow, Result};
use clap::{arg, command, value_parser, ArgAction, ArgMatches};
use context::AppContext;
use json_helper::JsonHelper;
use serde_json::Value;
use std::path::PathBuf;
use tokio::time::Instant;
use tracing::debug;

#[cfg(feature = "index")]
use fileutil::refresh_dir_files_digest;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> Result<()> {
    let args = args();
    let context = if let Some(config_file) = args.get_one::<PathBuf>("config") {
        AppContext::from(config_file)
    } else {
        AppContext::new()
    };
    let log_file_name = context.config["log_file_name"].string("");
    log::init_logger(&log_file_name).map_err(|e| anyhow!("init_logger error {:?}", e))?;

    let cpus = num_cpus::get() as u64;
    let time_start = Instant::now();

    let get_flag_repeat = args.get_flag("repeat");

    let catalog = args
        .get_one::<String>("catalog")
        .map(|x| x.as_str())
        .unwrap_or_default();

    debug!("catalog = {:#?}", catalog);

    #[cfg(feature = "index")]
    if args.get_flag("index") || get_flag_repeat {
        let config = context.config[catalog].clone();
        if config.is_null() {
            println!("catalog {} not found in config", catalog);
        } else {
            let part_size = config["part_size"].u64(102400u64);
            let max_tasks = config["max_tasks"].u64(cpus * 2);
            let path = config["path"].str("");
            if path.is_empty() {
                println!("path not provided in catalog {}", catalog)
            } else {
                refresh_dir_files_digest(
                    path,
                    "filelist.txt",
                    part_size,
                    max_tasks,
                    get_flag_repeat,
                )
                .await?;
            }
        }
    }
    #[cfg(feature = "xcopy")]
    if args.get_flag("xcopy") {
        let config = context.config.clone();
        let source_path = args
            .get_one::<String>("source_path")
            .map(|x| x.as_str())
            .unwrap_or_default();
        let target_path = args
            .get_one::<String>("target_path")
            .map(|x| x.as_str())
            .unwrap_or_default();
        if source_path.is_empty() || target_path.is_empty() {
            println!("Usage: filer --xcopy source_path target_path")
        } else {
            xcopy::xcopy_files(&config, source_path, target_path, cpus * 2).await?;
        }
    }
    if args.get_flag("server") {
        #[cfg(feature = "server")]
        server(&context).await;
        #[cfg(not(feature = "server"))]
        println!("run as server not suported");
    } else if args.get_flag("download") || args.get_flag("update") {
        #[cfg(feature = "download")]
        download::download_files(
            &context.config,
            args.get_flag("download"),
            cpus * 4,
            catalog,
        )
        .await?;
        #[cfg(not(feature = "download"))]
        println!("download/update not suported");
    }
    let pcpus = num_cpus::get_physical() as u64;
    println!(
        "Time taken: {}\nNumber of CPU cores: {}x{}",
        time_taken(time_start),
        pcpus,
        cpus / pcpus
    );
    Ok(())
}

#[cfg(feature = "server")]
async fn server(context: &Arc<AppContext>) {
    let server_config = context.config["server"].clone();

    let static_path = server_config["static_path"].string("public");
    let cache_age_in_minute: i32 = server_config["static_cache_age_in_minute"].i64(30) as i32;

    let ctx = context.clone();
    let app = Router::new()
        .nest("/api", api::api(ctx))
        .fallback_service(static_files::make_service(static_path, cache_age_in_minute));

    let http_server = tokio::spawn(start_server(server_config.clone(), false, app.clone()));
    let https_server = tokio::spawn(start_server(server_config, true, app));
    let (_, _) = tokio::join!(http_server, https_server);
}

#[cfg(feature = "server")]
async fn start_server(config: Value, is_https: bool, app: Router) {
    use axum_server::tls_rustls::RustlsConfig;
    use chrono::Local;
    use std::net::SocketAddr;
    let server_name = config["server_name"].string("W3");
    let protocol = if is_https { "HTTPS" } else { "HTTP" };
    let config_addr = addr::Addr::new(&config, is_https);
    let (is_active, addr) = config_addr.get();
    if is_active {
        let now = &Local::now().to_string()[0..19];
        println!(
            "{} {} server version {} started at {} listening on {}",
            server_name, protocol, VERSION, now, &config_addr
        );
        let app = app.into_make_service_with_connect_info::<SocketAddr>();
        let server = if is_https {
            let tls_config = RustlsConfig::from_pem_file("server.cer", "server.key")
                .await
                .unwrap();
            axum_server::bind_rustls(addr, tls_config).serve(app).await
        } else {
            axum_server::bind(addr).serve(app).await
        };
        server.unwrap();
    } else {
        println!(
            "{} {} server version {} is not active !",
            server_name, protocol, VERSION
        );
    }
}

fn args() -> ArgMatches {
    let app = command!().arg(
        arg!(-C --config <CONFIG> "set config file")
            .default_value("filer.json")
            .value_parser(value_parser!(PathBuf)),
    );

    #[cfg(any(feature = "server", feature = "index", feature = "download"))]
    let app =
        app.arg(arg!(-c --catalog <CATALOG> "set catalog in config").default_value("tcsoftV6"));

    #[cfg(feature = "index")]
    let app = app
        .arg(arg!(-i --index "generate the filelist.txt which contains a list of file hash,size,name").action(ArgAction::SetTrue))
        .arg(arg!(-r --repeat "list repeated files while indexing").action(ArgAction::SetTrue));

    #[cfg(feature = "xcopy")]
    let app = app
        .arg(
            arg!(-x --xcopy "xcopy file(s)" )
                .action(ArgAction::SetTrue)
                .conflicts_with("server")
                .conflicts_with("download")
                .conflicts_with("update"),
        )
        .arg(arg!([source_path] "Sets the XCopy source path or file")) //.index(1))
        .arg(arg!([target_path] "Sets the XCopy target path")); //.index(2));

    #[cfg(feature = "server")]
    let app = app.arg(
        arg!(-s --server "run as file distribution server")
            .action(ArgAction::SetTrue)
            .conflicts_with("download")
            .conflicts_with("update"),
    );

    #[cfg(feature = "download")]
    let app = app
        .arg(
            arg!(-d --download "run as file download client")
                .action(ArgAction::SetTrue)
                .conflicts_with("server")
                .conflicts_with("update"),
        )
        .arg(
            arg!(-u --update "run as file update client")
                .conflicts_with("server")
                .conflicts_with("download"),
        );
    app.get_matches()
}

fn time_taken(start_time: Instant) -> String {
    let dur = Instant::now() - start_time;
    let dur: f32 = dur.as_secs_f32();
    const F60: f32 = 60f32;
    if dur > F60 * F60 {
        let h = (dur / (F60 * F60)).round();
        let m = ((dur - h * F60 * F60) / F60).round();
        let s = dur - m * F60;
        format!("{}h{}m{:.2}s", h as i32, m as i32, s)
    } else if dur > F60 {
        let m = (dur / F60).round();
        let s = dur - m * F60;
        format!("{}m{:.2}s", m as i32, s)
    } else {
        format!("{:.2}s", dur)
    }
}
