use std::error::Error;
use std::{fs::File, sync::Mutex};
use time::{macros::format_description, UtcOffset};
use tracing_subscriber::{fmt::time::OffsetTime, EnvFilter};

pub fn init_logger(log_file_name: &str) -> Result<(), Box<dyn Error>> {
    let local_time = OffsetTime::new(
        UtcOffset::from_hms(8, 0, 0)?,
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]"),
    );
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_timer(local_time);
    if !log_file_name.is_empty() {
        let now = format!("{}", chrono::Local::now())
            .replace(':', "")
            .replace(' ', "_");
        let log_file_name = format!("{}.{}.log", log_file_name, &now[..24]);
        println!("Log to file: {}", log_file_name);

        let log_file = File::create(log_file_name)?;
        subscriber
            .with_ansi(false)
            .with_writer(Mutex::new(log_file))
            .init();
    } else {
        subscriber.init();
    };

    Ok(())
}
