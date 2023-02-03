mod config;
mod context;
mod fileutil;
mod json_helper;

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

use anyhow::Result;
use clap::{Arg, ArgMatches, Command};
use context::AppContext;
use json_helper::JsonHelper;
use serde_json::Value;
use tokio::time::Instant;

#[cfg(feature = "digest")]
use fileutil::refresh_dir_files_digest;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = args();
    let context = if let Some(config_file) = args.get_one::<String>("config") {
        AppContext::from(config_file.into())
    } else {
        AppContext::new()
    };
    let cpus = num_cpus::get() as u64;
    let time_start = Instant::now();

    let show_repeat = args.contains_id("repeat");

    let default_catalog: String = String::from("tcsoftV6");
    let empty_str: String = String::from("");

    #[cfg(feature = "digest")]
    if args.contains_id("digest") || show_repeat {
        let catalog = args
            .get_one::<String>("catalog")
            .unwrap_or(&default_catalog);
        let config = context.config[catalog].clone();
        let part_size = config["part_size"].u64(102400u64);
        let max_tasks = config["max_tasks"].u64(cpus * 2);
        let path = config["path"].str("d:/tcsoftV6");
        refresh_dir_files_digest(path, "filelist.txt", part_size, max_tasks, show_repeat).await?;
    }
    #[cfg(feature = "xcopy")]
    if args.contains_id("xcopy") {
        let config = context.config.clone();
        let source_path = args.get_one::<String>("source_path").unwrap_or(&empty_str);
        let target_path = args.get_one::<String>("target_path").unwrap_or(&empty_str);
        if source_path.is_empty() || target_path.is_empty() {
            println!("Usage: filer --xcopy source_path target_path")
        } else {
            xcopy::xcopy_files(&config, source_path, target_path, cpus * 2).await?;
        }
    }
    if args.contains_id("server") {
        println!();
        #[cfg(feature = "server")]
        server(&context).await;
    } else if args.contains_id("download") || args.contains_id("update") {
        let catalog = args
            .get_one::<String>("catalog")
            .unwrap_or(&default_catalog);
        #[cfg(feature = "download")]
        download::download_files(
            &context.config,
            args.contains_id("download"),
            cpus * 4,
            catalog,
        )
        .await?;
        println!();
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
    let app = Command::new("Filer 文件传输系统")
        .version(VERSION)
        .author("xander.xiao@gmail.com")
        .about("极速文件分发、拷贝工具")
        .mut_arg("version", |a| a.help(Some("显示版本号")))
        .mut_arg("help", |a| a.help(Some("显示帮助信息")))
        .arg(
            Arg::new("config")
                .help("指定配置文件")
                .short('C')
                .long("config")
                .value_name("config")
                .takes_value(true)
                .default_value("filer.json"),
        );

    #[cfg(any(feature = "server", feature = "calc_digest", feature = "download"))]
    let app = app.arg(
        Arg::new("catalog")
            .help("指定分发目录")
            .short('c')
            .long("catalog")
            .value_name("catalog")
            .takes_value(true)
            .default_value("tcsoftV6"),
    );

    #[cfg(feature = "digest")]
    let app = app.arg(
        Arg::new("digest")
            .help("刷新文件列表，计算文件的哈希值")
            .short('i')
            .long("index"),
    );

    #[cfg(feature = "digest")]
    let app = app.arg(
        Arg::new("repeat")
            .help("刷新文件哈希值列表时，列出重复文件")
            .short('r')
            .long("repeat"),
    );

    #[cfg(feature = "xcopy")]
    let app = app
        .arg(
            Arg::new("xcopy")
                .help("复制文件夹或文件")
                .short('x')
                .long("xcopy"),
        )
        .arg(
            Arg::new("source_path")
                .help("Sets the XCopy source path or file")
                .index(1),
        )
        .arg(
            Arg::new("target_path")
                .help("Sets the XCopy target path")
                .index(2),
        );

    #[cfg(feature = "server")]
    let app = app.arg(
        Arg::new("server")
            .help("作为服务器启动文件服务")
            .short('s')
            .long("server")
            .conflicts_with("download")
            .conflicts_with("update"),
    );

    #[cfg(feature = "download")]
    let app = app
        .arg(
            Arg::new("download")
                .help("作为客户端下载所有文件")
                .short('d')
                .long("download")
                .conflicts_with("server")
                .conflicts_with("update"),
        )
        .arg(
            Arg::new("update")
                .help("作为客户端下载更新文件")
                .short('u')
                .long("update")
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
